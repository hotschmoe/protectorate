package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/signal"
	"runtime"
	"strings"
	"syscall"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/quota"
	"github.com/hotschmoe/protectorate/internal/session"
)

var version = "dev"

func main() {
	if err := newRoot().ExecuteContext(context.Background()); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		os.Exit(1)
	}
}

type opts struct {
	hubURL string
	json   bool
}

func (o *opts) out(cmd *cobra.Command) out { return out{w: cmd.OutOrStdout(), json: o.json} }

// httpClient sends the hub token on every request.
func (o *opts) httpClient() (*http.Client, error) {
	tok, err := hub.ReadToken()
	if err != nil {
		return nil, err
	}
	return &http.Client{Transport: bearer{tok: tok, next: http.DefaultTransport}}, nil
}

type bearer struct {
	tok  string
	next http.RoundTripper
}

func (b bearer) RoundTrip(r *http.Request) (*http.Response, error) {
	r = r.Clone(r.Context())
	r.Header.Set("Authorization", "Bearer "+b.tok)
	return b.next.RoundTrip(r)
}

func newRoot() *cobra.Command {
	o := &opts{}
	root := &cobra.Command{
		Use:           "protectorate",
		Short:         "Orchestrate coding-agent harnesses across machines",
		SilenceUsage:  true,
		SilenceErrors: true,
	}
	hubDefault := "http://127.0.0.1:7470"
	if v := os.Getenv("PROTECTORATE_HUB"); v != "" {
		hubDefault = v
	}
	root.PersistentFlags().StringVar(&o.hubURL, "hub", hubDefault, "hub URL (env PROTECTORATE_HUB)")
	root.PersistentFlags().BoolVar(&o.json, "json", false, "print proto-JSON, one object per line")

	root.AddCommand(hubCmd(), statusCmd(o), quotaCmd(o), nodeCmd(o), clientCmd(), backlogCmd(o), eventsCmd(o), sessionCmd(o), apiCmd(o), mcpCmd(o), driverCmd(), versionCmd(o))
	return root
}

func envOr(key, def string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return def
}

func hubCmd() *cobra.Command {
	c := &cobra.Command{Use: "hub", Short: "Run the hub"}
	var listen, tmuxSocket, webDir string
	var quotaInterval time.Duration
	var localURLs []string
	var nodeInterval time.Duration
	var nodeToken string
	var watch []string
	serve := &cobra.Command{
		Use:   "serve",
		Short: "Serve the hub API",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			tok, err := hub.EnsureToken()
			if err != nil {
				return err
			}
			l, err := net.Listen("tcp", listen)
			if err != nil {
				return err
			}
			ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt, syscall.SIGTERM)
			defer stop()
			fmt.Fprintf(cmd.ErrOrStderr(), "hub %s listening on %s\n", version, l.Addr())
			readers := []quota.Reader{quota.NewClaude(), quota.NewCodex(), quota.NewGrok(), quota.NewAntigravity()}
			for _, u := range localURLs {
				readers = append(readers, quota.NewLocal(u))
			}
			h := hub.New(version, tok)
			h.Sessions = session.New(tmuxSocket)
			h.WebDir = webDir
			h.SetQuota(hub.NewQuotaCache(readers, hub.QuotaOptions{Interval: quotaInterval}))
			extra, nodeClient, closeTailnet, err := startTailnet(ctx, cmd.ErrOrStderr())
			if err != nil {
				return err
			}
			defer closeTailnet()
			if nodeToken == "" {
				nodeToken = os.Getenv(nodeTokenEnv)
			}
			if err := attachNodes(h, nodeInterval, nodeToken, watch, nodeClient); err != nil {
				return err
			}
			return h.Serve(ctx, l, extra...)
		},
	}
	serve.Flags().DurationVar(&nodeInterval, "node-interval", 30*time.Second, "how often to poll node metrics")
	serve.Flags().StringVar(&nodeToken, "node-token", "", "bearer token presented to node agents (env "+nodeTokenEnv+")")
	serve.Flags().StringSliceVar(&watch, "watch", nil, "directory on the hub whose size is tracked (repeatable)")
	serve.Flags().StringVar(&listen, "listen", "127.0.0.1:7470", "address to listen on")
	serve.Flags().DurationVar(&quotaInterval, "quota-interval", 5*time.Minute, "how often to poll provider quotas")
	serve.Flags().StringSliceVar(&localURLs, "local-llm-url", nil, "OpenAI-compatible local LLM base URL to monitor (repeatable)")
	serve.Flags().StringVar(&tmuxSocket, "tmux-socket", envOr("PROTECTORATE_TMUX_SOCKET", session.DefaultSocket),
		"tmux socket name (tmux -L) for harness sessions, kept apart from your default tmux server")
	serve.Flags().StringVar(&webDir, "web", "", "serve a built web client directory (e.g. app/build/web) at /")
	c.AddCommand(serve, hubTailnetCmd())
	return c
}

