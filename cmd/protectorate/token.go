package main

import (
	"fmt"
	"io"
	"path/filepath"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"
	"google.golang.org/protobuf/types/known/durationpb"
	"google.golang.org/protobuf/types/known/timestamppb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/hub"
)

// deviceScopes and deviceTTL are what `pair` grants a paired phone or browser.
var deviceScopes = []string{"read", "terminal", "events", "session_*", "backlog_*"}

const deviceTTL = 90 * 24 * time.Hour

func (o *opts) tokenClient() (protectoratev1connect.TokenServiceClient, error) {
	hc, err := o.httpClient()
	if err != nil {
		return nil, err
	}
	return protectoratev1connect.NewTokenServiceClient(hc, o.hubURL), nil
}

func tokenCmd(o *opts) *cobra.Command {
	c := &cobra.Command{
		Use:   "token",
		Short: "Issue, list and revoke scoped API tokens (owner token only)",
		Long: `Scoped tokens limit what a phone, bridge or agent can do. The owner token
in $PROTECTORATE_HOME/token has full access; every issued token starts with
nothing and gains only the scopes it is created with. See docs/auth.md.`,
	}
	c.AddCommand(tokenCreateCmd(o), tokenLsCmd(o), tokenRevokeCmd(o))
	return c
}

func tokenCreateCmd(o *opts) *cobra.Command {
	var scopes []string
	var ttl time.Duration
	c := &cobra.Command{
		Use:   "create <name>",
		Short: "Issue a token; the secret is printed once",
		Example: `  protectorate token create phone --scope read --scope terminal --ttl 720h
  protectorate token create bridge --scope read --scope 'backlog_*'`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
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
	c.Flags().StringArrayVar(&scopes, "scope", nil, "scope to grant: read, events, terminal, terminal:readonly or a tool-name glob like 'session_*' (repeatable, required)")
	c.Flags().DurationVar(&ttl, "ttl", 720*time.Hour, "lifetime; 0 never expires")
	return c
}

func fmtStamp(ts *timestamppb.Timestamp, none string) string {
	if ts == nil {
		return none
	}
	return ts.AsTime().Local().Format(time.RFC3339)
}

func tokenLsCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "ls",
		Short: "List issued tokens",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			cl, err := o.tokenClient()
			if err != nil {
				return err
			}
			res, err := cl.ListTokens(cmd.Context(), connect.NewRequest(&pb.ListTokensRequest{}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) {
				fmt.Fprintln(w, "ID\tNAME\tSTATE\tEXPIRES\tLAST USED\tSCOPES")
				now := time.Now()
				for _, t := range res.Msg.Tokens {
					state := "active"
					switch {
					case t.RevokedAt != nil:
						state = "revoked"
					case t.ExpiresAt != nil && !now.Before(t.ExpiresAt.AsTime()):
						state = "expired"
					}
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%v\n", t.Id, t.Name, state,
						fmtStamp(t.ExpiresAt, "never"), fmtStamp(t.LastUsedAt, "-"), t.Scopes)
				}
			})
		},
	}
}

func tokenRevokeCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "revoke <id>",
		Short: "Revoke a token immediately",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
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

func auditCmd(o *opts) *cobra.Command {
	c := &cobra.Command{Use: "audit", Short: "Inspect the audit log"}
	var n int
	tail := &cobra.Command{
		Use:   "tail",
		Short: "Show the most recent audit entries (reads the local log of a hub on this machine)",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			home, err := hub.Home()
			if err != nil {
				return err
			}
			entries, err := auth.Tail(filepath.Join(home, "audit"), n)
			if err != nil {
				return err
			}
			w := cmd.OutOrStdout()
			for _, e := range entries {
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
				fmt.Fprintln(w)
			}
			return nil
		},
	}
	tail.Flags().IntVarP(&n, "lines", "n", 50, "number of entries")
	c.AddCommand(tail)
	return c
}
