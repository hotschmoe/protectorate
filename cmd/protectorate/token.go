package main

import (
	"fmt"
	"io"
	"path/filepath"
	"slices"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/fsutil"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/orchestrator"
)

// deviceScopes and deviceTTL are what `pair` grants a paired phone or browser;
// --host-shell adds terminal:host.
var deviceScopes = []string{"read", "terminal", "events", "session_*", "sleeve_*", "chat_*", "backlog_*"}

const deviceTTL = 90 * 24 * time.Hour

func (o *opts) tokenClient() (protectoratev1connect.TokenServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewTokenServiceClient(hc, o.hubURL), nil
}

func tokenCmd(o *opts) *command {
	c := &command{
		Use:   "token",
		Short: "Issue, list, update and revoke scoped API tokens (owner token only)",
		Long: `Scoped tokens limit what a phone, bridge or agent can do. The owner token
in $PROTECTORATE_HOME/token has full access; every issued token starts with
nothing and gains only the scopes it is created with. See docs/auth.md.`,
	}
	c.AddCommand(tokenCreateCmd(o), tokenLsCmd(o), tokenRevokeCmd(o), tokenScopesCmd(o, false))
	return c
}

func tokenScopesCmd(o *opts, envoy bool) *command {
	var add, remove []string
	c := &command{
		Use: "scopes <id|name>", Short: "Change token scopes in place (owner only)", Args: exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			if len(add) == 0 && len(remove) == 0 {
				return fmt.Errorf("provide --add or --remove")
			}
			cl, err := o.tokenClient()
			if err != nil {
				return err
			}
			id := ""
			if envoy {
				list, err := cl.ListTokens(cmd.Context(), connect.NewRequest(&pb.ListTokensRequest{}))
				if err != nil {
					return err
				}
				var latest *pb.TokenInfo
				for _, t := range list.Msg.Tokens {
					if t.Name == orchestrator.TokenName && t.RevokedAt == nil && (t.ExpiresAt == nil || time.Now().Before(t.ExpiresAt.AsTime())) && (latest == nil || t.CreatedAt.AsTime().After(latest.CreatedAt.AsTime())) {
						latest = t
					}
				}
				if latest == nil {
					return fmt.Errorf("no active Envoy token")
				}
				id = latest.Id
			} else {
				id = args[0]
			}
			res, err := cl.UpdateTokenScopes(cmd.Context(), connect.NewRequest(&pb.UpdateTokenScopesRequest{Id: id, Add: add, Remove: remove}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "token\t%s (%s)\nscopes\t%v\n", res.Msg.Token.Id, res.Msg.Token.Name, res.Msg.Token.Scopes)
			})
		},
	}
	if envoy {
		c.Use = "scopes"
		c.Args = noArgs
	}
	c.Flags().StringSliceVar(&add, "add", nil, "scopes to add (comma separated or repeated)")
	c.Flags().StringSliceVar(&remove, "remove", nil, "exact scopes to remove (comma separated or repeated)")
	return c
}

func tokenCreateCmd(o *opts) *command {
	var scopes []string
	var ttl time.Duration
	c := &command{
		Use:   "create <name>",
		Short: "Issue a token; the secret is printed once",
		Example: `  protectorate token create phone --scope read --scope terminal --ttl 720h
  protectorate token create bridge --scope read --scope 'backlog_*'`,
		Args: exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := o.tokenClient()
			if err != nil {
				return err
			}
			res, err := cl.CreateToken(cmd.Context(), connect.NewRequest(&pb.CreateTokenRequest{
				Name: args[0], Scopes: scopes, Ttl: durationpb.New(ttl),
			}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "id\t%s\n", res.Msg.Token.Id)
				fmt.Fprintf(w, "name\t%s\n", res.Msg.Token.Name)
				fmt.Fprintf(w, "scopes\t%v\n", res.Msg.Token.Scopes)
				fmt.Fprintf(w, "expires\t%s\n", fmtStamp(res.Msg.Token.ExpiresAt, "never"))
				fmt.Fprintf(w, "secret\t%s\n", res.Msg.Secret)
				fmt.Fprintln(w, "\nThis is the only time the secret is shown.")
			})
		},
	}
	c.Flags().StringArrayVar(&scopes, "scope", nil, "scope to grant: read, events, terminal, terminal:readonly, terminal:host or a tool-name glob like 'session_*' (repeatable, required)")
	c.Flags().DurationVar(&ttl, "ttl", 720*time.Hour, "lifetime; 0 never expires")
	return c
}

func fmtStamp(ts *timestamppb.Timestamp, none string) string {
	if ts == nil {
		return none
	}
	return ts.AsTime().Local().Format(time.RFC3339)
}

func tokenLsCmd(o *opts) *command {
	return &command{
		Use:   "ls",
		Short: "List issued tokens",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
			cl, err := o.tokenClient()
			if err != nil {
				return err
			}
			res, err := cl.ListTokens(cmd.Context(), connect.NewRequest(&pb.ListTokensRequest{}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "ID\tNAME\tSTATE\tEXPIRES\tLAST USED\tSCOPES\tTASK")
				now := time.Now()
				for _, t := range res.Msg.Tokens {
					state := "active"
					switch {
					case t.RevokedAt != nil:
						state = "revoked"
					case t.ExpiresAt != nil && !now.Before(t.ExpiresAt.AsTime()):
						state = "expired"
					}
					task := "-"
					if t.Task != "" {
						task = t.Task + " (by " + t.MintedBy + ")"
					}
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%v\t%s\n", t.Id, t.Name, state,
						fmtStamp(t.ExpiresAt, "never"), fmtStamp(t.LastUsedAt, "-"), t.Scopes, task)
				}
			})
		},
	}
}

