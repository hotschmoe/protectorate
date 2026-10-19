package orchestrator

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

const (
	SleeveName = "poe"
	TokenName  = "orchestrator-poe"
	TokenFile  = ".protectorate-token"
	DirName    = "orchestrator"

	missingHosts = "# Hosts\n\nThe hub has not written an inventory yet. Run `protectorate node ls`.\n"
)

// Hub is the part of the hub API Spawn uses. Token management needs the
// owner token.
type Hub interface {
	protectoratev1connect.TokenServiceClient
	protectoratev1connect.SleeveServiceClient
	protectoratev1connect.QuotaServiceClient
}

// Options configure one spawn.
type Options struct {
	Home    string // $PROTECTORATE_HOME
	HubURL  string // reached from inside the sandbox
	Harness string // auto, claude, codex, grok, pi or shell
	Scopes  []string
	TTL     time.Duration
	Tools   []Tool
	// Commands feed the generated CLI reference.
	Commands []Command
	Now      func() time.Time
}

// Result describes the sleeve that was started.
type Result struct {
	Sleeve    *pb.Sleeve
	Token     *pb.TokenInfo
	Workspace string
	Harness   string
	Reason    string
	Replaced  bool
}

var harnessNames = map[string]pb.Harness{
	"claude": pb.Harness_HARNESS_CLAUDE, "codex": pb.Harness_HARNESS_CODEX, "grok": pb.Harness_HARNESS_GROK,
	"pi": pb.Harness_HARNESS_PI, "shell": pb.Harness_HARNESS_SHELL,
}

func harnessLabel(h pb.Harness) string {
	for k, v := range harnessNames {
		if v == h {
			return k
		}
	}
	return "?"
}

// Spawn creates or respawns the poe sleeve: it stops a running one, revokes
// every earlier poe token, mints a fresh scoped token, regenerates the
// workspace files and starts the harness.
func Spawn(ctx context.Context, hub Hub, o Options) (*Result, error) {
	if o.Now == nil {
		o.Now = time.Now
	}
	harness, reason, err := resolveHarness(ctx, hub, o.Harness)
	if err != nil {
		return nil, err
	}
	scopes := o.Scopes
	if len(scopes) == 0 {
		scopes = DefaultScopes(o.Tools)
	}

	ws := filepath.Join(o.Home, DirName)
	if err := os.MkdirAll(ws, 0o700); err != nil {
		return nil, fmt.Errorf("create %s: %w", ws, err)
	}

	res := &Result{Workspace: ws, Harness: harnessLabel(harness), Reason: reason}
	if cur, err := hub.GetSleeve(ctx, connect.NewRequest(&pb.GetSleeveRequest{Name: SleeveName})); err == nil {
		if cur.Msg.Sleeve.State == pb.SleeveState_SLEEVE_STATE_RUNNING {
			if _, err := hub.KillSleeve(ctx, connect.NewRequest(&pb.KillSleeveRequest{Name: SleeveName})); err != nil {
				return nil, fmt.Errorf("stop running %s: %w", SleeveName, err)
			}
			res.Replaced = true
		}
	} else if connect.CodeOf(err) != connect.CodeNotFound {
		return nil, fmt.Errorf("look up %s: %w", SleeveName, err)
	}
	if err := RevokeTokens(ctx, hub); err != nil {
		return nil, err
	}

	created, err := hub.CreateToken(ctx, connect.NewRequest(&pb.CreateTokenRequest{
		Name: TokenName, Scopes: scopes, Ttl: durationpb.New(o.TTL),
	}))
	if err != nil {
		return nil, fmt.Errorf("mint token: %w", err)
	}
	res.Token = created.Msg.Token
	fail := func(err error) (*Result, error) {
		hub.RevokeToken(ctx, connect.NewRequest(&pb.RevokeTokenRequest{Id: res.Token.Id}))
		return nil, err
	}

	tokenPath := filepath.Join(ws, TokenFile)
	if err := writeAtomic(tokenPath, []byte(created.Msg.Secret+"\n"), 0o600); err != nil {
		return fail(fmt.Errorf("write token file: %w", err))
	}
	expires := "never"
	if t := res.Token.ExpiresAt; t != nil {
		expires = t.AsTime().UTC().Format(time.RFC3339)
	}
	doc := RenderAgents(DocInput{
		Generated: o.Now(), Harness: res.Harness, Hub: o.HubURL,
		Token:    TokenSummary{ID: res.Token.Id, Name: res.Token.Name, Scopes: res.Token.Scopes, Expires: expires},
		Commands: o.Commands, Tools: o.Tools,
	})
	if err := WriteContext(ws, doc, readHosts(o.Home)); err != nil {
		return fail(err)
	}

	spawned, err := hub.SpawnSleeve(ctx, connect.NewRequest(&pb.SpawnSleeveRequest{
		Name: SleeveName, Workspace: ws, Harness: harness,
		Env: map[string]string{"PROTECTORATE_HUB": o.HubURL, "PROTECTORATE_TOKEN_FILE": tokenPath},
	}))
	if err != nil {
		return fail(fmt.Errorf("spawn %s: %w", SleeveName, err))
	}
	res.Sleeve = spawned.Msg.Sleeve
	return res, nil
}