func quotaCmd(o *opts) *cobra.Command {
	var refresh bool
	c := &cobra.Command{
		Use:   "quota",
		Short: "Show subscription usage, resets and projected unused capacity",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			hc, err := o.httpClient()
			if err != nil {
				return err
			}
			res, err := protectoratev1connect.NewQuotaServiceClient(hc, o.hubURL).
				GetQuota(cmd.Context(), connect.NewRequest(&pb.GetQuotaRequest{Refresh: refresh}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { writeQuota(w, res.Msg, time.Now()) })
		},
	}
	c.Flags().BoolVar(&refresh, "refresh", false, "force an immediate read (rate-limited per provider)")
	return c
}

func statusCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "status",
		Short: "Show hub status",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			hc, err := o.httpClient()
			if err != nil {
				return err
			}
			res, err := protectoratev1connect.NewHubServiceClient(hc, o.hubURL).
				GetStatus(cmd.Context(), connect.NewRequest(&pb.GetStatusRequest{}))
			if err != nil {
				return err
			}
			s := res.Msg
			return o.out(cmd).print(s, func(w io.Writer) {
				fmt.Fprintf(w, "version\t%s\n", s.Version)
				fmt.Fprintf(w, "hostname\t%s\n", s.Hostname)
				fmt.Fprintf(w, "platform\t%s/%s\n", s.Os, s.Arch)
				fmt.Fprintf(w, "uptime\t%s\n", (time.Duration(s.UptimeSeconds) * time.Second))
				fmt.Fprintf(w, "started\t%s\n", s.StartedAt.AsTime().Local().Format(time.RFC3339))
			})
		},
	}
}

func eventsCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "events",
		Short: "Stream hub events, one per line",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			hc, err := o.httpClient()
			if err != nil {
				return err
			}
			ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt)
			defer stop()
			stream, err := protectoratev1connect.NewEventServiceClient(hc, o.hubURL).
				Subscribe(ctx, connect.NewRequest(&pb.SubscribeRequest{}))
			if err != nil {
				return err
			}
			defer stream.Close()
			p := o.out(cmd)
			for stream.Receive() {
				e := stream.Msg()
				err := p.print(e, func(w io.Writer) {
					payload, _ := protoJSON(e.Payload)
					fmt.Fprintf(w, "%s\t%s\t%s\t%s\n",
						e.Time.AsTime().Local().Format(time.RFC3339), e.Kind, e.Subject, payload)
				})
				if err != nil {
					return err
				}
			}
			if err := stream.Err(); err != nil && ctx.Err() == nil {
				return err
			}
			return nil
		},
	}
}

func apiCmd(o *opts) *cobra.Command {
	c := &cobra.Command{Use: "api", Short: "Raw API access"}
	c.AddCommand(&cobra.Command{
		Use:   "call <Service/Method> [json]",
		Short: "Unary call, e.g. api call HubService/GetStatus '{}'",
		Args:  cobra.RangeArgs(1, 2),
		RunE: func(cmd *cobra.Command, args []string) error {
			svc, method, ok := strings.Cut(args[0], "/")
			if !ok || svc == "" || method == "" {
				return errors.New("method must look like Service/Method")
			}
			if !strings.Contains(svc, ".") {
				svc = "protectorate.v1." + svc
			}
			body := "{}"
			if len(args) == 2 {
				body = args[1]
			}
			hc, err := o.httpClient()
			if err != nil {
				return err
			}
			req, err := http.NewRequestWithContext(cmd.Context(), http.MethodPost,
				strings.TrimRight(o.hubURL, "/")+"/"+svc+"/"+method, strings.NewReader(body))
			if err != nil {
				return err
			}
			req.Header.Set("Content-Type", "application/json")
			res, err := hc.Do(req)
			if err != nil {
				return err
			}
			defer res.Body.Close()
			raw, err := io.ReadAll(res.Body)
			if err != nil {
				return err
			}
			var buf bytes.Buffer
			if o.json {
				err = json.Compact(&buf, raw)
			} else {
				err = json.Indent(&buf, raw, "", "  ")
			}
			if err != nil {
				buf.Reset()
				buf.Write(raw)
			}
			if res.StatusCode != http.StatusOK {
				return fmt.Errorf("%s: %s", res.Status, bytes.TrimSpace(buf.Bytes()))
			}
			fmt.Fprintf(cmd.OutOrStdout(), "%s\n", bytes.TrimSpace(buf.Bytes()))
			return nil
		},
	})
	return c
}

func versionCmd(o *opts) *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Print the client version",
		Args:  cobra.NoArgs,
		Run: func(cmd *cobra.Command, _ []string) {
			if o.json {
				fmt.Fprintf(cmd.OutOrStdout(), `{"version":%q,"os":%q,"arch":%q}`+"\n", version, runtime.GOOS, runtime.GOARCH)
				return
			}
			fmt.Fprintf(cmd.OutOrStdout(), "protectorate %s %s/%s\n", version, runtime.GOOS, runtime.GOARCH)
		},
	}
}
