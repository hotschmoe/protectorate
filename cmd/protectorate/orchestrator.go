package main

import (
	"encoding/json"
	"fmt"
	"io"
	"strings"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"
	"github.com/spf13/pflag"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/structpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/mcpbridge"
	"github.com/hotschmoe/protectorate/internal/orchestrator"
)

// orchestratorHidden are commands Envoy's generated CLI reference leaves out:
// owner-only administration, node enrollment, and interactive commands an
// agent cannot drive.
var orchestratorHidden = map[string]bool{
	"hub": true, "service": true, "pair": true, "token": true, "audit": true, "orchestrator": true,
	"client": true, "mcp": true, "node invite": true, "node join": true, "node renew": true,
	"node revoke": true, "node rm": true, "node serve": true, "node metrics": true,
	"session attach": true, "sleeve attach": true,
}

// commandDocs lists every runnable command of the tree for AGENTS.md.
func commandDocs(root *cobra.Command) []orchestrator.Command {
	var out []orchestrator.Command
	var walk func(c *cobra.Command, path string)
	walk = func(c *cobra.Command, path string) {
		if path != "" && (c.Hidden || c.Name() == "help" || c.Name() == "completion" || orchestratorHidden[path]) {
			return
		}
		if path != "" && c.Runnable() {
			d := orchestrator.Command{Path: path, Use: strings.TrimPrefix(c.CommandPath(), root.Name()+" "), Short: c.Short}
			c.NonInheritedFlags().VisitAll(func(f *pflag.Flag) {
				if f.Hidden {
					return
				}
				def := f.DefValue
				if def == "[]" || def == "false" || def == "0" || def == "0s" {
					def = ""
				}
				d.Flags = append(d.Flags, orchestrator.Flag{Name: f.Name, Type: f.Value.Type(), Default: def, Usage: f.Usage})
			})
			out = append(out, d)
		}
		for _, sub := range c.Commands() {
			p := sub.Name()
			if path != "" {
				p = path + " " + p
			}
			walk(sub, p)
		}
	}
	walk(root, "")
	return out
}

func toolDocs() ([]orchestrator.Tool, error) {
	cat, err := mcpbridge.Catalog()
	if err != nil {
		return nil, err
	}
	out := make([]orchestrator.Tool, len(cat))
	for i, t := range cat {
		out[i] = orchestrator.Tool{Name: t.Name, Description: t.Description, ReadOnly: t.ReadOnly, Destructive: t.Destructive}
	}
	return out, nil
}

type orchestratorHub struct {
	protectoratev1connect.TokenServiceClient
	protectoratev1connect.SleeveServiceClient
}

func (o *opts) orchestratorHub() (orchestratorHub, error) {
	hc, err := o.httpClient()
	if err != nil {
		return orchestratorHub{}, err
	}
	return orchestratorHub{
		protectoratev1connect.NewTokenServiceClient(hc, o.hubURL),
		protectoratev1connect.NewSleeveServiceClient(hc, o.hubURL),
	}, nil
}

func orchestratorCmd(o *opts) *cobra.Command {
	c := &cobra.Command{
		Use:   "orchestrator",
		Short: "Run Envoy, the orchestrator sleeve (owner token)",
		Long: `Envoy is an ordinary sleeve named "envoy" whose workspace is
$PROTECTORATE_HOME/orchestrator on the hub machine. It holds generated AGENTS.md
(and CLAUDE.md), hosts.md and a scoped token; its only tool is this CLI. The hub
does the work, so the app's ENVOY tab starts it as well. See docs/orchestrator.md.`,
	}
	c.AddCommand(orchestratorSpawnCmd(o), orchestratorStatusCmd(o), tokenScopesCmd(o, true))
	return c
}