func resolveHarness(ctx context.Context, hub protectoratev1connect.QuotaServiceClient, name string) (pb.Harness, string, error) {
	if name == "" || name == "auto" {
		sug, err := hub.Suggest(ctx, connect.NewRequest(&pb.SuggestRequest{}))
		if err != nil {
			return 0, "", fmt.Errorf("pick a harness: %w", err)
		}
		for _, s := range sug.Msg.Suggestions {
			if s.Recommended {
				return s.Harness, s.Reason, nil
			}
		}
		return 0, "", errors.New("no harness is recommended; pass --harness explicitly")
	}
	h, ok := harnessNames[name]
	if !ok {
		return 0, "", fmt.Errorf("unknown harness %q, want auto, claude, codex, grok or pi", name)
	}
	return h, "chosen explicitly", nil
}

// RevokeTokens revokes every unrevoked token named TokenName.
func RevokeTokens(ctx context.Context, hub protectoratev1connect.TokenServiceClient) error {
	list, err := hub.ListTokens(ctx, connect.NewRequest(&pb.ListTokensRequest{}))
	if err != nil {
		return fmt.Errorf("list tokens (the owner token is required): %w", err)
	}
	for _, t := range list.Msg.Tokens {
		if t.Name != TokenName || t.RevokedAt != nil {
			continue
		}
		if _, err := hub.RevokeToken(ctx, connect.NewRequest(&pb.RevokeTokenRequest{Id: t.Id})); err != nil {
			return fmt.Errorf("revoke previous token %s: %w", t.Id, err)
		}
	}
	return nil
}

func readHosts(home string) []byte {
	b, err := os.ReadFile(filepath.Join(home, "hosts.md"))
	if err != nil {
		return []byte(missingHosts)
	}
	return b
}

// WriteContext writes AGENTS.md and hosts.md atomically and makes CLAUDE.md a
// relative symlink to AGENTS.md: Claude Code reads CLAUDE.md, while Codex,
// Grok and pi read AGENTS.md. A CLAUDE.md that is something else is renamed
// aside, never deleted.
func WriteContext(dir string, agents, hosts []byte) error {
	if err := writeAtomic(filepath.Join(dir, "AGENTS.md"), agents, 0o644); err != nil {
		return fmt.Errorf("write AGENTS.md: %w", err)
	}
	if err := writeAtomic(filepath.Join(dir, "hosts.md"), hosts, 0o644); err != nil {
		return fmt.Errorf("write hosts.md: %w", err)
	}
	link := filepath.Join(dir, "CLAUDE.md")
	fi, err := os.Lstat(link)
	switch {
	case os.IsNotExist(err):
	case err != nil:
		return fmt.Errorf("inspect CLAUDE.md: %w", err)
	case fi.Mode()&os.ModeSymlink != 0:
		if t, err := os.Readlink(link); err == nil && t == "AGENTS.md" {
			return nil
		}
		fallthrough
	default:
		aside := fmt.Sprintf("%s.orphaned-%d", link, time.Now().Unix())
		if err := os.Rename(link, aside); err != nil {
			return fmt.Errorf("move CLAUDE.md aside: %w", err)
		}
	}
	if err := os.Symlink("AGENTS.md", link); err != nil {
		return fmt.Errorf("link CLAUDE.md: %w", err)
	}
	return nil
}

func writeAtomic(path string, data []byte, perm os.FileMode) (err error) {
	f, err := os.CreateTemp(filepath.Dir(path), "."+filepath.Base(path)+".*")
	if err != nil {
		return err
	}
	tmp := f.Name()
	defer func() {
		if err != nil {
			f.Close()
			os.Remove(tmp)
		}
	}()
	if err = f.Chmod(perm); err != nil {
		return err
	}
	if _, err = f.Write(data); err != nil {
		return err
	}
	if err = f.Close(); err != nil {
		return err
	}
	return os.Rename(tmp, path)
}
