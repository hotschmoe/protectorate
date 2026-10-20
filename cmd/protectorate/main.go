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
	"net/netip"
	"os"
	"os/signal"
	"path/filepath"
	"runtime"
	"strings"
	"sync/atomic"
	"syscall"
	"time"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
	"github.com/hotschmoe/protectorate/internal/auth"
	"github.com/hotschmoe/protectorate/internal/backup"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/quota"
	"github.com/hotschmoe/protectorate/internal/release"
	"github.com/hotschmoe/protectorate/internal/session"
)

var version = "dev"

func main() {
	if err := runMain(context.Background(), func(ctx context.Context) error { return newRoot().ExecuteContext(ctx) }); err != nil {
		fmt.Fprintln(os.Stderr, "error:", err)
		var ee *exitError
		if errors.As(err, &ee) {
			os.Exit(ee.code)
		}
		os.Exit(1)
	}
}

type opts struct {
	hubURL    string
	tokenFile string
	json      bool
}

const (
	tokenEnv     = "PROTECTORATE_TOKEN"
	tokenFileEnv = "PROTECTORATE_TOKEN_FILE"
)

// token resolves the bearer secret: --token-file, then env
// PROTECTORATE_TOKEN_FILE, then env PROTECTORATE_TOKEN, then a saved device
// token for this hub, then the owner token file of a hub on this machine.
func (o *opts) token() (string, error) {
	file := o.tokenFile
	if file == "" {
		file = os.Getenv(tokenFileEnv)
	}
	if file != "" {
		b, err := os.ReadFile(file)
		if err != nil {
			return "", fmt.Errorf("read token file: %w", err)
		}
		tok := strings.TrimSpace(string(b))
		if tok == "" {
			return "", fmt.Errorf("token file %s is empty", file)
		}
		return tok, nil
	}
	if v := strings.TrimSpace(os.Getenv(tokenEnv)); v != "" {
		return v, nil
	}
	if tok, err := o.savedDeviceToken(); err != nil || tok != "" {
		return tok, err
	}
	return hub.ReadToken()
}

func (o *opts) out(cmd *command) out { return out{w: cmd.OutOrStdout(), json: o.json} }

