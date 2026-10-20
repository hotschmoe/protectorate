package hub

import (
	"context"
	"errors"
	"fmt"
	"log"
	"path/filepath"
	"sort"
	"sync"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/driver"
	"github.com/hotschmoe/protectorate/internal/orchestrator"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

const (
	defaultEnvoyTTL = 720 * time.Hour
	// envoyWatchEvery is how often the hub checks Envoy's token and hosts.md.
	envoyWatchEvery = time.Minute
	// envoyGrace is how long a replaced token keeps working, so a CLI call
	// that read the old token file just before the swap still succeeds.
	envoyGrace = 5 * time.Minute
	// previousEnvoyToken is the token name from before Envoy was renamed.
	previousEnvoyToken = "orchestrator-poe"
)

// EnvoyConfig is what the hub cannot derive itself: the CLI reference and
// the MCP tool catalog that AGENTS.md is generated from. Both come from the
// binary's command tree, which lives in cmd/.
type EnvoyConfig struct {
	Commands []orchestrator.Command
	Tools    []orchestrator.Tool
}

// SetEnvoy enables SpawnEnvoy and the token renewal. Call after SetSleeves
// and before Handler or Serve.
func (h *Hub) SetEnvoy(c EnvoyConfig) { h.envoy = &c }

// StartEnvoyIfMissing starts Envoy on harness when the hub has no record of
// it. An existing record is left alone, running or not: its next message
// resumes its session, so a boot never costs a prompt cache by itself.
func (h *Hub) StartEnvoyIfMissing(ctx context.Context, harness pb.Harness) error {
	if h.sleeves == nil {
		return nil
	}
	if _, err := h.sleeves.Get(orchestrator.SleeveName); err == nil {
		return nil
	}
	owner := auth.WithPrincipal(ctx, &auth.Principal{Owner: true})
	if _, err := h.SpawnEnvoy(owner, connect.NewRequest(&pb.SpawnEnvoyRequest{Harness: harness})); err != nil {
		return fmt.Errorf("start envoy: %w", err)
	}
	return nil
}

type envoyState struct {
	mu sync.Mutex
}

type envoySpawnKey struct{}

// spawningEnvoy reports whether ctx comes from SpawnEnvoy, the only caller
// allowed to spawn the orchestrator's sleeve.
func spawningEnvoy(ctx context.Context) bool {
	v, _ := ctx.Value(envoySpawnKey{}).(bool)
	return v
}

func rpcErr(err error) error {
	var ce *connect.Error
	if errors.As(err, &ce) {
		return connect.NewError(ce.Code(), err)
	}
	return connect.NewError(connect.CodeInternal, err)
}

// SpawnEnvoy runs orchestrator.Spawn against the hub's own handlers, with the
// caller's (owner) context, so the steps are the ones the CLI used to drive
// over HTTP.
func (h *Hub) SpawnEnvoy(ctx context.Context, req *connect.Request[pb.SpawnEnvoyRequest]) (*connect.Response[pb.SpawnEnvoyResponse], error) {
	if h.sleeves == nil || h.envoy == nil {
		return nil, connect.NewError(connect.CodeFailedPrecondition, errors.New("this hub runs without sleeves or the CLI reference, so it cannot start Envoy"))
	}
	m := req.Msg
	harness := "auto"
	switch m.Harness {
	case pb.Harness_HARNESS_UNSPECIFIED:
	case pb.Harness_HARNESS_SHELL:
		return nil, invalid("harness shell cannot run Envoy")
	default:
		harness = orchestrator.HarnessName(m.Harness)
	}
	ttl := defaultEnvoyTTL
	if m.Ttl != nil {
		if m.Ttl.CheckValid() != nil || m.Ttl.AsDuration() < 0 {
			return nil, invalid("invalid ttl")
		}
		ttl = m.Ttl.AsDuration()
	}
	scopes := m.Scopes
	if len(scopes) == 0 {
		scopes = orchestrator.DefaultScopes(h.envoy.Tools)
	}
	scopes = orchestrator.MergeScopes(scopes, m.AddScopes)
	if selectionFromProto(m.Selection) != (driver.Selection{}) {
		if m.Harness != pb.Harness_HARNESS_CODEX {
			return nil, invalid("choose Codex explicitly for model/effort selection")
		}
		if err := h.validateSelection(ctx, driver.Codex, selectionFromProto(m.Selection)); err != nil {
			return nil, err
		}
	}

	h.envoyState.mu.Lock()
	defer h.envoyState.mu.Unlock()
	res, err := orchestrator.Spawn(context.WithValue(ctx, envoySpawnKey{}, true), h, orchestrator.Options{
		Home: h.sleeves.StateDir(), HubURL: h.identityEnv()["PROTECTORATE_HUB"], Harness: harness,
		Scopes: scopes, TTL: ttl, Tools: h.envoy.Tools, Commands: h.envoy.Commands,
		Selection: m.Selection,
	})
	if err != nil {
		return nil, rpcErr(err)
	}
	auth.Annotate(ctx, orchestrator.SleeveName, fmt.Sprintf("harness=%s replaced=%t", res.Harness, res.Replaced))
	return connect.NewResponse(&pb.SpawnEnvoyResponse{
		Sleeve: res.Sleeve, Token: res.Token, Workspace: res.Workspace, Reason: res.Reason, Replaced: res.Replaced,
	}), nil
}

// planEnvoyRenewal decides, from the token list alone, what to do about
// Envoy's token: which token to replace because it nears its end, and which
// older ones to revoke because their replacement is past its grace period.
func planEnvoyRenewal(tokens []auth.Token, now time.Time) (rotate *auth.Token, revoke []auth.Token) {
	var live []auth.Token
	for _, t := range tokens {
		if t.Name == orchestrator.TokenName && t.RevokedAt == nil {
			live = append(live, t)
		}
	}
	if len(live) == 0 {
		return nil, nil
	}
	sort.SliceStable(live, func(i, j int) bool { return live[i].Created.Before(live[j].Created) })
	newest := live[len(live)-1]
	for _, t := range live[:len(live)-1] {
		if !now.Before(newest.Created.Add(envoyGrace)) {
			revoke = append(revoke, t)
		}
	}
	if newest.Expires != nil {
		life := newest.Expires.Sub(newest.Created)
		if newest.Expires.Sub(now) <= life/4 {
			rotate = &newest
		}
	}
	return rotate, revoke
}

// swapEnvoyToken mints a token like old (same scopes) that lives ttl, and
// writes it into Envoy's token file. The running harness is not touched: the
// CLI reads the file on every call.
func (h *Hub) swapEnvoyToken(info sleeve.Info, scopes []string, ttl time.Duration, now time.Time) (auth.Token, error) {
	st := h.authz.Store()
	t, secret, err := st.Renew(auth.Token{Name: orchestrator.TokenName, Scopes: scopes}, ttl, now)
	if err != nil {
		return auth.Token{}, fmt.Errorf("mint envoy token: %w", err)
	}
	if err := orchestrator.WriteTokenFile(filepath.Join(info.Workspace, orchestrator.TokenFile), secret); err != nil {
		if _, rerr := st.Revoke(t.ID, now); rerr != nil {
			log.Printf("envoy: revoke unused token %s: %v", t.ID, rerr)
		}
		return auth.Token{}, err
	}
	return t, nil
}

// renewEnvoy rotates Envoy's token in place when it nears its end, revokes
// replaced tokens after their grace period, and keeps hosts.md current.
func (h *Hub) renewEnvoy(now time.Time) error {
	if h.authz == nil || h.authz.Store() == nil || h.sleeves == nil {
		return nil
	}
	h.envoyState.mu.Lock()
	defer h.envoyState.mu.Unlock()
	st := h.authz.Store()
	rotate, revoke := planEnvoyRenewal(st.List(), now)
	for _, t := range revoke {
		if _, err := st.Revoke(t.ID, now); err != nil {
			return fmt.Errorf("revoke replaced envoy token %s: %w", t.ID, err)
		}
	}
	info, err := h.sleeves.Get(orchestrator.SleeveName)
	if err != nil {
		return nil
	}
	if rotate != nil {
		t, err := h.swapEnvoyToken(info, rotate.Scopes, rotate.Expires.Sub(rotate.Created), now)
		if err != nil {
			return err
		}
		log.Printf("envoy: token %s renewed as %s", rotate.ID, t.ID)
	}
	if err := orchestrator.WriteHosts(info.Workspace, orchestrator.ReadHosts(h.sleeves.StateDir())); err != nil {
		return err
	}
	return nil
}

func (h *Hub) watchEnvoy(ctx context.Context) {
	tick := time.NewTicker(envoyWatchEvery)
	defer tick.Stop()
	for {
		if err := h.renewEnvoy(time.Now()); err != nil {
			log.Printf("envoy: %v", err)
		}
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
		}
	}
}