func tokenRevokeCmd(o *opts) *command {
	return &command{
		Use:   "revoke <id>",
		Short: "Revoke a token immediately",
		Args:  exactArgs(1),
		RunE: func(cmd *command, args []string) error {
			cl, err := o.tokenClient()
			if err != nil {
				return err
			}
			res, err := cl.RevokeToken(cmd.Context(), connect.NewRequest(&pb.RevokeTokenRequest{Id: args[0]}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintf(w, "revoked %s (%s)\n", res.Msg.Token.Id, res.Msg.Token.Name)
			})
		},
	}
}

func (o *opts) grantClient() (protectoratev1connect.GrantServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewGrantServiceClient(hc, o.hubURL), nil
}

func grantCmd(o *opts) *command {
	c := &command{Use: "grant", Short: "Mint task-scoped grants"}
	var node, task, out string
	var ttl time.Duration
	admin := &command{
		Use:   "admin --node <node|glob> --task <text> [--ttl 4h]",
		Short: "Mint a short-lived token for one task: job_* and node_admin:<node>",
		Long: `Mints a token whose scopes are job_* and node_admin:<node>: jobs on that node,
including a care node and admin jobs (job run --admin). It expires after --ttl
(default 4h, at most 24h), records the task, and every call made with it is
audited with the task. The owner may name a glob over node names; the
orchestrator may grant one shared node by name when the hub runs with
--orchestrator-admin-grants. Revoke early with protectorate token revoke <id>.`,
		Example: `  protectorate grant admin --node kidsbox --ttl 4h --task "update kids machine"`,
		Args:    noArgs,
		RunE: func(cmd *command, _ []string) error {
			cl, err := o.grantClient()
			if err != nil {
				return err
			}
			res, err := cl.Admin(cmd.Context(), connect.NewRequest(&pb.GrantServiceAdminRequest{Node: node, Task: task, Ttl: durationpb.New(ttl)}))
			if err != nil {
				return err
			}
			use := "PROTECTORATE_TOKEN=" + res.Msg.Secret + " protectorate"
			if out != "" {
				if err := fsutil.WriteFile(out, []byte(res.Msg.Secret+"\n"), 0o600); err != nil {
					return fmt.Errorf("write %s: %w", out, err)
				}
				use = "protectorate --token-file " + out
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				t := res.Msg.Token
				fmt.Fprintf(w, "grant\t%s (%s)\n", t.Id, t.Name)
				fmt.Fprintf(w, "task\t%s\n", t.Task)
				fmt.Fprintf(w, "scopes\t%v\n", t.Scopes)
				fmt.Fprintf(w, "expires\t%s\n", fmtStamp(t.ExpiresAt, "never"))
				fmt.Fprintf(w, "\nUse: %s job run --on %s [--admin] -- <cmd>\n", use, node)
				if out == "" {
					fmt.Fprintln(w, "This is the only time the secret is shown.")
				}
			})
		},
	}
	admin.Flags().StringVar(&node, "node", "", "node name, or (owner) a glob over node names (required)")
	admin.Flags().StringVar(&task, "task", "", "what the grant is for; recorded with the token and in the audit log (required)")
	admin.Flags().DurationVar(&ttl, "ttl", 4*time.Hour, "lifetime, at most 24h")
	admin.Flags().StringVar(&out, "out", "", "write the secret to this file (mode 0600) instead of printing it")
	_ = admin.MarkFlagRequired("node")
	_ = admin.MarkFlagRequired("task")
	c.AddCommand(admin)
	return c
}

func auditCmd(o *opts) *command {
	c := &command{Use: "audit", Short: "Inspect the audit log"}
	var n int
	tail := &command{
		Use:   "tail",
		Short: "Show the most recent audit entries (reads the local log of a hub on this machine)",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
			home, err := hub.Home()
			if err != nil {
				return err
			}
			page, err := auth.ReadPage(filepath.Join(home, "audit"), auth.Query{Limit: n})
			if err != nil {
				return err
			}
			w := cmd.OutOrStdout()
			for _, e := range slices.Backward(page.Entries) {
				if o.json {
					b, _ := auth.MarshalEntry(e)
					fmt.Fprintf(w, "%s\n", b)
					continue
				}
				who := e.Actor
				if e.Tailnet != "" {
					who += "@" + e.Tailnet
				}
				fmt.Fprintf(w, "%s  %s  %s  %s  %s", e.Time.Local().Format(time.RFC3339), who, e.Procedure, e.Subject, e.Outcome)
				if e.Code != "" {
					fmt.Fprintf(w, " (%s)", e.Code)
				}
				if e.Detail != "" {
					fmt.Fprintf(w, "  %s", e.Detail)
				}
				if e.Task != "" {
					fmt.Fprintf(w, "  [task: %s]", e.Task)
				}
				fmt.Fprintln(w)
			}
			return nil
		},
	}
	tail.Flags().IntVarP(&n, "lines", "n", 50, "number of entries")
	c.AddCommand(tail)
	return c
}