// httpClient sends the resolved token on every request.
func (o *opts) httpClient() (*http.Client, error) {
	tok, err := o.token()
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

func newRoot() *command {
	o := &opts{}
	root := &command{
		Use:   "protectorate",
		Short: "Orchestrate coding-agent harnesses across machines",
	}
	hubDefault := "http://127.0.0.1:7470"
	if v := os.Getenv("PROTECTORATE_HUB"); v != "" {
		hubDefault = v
	}
	root.PersistentFlags().StringVar(&o.hubURL, "hub", hubDefault, "hub URL (env PROTECTORATE_HUB)")
	root.PersistentFlags().StringVar(&o.tokenFile, "token-file", "", "file holding the bearer token (env "+tokenFileEnv+", or the token itself in env "+tokenEnv+"; default: saved device credential, then this machine's owner token)")
	root.PersistentFlags().BoolVar(&o.json, "json", false, "print proto-JSON, one object per line")

	root.AddCommand(hubCmd(o), pairCmd(o), devicesCmd(o), tokenCmd(o), grantCmd(o), auditCmd(o), statusCmd(o), quotaCmd(o), nodeCmd(o), clientCmd(), tailnetCmd(o), backlogCmd(o), orchestratorCmd(o), serviceCmd(), eventsCmd(o), sessionCmd(o), sleeveCmd(o), chatCmd(o), reportCmd(o), leaseCmd(o), deskCmd(o), whoCmd(o), msgCmd(o), loginCmd(o), jobCmd(o), apiCmd(o), mcpCmd(o), driverCmd(), versionCmd(o))
	return root
}

func envOr(key, def string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return def
}

func hubCmd(o *opts) *command {
	c := &command{Use: "hub", Short: "Run the hub"}
	var listen, tmuxSocket, webDir, binaryDir, publicURL string
	var releaseRepo, updateChannel, releaseAPI, releaseDir string
	var hubUpdateTimeout time.Duration
	var quotaInterval, deadRetention time.Duration
	var localURLs, localModelHosts []string
	var nodeInterval, ackTimeout time.Duration
	var nodeTokenFile, backupDir string
	var backupEvery time.Duration
	var watch, toolDirs, refDirs, workspaceRoots []string
	var orchestratorGrants bool
	var startEnvoy, sleeveEffort, goCacheMax string
	var trustedProxies []string
	serve := &command{
		Use:   "serve",
		Short: "Serve the hub API",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) (retErr error) {
			hubExe, err := os.Executable()
			if err != nil {
				return fmt.Errorf("resolve hub executable: %w", err)
			}
			hubExe, err = filepath.EvalSymlinks(hubExe)
			if err != nil {
				return fmt.Errorf("resolve hub executable symlink: %w", err)
			}
			source, err := release.New(releaseRepo, updateChannel, releaseAPI, releaseDir)
			if err != nil {
				return err
			}
			if hubUpdateTimeout <= 0 {
				return errors.New("--hub-update-timeout must be positive")
			}
			cacheLimit, err := parseSize(goCacheMax)
			if err != nil {
				return fmt.Errorf("parse sleeve Go cache limit: %w", err)
			}
			if backupDir != "" && backupEvery <= 0 {
				return errors.New("--backup-every must be positive")
			}
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
			ctx, cancelHub := context.WithCancel(ctx)
			defer cancelHub()
			var wantRestart atomic.Bool
			defer func() {
				if !wantRestart.Load() {
					return
				}
				if hubSystemdUnit() {
					retErr = &exitError{code: 75, msg: "restarting hub through supervisor"}
					return
				}
				retErr = reexecHub(hubExe, os.Args[1:])
			}()
			fmt.Fprintf(cmd.ErrOrStderr(), "hub %s listening on %s\n", version, l.Addr())
			readers := []quota.Reader{quota.NewClaude(), quota.NewCodex(), quota.NewGrok(), quota.NewAntigravity()}
			for _, u := range localURLs {
				readers = append(readers, quota.NewLocal(u))
			}
			home, err := hub.Home()
			if err != nil {
				return err
			}
			store, err := auth.OpenStore(filepath.Join(home, "tokens.json"))
			if err != nil {
				return err
			}
			audit := auth.NewAudit(filepath.Join(home, "audit"))
			audit.OnError = func(err error) { fmt.Fprintln(cmd.ErrOrStderr(), "error:", err) }
			defer audit.Close()
			defer store.Flush()
			h := hub.New(version, tok)
			h.SleeveEffort = sleeveEffort
			for _, p := range trustedProxies {
				ip, err := netip.ParseAddr(p)
				if err != nil {
					return fmt.Errorf("--trusted-proxy %q: %w", p, err)
				}
				h.TrustedProxies = append(h.TrustedProxies, ip.Unmap())
			}
			h.SelfURL = hub.LocalURL(l.Addr())
			h.AckTimeout = ackTimeout
			h.SetAuth(auth.New(tok, store, audit))
			h.Sessions = session.New(tmuxSocket)
			h.DeadRetention = deadRetention
			h.WebDir = webDir
			h.BinaryDir = binaryDir
			h.Releases = source
			h.SelfExe = hubExe
			trial, trialErr := loadHubTrial(filepath.Join(home, "hub-update.json"))
			if trialErr != nil && !errors.Is(trialErr, os.ErrNotExist) {
				return trialErr
			}
			if trialErr != nil || trial.Phase != "trial" {
				previous, _ := os.Readlink(filepath.Join(source.Dir, "previous"))
				if err := source.Prune(previous); err != nil {
					return fmt.Errorf("prune installed releases: %w", err)
				}
			}
			h.ReplaceHub = func(ctx context.Context, path string, r release.Info) (func(), error) {
				return prepareHubUpdate(ctx, h, filepath.Join(home, "token"), hubUpdateTimeout, cancelHub, &wantRestart, path, r)
			}
			h.PublicURL = publicURL
			h.OrchestratorGrants = orchestratorGrants
			h.LocalModelHosts = localModelHosts
			h.SetQuota(hub.NewQuotaCache(readers, hub.QuotaOptions{Interval: quotaInterval}))
			ts, err := startTailnet(ctx, cmd.ErrOrStderr())
			if err != nil {
				return err
			}
			defer ts.Close()
			h.SetTailnetStatus(ts.Status)
			if ts.Admin != nil {
				h.SetTailnetAdmin(ts.Admin)
			}
			nodeTok, err := nodeToken(nodeTokenFile)
			if err != nil {
				return err
			}
			if err := attachNodes(h, nodeInterval, nodeTok, watch, ts.Client, ts.Peers); err != nil {
				return err
			}
			if err := attachSleeves(ctx, h, toolDirs, refDirs, workspaceRoots, cacheLimit); err != nil {
				return err
			}
			tools, err := toolDocs()
			if err != nil {
				return err
			}
			h.SetEnvoy(hub.EnvoyConfig{Commands: commandDocs(cmd.Root()), Tools: tools})
			if startEnvoy != "" {
				hv, ok := harnessNames[startEnvoy]
				if startEnvoy == "auto" {
					hv, ok = pb.Harness_HARNESS_UNSPECIFIED, true
				}
				if !ok || hv == pb.Harness_HARNESS_SHELL {
					return fmt.Errorf("--start-envoy %q: want auto, claude, codex, grok or pi", startEnvoy)
				}
				if err := h.StartEnvoyIfMissing(ctx, hv); err != nil {
					fmt.Fprintln(cmd.ErrOrStderr(), "error:", err)
				}
			}
			if backupDir != "" {
				bctx, bstop := context.WithCancel(ctx)
				backups := make(chan struct{})
				go func() {
					defer close(backups)
					backup.Run(bctx, backup.Options{Home: home, Dir: backupDir, Version: version}, backupEvery,
						func(format string, args ...any) { fmt.Fprintf(cmd.ErrOrStderr(), format+"\n", args...) })
				}()
				defer func() {
					bstop()
					<-backups
				}()
			}
			return h.Serve(ctx, l, ts.Extra...)
		},
	}
	serve.Flags().StringVar(&backupDir, "backup-dir", "", "write hub backups here (see hub backup); old ones are never deleted")
	serve.Flags().DurationVar(&backupEvery, "backup-every", 6*time.Hour, "how often to back up when --backup-dir is set")
	serve.Flags().DurationVar(&nodeInterval, "node-interval", 30*time.Second, "how often to poll node metrics")
	serve.Flags().DurationVar(&ackTimeout, "msg-ack-timeout", 30*time.Minute, "how long an ack-required message may stay unacked before it is re-delivered, and again before it is escalated to the orchestrator")
	serve.Flags().StringVar(&nodeTokenFile, "node-token-file", "", "file holding the bearer token presented to node agents (or the token itself in env "+nodeTokenEnv+")")
	serve.Flags().StringSliceVar(&watch, "watch", nil, "directory on the hub whose size is tracked (repeatable)")
	serve.Flags().StringSliceVar(&refDirs, "sleeve-reference-dir", nil, "directory offered read-only to sleeves by 'sleeve workspaces' (repeatable), besides each workspace's references/")
	serve.Flags().StringSliceVar(&trustedProxies, "trusted-proxy", nil, "reverse proxy IP address whose X-Forwarded-For names the client, e.g. a Traefik host (repeatable; default none)")
	serve.Flags().StringVar(&sleeveEffort, "sleeve-effort", "high", "default requested reasoning effort for supported chat harnesses (empty uses native default; unsupported levels fall back to native default)")
	serve.Flags().StringVar(&goCacheMax, "sleeve-go-cache-max", "20G", "maximum logical bytes per shared Go cache snapshot (0 disables sharing; recent entries imported newest first in background)")
	serve.Flags().StringSliceVar(&workspaceRoots, "workspace-root", nil, "directory browser root (repeatable; defaults to ~/github when it exists)")
	serve.Flags().StringSliceVar(&toolDirs, "sleeve-tool-dir", nil, "directory bound read-only into sleeves (repeatable; replaces the default ~/.local/{bin,lib,opt,go} ~/.nvm ~/go ~/.cargo/bin ~/.rustup)")
	serve.Flags().StringVar(&listen, "listen", "127.0.0.1:7470", "address to listen on")
	serve.Flags().DurationVar(&quotaInterval, "quota-interval", 5*time.Minute, "how often to poll provider quotas")
	serve.Flags().StringSliceVar(&localURLs, "local-llm-url", nil, "OpenAI-compatible local LLM base URL to monitor (repeatable)")
	serve.Flags().StringSliceVar(&localModelHosts, "local-model-host", nil, "hostname of a model server of your own that the app flags as a local model, beyond private and tailnet addresses (repeatable)")
	serve.Flags().StringVar(&tmuxSocket, "tmux-socket", envOr("PROTECTORATE_TMUX_SOCKET", session.DefaultSocket),
		"tmux socket name (tmux -L) for harness sessions, kept apart from your default tmux server")
	serve.Flags().StringVar(&webDir, "web", "", "web client directory (default: current verified release web directory)")
	serve.Flags().StringVar(&binaryDir, "binary-dir", "", "explicit platform binary directory (default: current verified release)")
	serve.Flags().StringVar(&releaseRepo, "release-repo", "hotschmoe/protectorate", "GitHub releases repository (OWNER/REPO)")
	serve.Flags().StringVar(&updateChannel, "update-channel", "stable", "release channel (stable or dev)")
	serve.Flags().StringVar(&releaseAPI, "release-api-url", "", "GitHub API base URL (override for local release-server testing)")
	serve.Flags().StringVar(&releaseDir, "release-dir", "", "verified release cache (default: $XDG_DATA_HOME/protectorate/releases or ~/.local/share/protectorate/releases)")
	serve.Flags().DurationVar(&hubUpdateTimeout, "hub-update-timeout", 45*time.Second, "deadline for an updated hub to report healthy before rollback")
	serve.Flags().StringVar(&publicURL, "public-url", "", "URL new machines reach this hub at, used in the install scripts and the Add node commands (default: the URL each request came in on)")
	serve.Flags().BoolVar(&orchestratorGrants, "orchestrator-admin-grants", false, "let the orchestrator mint admin grants (job_* and node_admin) for shared nodes; care nodes are granted by the owner only")
	serve.Flags().StringVar(&startEnvoy, "start-envoy", "", "at boot, start Envoy on this harness (auto, claude, codex, grok or pi) only if the hub has no Envoy yet; off by default, since the app starts it")
	serve.Flags().DurationVar(&deadRetention, "dead-retention", 24*time.Hour, "keep exited sessions this long before the hub kills them (0 keeps them until killed or respawned)")
	c.AddCommand(serve, hubTailnetCmd(), hubBackupCmd(), hubRestoreCmd(), hubUpdateCmd(o), hubUpdateGuardCmd())
	return c
}