func orchestratorSpawnCmd(o *opts) *cobra.Command {
	sel := &pb.ModelSelection{}
	var harness string
	var scopes, addScopes []string
	var ttl time.Duration
	c := &cobra.Command{
		Use:   "spawn [--harness auto|claude|codex|grok|pi] [--scopes ...]",
		Short: "Create or respawn the envoy sleeve with a fresh scoped token",
		Long: `Ask the hub to create the sleeve "envoy", or to stop and recreate it when it
already exists. Every spawn revokes the previous envoy token, mints a new one,
regenerates AGENTS.md, CLAUDE.md and hosts.md from the live CLI, MCP catalog and
node inventory, and starts the harness. AGENTS.md is rewritten only when its
content changes, so a respawn on the same harness, scopes and CLI keeps the
harness's prompt cache. --harness auto takes the top of 'quota suggest'.

Default scopes: read, events and the non-destructive backlog_*, lease_*,
message_*, sleeve_spawn_sleeve and sleeve_resleeve_sleeve tools (never session
creation: it runs outside the sandbox), named explicitly. --scopes replaces them;
--add-scopes extends them. Envoy can kill its own workers without a kill scope.
Use orchestrator scopes --add/--remove to change its grants in place.
The hub renews the token before it expires without restarting the harness.
Needs the owner token.`,
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			var hv pb.Harness
			if harness != "" && harness != "auto" {
				var ok bool
				if hv, ok = harnessNames[harness]; !ok || hv == pb.Harness_HARNESS_SHELL {
					return fmt.Errorf("unknown harness %q, want auto, claude, codex, grok or pi", harness)
				}
			}
			sl, err := sleeveClient(o)
			if err != nil {
				return err
			}
			res, err := sl.SpawnEnvoy(cmd.Context(), connect.NewRequest(&pb.SpawnEnvoyRequest{
				Harness: hv, Scopes: scopes, AddScopes: addScopes, Ttl: durationpb.New(ttl), Selection: optionalSelection(sel),
			}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				r := res.Msg
				verb := "spawned"
				if r.Replaced {
					verb = "respawned"
				}
				fmt.Fprintf(w, "%s\t%s\nharness\t%s (%s)\nworkspace\t%s\n", verb, r.Sleeve.Name, harnessName(r.Sleeve.Harness), r.Reason, r.Workspace)
				fmt.Fprintf(w, "token\t%s\nscopes\t%s\nchat\tprotectorate chat watch %s\n", r.Token.Id, strings.Join(r.Token.Scopes, ", "), r.Sleeve.Name)
			})
		},
	}
	c.Flags().StringVar(&harness, "harness", "auto", "auto (top of quota suggest), claude, codex, grok or pi")
	selectionFlags(c, sel)
	c.Flags().StringSliceVar(&scopes, "scopes", nil, "replace the default scopes (comma separated or repeated)")
	c.Flags().StringSliceVar(&addScopes, "add-scopes", nil, "scopes to add to the defaults")
	c.Flags().DurationVar(&ttl, "ttl", 720*time.Hour, "token lifetime, renewed by the hub; 0 never expires")
	return c
}

func orchestratorStatusCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show the envoy sleeve, its harness and its token",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			h, err := o.orchestratorHub()
			if err != nil {
				return err
			}
			var sl *pb.Sleeve
			res, err := h.GetSleeve(cmd.Context(), connect.NewRequest(&pb.GetSleeveRequest{Name: orchestrator.SleeveName}))
			switch {
			case err == nil:
				sl = res.Msg.Sleeve
			case connect.CodeOf(err) != connect.CodeNotFound:
				return err
			}
			list, err := h.ListTokens(cmd.Context(), connect.NewRequest(&pb.ListTokensRequest{}))
			if err != nil {
				return err
			}
			var tok *pb.TokenInfo
			for _, t := range list.Msg.Tokens {
				if t.Name == orchestrator.TokenName && (tok == nil || t.CreatedAt.AsTime().After(tok.CreatedAt.AsTime())) {
					tok = t
				}
			}
			msg, err := statusMessage(sl, tok)
			if err != nil {
				return err
			}
			return o.out(cmd).print(msg, func(w io.Writer) { writeOrchestratorStatus(w, sl, tok, time.Now()) })
		},
	}
}

func writeOrchestratorStatus(w io.Writer, sl *pb.Sleeve, tok *pb.TokenInfo, now time.Time) {
	if sl == nil {
		fmt.Fprintln(w, "sleeve\tnot created (run: protectorate orchestrator spawn)")
	} else {
		fmt.Fprintf(w, "sleeve\t%s (%s)\nharness\t%s\nworkspace\t%s\n", sl.Name, sleeveStateLabel(sl.State), harnessName(sl.Harness), sl.Workspace)
		fmt.Fprintf(w, "started\t%s\n", sl.CreatedAt.AsTime().Local().Format(time.RFC3339))
	}
	if tok == nil {
		fmt.Fprintln(w, "token\tnone")
		return
	}
	state := "active"
	switch {
	case tok.RevokedAt != nil:
		state = "revoked"
	case tok.ExpiresAt != nil && !tok.ExpiresAt.AsTime().After(now):
		state = "expired"
	}
	fmt.Fprintf(w, "token\t%s (%s)\nscopes\t%s\n", tok.Id, state, strings.Join(tok.Scopes, ", "))
	fmt.Fprintf(w, "expires\t%s\nlast used\t%s\n", fmtStamp(tok.ExpiresAt, "never"), fmtStamp(tok.LastUsedAt, "never"))
}

// statusMessage joins the sleeve and token as one proto-JSON object for --json.
func statusMessage(sl *pb.Sleeve, tok *pb.TokenInfo) (*structpb.Struct, error) {
	m := map[string]any{}
	for key, msg := range map[string]proto.Message{"sleeve": sl, "token": tok} {
		if msg == nil || !msg.ProtoReflect().IsValid() {
			continue
		}
		raw, err := protoJSON(msg)
		if err != nil {
			return nil, err
		}
		var v any
		if err := json.Unmarshal(raw, &v); err != nil {
			return nil, err
		}
		m[key] = v
	}
	return structpb.NewStruct(m)
}