// migrateEnvoyTokens runs once the sleeves are loaded. Tokens of the previous
// orchestrator name are revoked; when the renamed sleeve still has no token
// of its own, a replacement with the same scopes and remaining life is
// written to its token file first, so its resumed session keeps working
// until the next respawn.
func (h *Hub) migrateEnvoyTokens(now time.Time) {
	if h.authz == nil || h.authz.Store() == nil {
		return
	}
	st := h.authz.Store()
	var old []auth.Token
	haveNew := false
	for _, t := range st.List() {
		switch {
		case t.RevokedAt != nil:
		case t.Name == previousEnvoyToken:
			old = append(old, t)
		case t.Name == orchestrator.TokenName:
			haveNew = true
		}
	}
	if len(old) == 0 {
		return
	}
	if info, err := h.sleeves.Get(orchestrator.SleeveName); err == nil && !haveNew {
		last := old[len(old)-1]
		ttl := time.Duration(0)
		usable := true
		if last.Expires != nil {
			ttl = last.Expires.Sub(now)
			usable = ttl > 0
		}
		if usable {
			if t, err := h.swapEnvoyToken(info, last.Scopes, ttl, now); err != nil {
				log.Printf("envoy: replace token %s: %v", last.ID, err)
			} else {
				log.Printf("envoy: token %s replaced by %s under the new name", last.ID, t.ID)
			}
		}
	}
	for _, t := range old {
		if _, err := st.Revoke(t.ID, now); err != nil {
			log.Printf("envoy: revoke token %s: %v", t.ID, err)
		}
	}
}