func quotaCmd(o *opts) *command {
	var refresh bool
	c := &command{
		Use:   "quota",
		Short: "Show subscription usage, resets and projected unused capacity",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
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
	c.AddCommand(quotaSuggestCmd(o))
	return c
}

func statusCmd(o *opts) *command {
	return &command{
		Use:   "status",
		Short: "Show hub status",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
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
			runtime, err := protectoratev1connect.NewHubServiceClient(hc, o.hubURL).
				GetSleeveRuntime(cmd.Context(), connect.NewRequest(&pb.GetSleeveRuntimeRequest{}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(s, func(w io.Writer) {
				fmt.Fprintf(w, "version\t%s\n", s.Version)
				fmt.Fprintf(w, "available\t%s\nchannel\t%s\n", s.AvailableVersion, s.UpdateChannel)
				if s.ReleaseNotesUrl != "" {
					fmt.Fprintf(w, "release notes\t%s\n", s.ReleaseNotesUrl)
				}
				fmt.Fprintf(w, "hostname\t%s\n", s.Hostname)
				fmt.Fprintf(w, "platform\t%s/%s\n", s.Os, s.Arch)
				fmt.Fprintf(w, "uptime\t%s\n", (time.Duration(s.UptimeSeconds) * time.Second))
				fmt.Fprintf(w, "started\t%s\n", s.StartedAt.AsTime().Local().Format(time.RFC3339))
				fmt.Fprintf(w, "bwrap\t%s (%s)\noverlays\t%t\n", runtime.Msg.BwrapPath, runtime.Msg.BwrapVersion, runtime.Msg.Overlays)
			})
		},
	}
}

func eventsCmd(o *opts) *command {
	return &command{
		Use:   "events",
		Short: "Stream hub events, one per line",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
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

func apiCmd(o *opts) *command {
	c := &command{Use: "api", Short: "Raw API access"}
	c.AddCommand(&command{
		Use:   "call <Service/Method> [json]",
		Short: "Unary call, e.g. api call HubService/GetStatus '{}'",
		Args:  rangeArgs(1, 2),
		RunE: func(cmd *command, args []string) error {
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

func versionCmd(o *opts) *command {
	return &command{
		Use:   "version",
		Short: "Print the client version",
		Args:  noArgs,
		Run: func(cmd *command, _ []string) {
			if o.json {
				fmt.Fprintf(cmd.OutOrStdout(), `{"version":%q,"os":%q,"arch":%q}`+"\n", version, runtime.GOOS, runtime.GOARCH)
				return
			}
			fmt.Fprintf(cmd.OutOrStdout(), "protectorate %s %s/%s\n", version, runtime.GOOS, runtime.GOARCH)
		},
	}
}

func pairCmd(o *opts) *command {
	var pubURL string
	var appQR bool
	c := &command{
		Use:   "pair",
		Short: "Print a pairing link and QR code for adding this hub on another device",
		Args:  noArgs,
		RunE: func(cmd *command, _ []string) error {
			if pubURL == "" {
				pubURL = o.hubURL
			}
			links, err := hub.NewPairLinks(pubURL)
			if err != nil {
				return err
			}
			w := cmd.OutOrStdout()
			if o.json {
				return json.NewEncoder(w).Encode(map[string]string{"web": links.Web, "app": links.Scheme})
			}
			qrText := links.Web
			if appQR {
				qrText = links.Scheme
			}
			if err := hub.WriteQR(w, qrText); err != nil {
				return err
			}
			fmt.Fprintf(w, "\nweb:  %s\napp:  %s\n", links.Web, links.Scheme)
			fmt.Fprintln(w, "\nOpen the link, connect, then approve the matching code on a trusted device.")
			return nil
		},
	}
	c.Flags().StringVar(&pubURL, "url", "", "public base URL the other device can reach (default --hub)")
	c.Flags().BoolVar(&appQR, "app", false, "encode the protectorate:// link for the native app instead of the web link")
	return c
}

func quotaSuggestCmd(o *opts) *command {
	var forSize string
	var refresh bool
	c := &command{
		Use:   "suggest [--for small|medium|large]",
		Short: "Rank harnesses by usable spare capacity before reset",
		Long: `Rank claude, codex, grok and pi (local LLM) by capacity that is usable now
and would otherwise expire unused at a reset. Only fresh quota data counts:
stale or unknown providers are listed as not picked. The local LLM is always
available as the fallback. --for says how much headroom the task needs
(small 5%, medium 15%, large 30% usable).`,
		Args: noArgs,
		RunE: func(cmd *command, _ []string) error {
			size := pb.TaskSize_TASK_SIZE_UNSPECIFIED
			if forSize != "" {
				var ok bool
				if size, ok = taskSizes[forSize]; !ok {
					return fmt.Errorf("unknown task size %q, want small, medium or large", forSize)
				}
			}
			hc, err := o.httpClient()
			if err != nil {
				return err
			}
			res, err := protectoratev1connect.NewQuotaServiceClient(hc, o.hubURL).
				Suggest(cmd.Context(), connect.NewRequest(&pb.SuggestRequest{TaskSize: size, Refresh: refresh}))
			if err != nil {
				return err
			}
			return o.out(cmd).print(res.Msg, func(w io.Writer) { writeSuggest(w, res.Msg) })
		},
	}
	c.Flags().StringVar(&forSize, "for", "", "task size hint: small, medium (default) or large")
	c.Flags().BoolVar(&refresh, "refresh", false, "force an immediate quota read first (rate-limited per provider)")
	return c
}
