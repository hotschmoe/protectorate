package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"connectrpc.com/connect"
	"github.com/spf13/cobra"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/node"
	"github.com/hotschmoe/protectorate/internal/tailnet"
)

func hubTailnetCmd() *cobra.Command {
	c := &cobra.Command{Use: "tailnet", Short: "Manage the hub's tailnet (headscale)"}

	var version string
	install := &cobra.Command{
		Use:   "install",
		Short: "Download headscale into $PROTECTORATE_HOME/bin and verify its sha256",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			home, err := hub.Home()
			if err != nil {
				return err
			}
			path, err := tailnet.Install(cmd.Context(), home, version, nil)
			if err != nil {
				return err
			}
			fmt.Fprintf(cmd.OutOrStdout(), "installed %s (sha256 verified)\n", path)
			return nil
		},
	}
	install.Flags().StringVar(&version, "version", tailnet.HeadscaleVersion, "headscale version (only the pinned one is supported at run time)")

	var cfg tailnet.Config
	var noEmbeddedDERP bool
	init := &cobra.Command{
		Use:   "init",
		Short: "Write the tailnet config (server URL, DERP, key expiry, pools)",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			home, err := hub.Home()
			if err != nil {
				return err
			}
			cfg.EmbeddedDERP = !noEmbeddedDERP
			if err := cfg.Save(home); err != nil {
				return err
			}
			if err := cfg.WriteFiles(home); err != nil {
				return err
			}
			fmt.Fprintf(cmd.OutOrStdout(), "wrote %s; `hub serve` now runs headscale and joins the tailnet\n", tailnet.Dir(home))
			return nil
		},
	}
	d := tailnet.DefaultConfig("")
	f := init.Flags()
	f.StringVar(&cfg.ServerURL, "server-url", "", "URL nodes use to reach headscale, e.g. https://hs.example.com or http://LAN-IP:7480")
	f.StringVar(&cfg.Listen, "listen", d.Listen, "headscale listen address")
	f.StringVar(&cfg.GRPCListen, "grpc-listen", d.GRPCListen, "headscale gRPC listen address (keep on loopback)")
	f.StringVar(&cfg.MetricsListen, "metrics-listen", d.MetricsListen, "headscale metrics listen address (keep on loopback)")
	f.StringVar(&cfg.BaseDomain, "base-domain", d.BaseDomain, "MagicDNS base domain (must differ from the server URL's domain)")
	f.StringVar(&cfg.HubName, "hub-name", d.HubName, "tailnet hostname of the hub")
	f.BoolVar(&noEmbeddedDERP, "no-embedded-derp", false, "disable headscale's embedded DERP relay and STUN")
	f.StringVar(&cfg.STUNListen, "stun-listen", d.STUNListen, "embedded STUN UDP listen address")
	f.StringVar(&cfg.DERPIPv4, "derp-ipv4", "", "public IPv4 advertised for the embedded DERP region")
	f.BoolVar(&cfg.PublicDERP, "public-derp", false, "also use Tailscale's public DERP map")
	f.StringVar(&cfg.TLSCert, "tls-cert", "", "TLS certificate for the control URL (omit when a reverse proxy terminates TLS)")
	f.StringVar(&cfg.TLSKey, "tls-key", "", "TLS key for the control URL")
	f.IntVar(&cfg.KeyExpiryDays, "key-expiry-days", d.KeyExpiryDays, "node key lifetime; expired nodes must be re-invited")
	f.StringSliceVar(&cfg.Pools, "pool", nil, "build pool whose members may reach each other (repeatable)")
	_ = init.MarkFlagRequired("server-url")

	c.AddCommand(install, init)
	return c
}

// admin connects to the running hub's headscale.
func admin(ctx context.Context) (*tailnet.Admin, tailnet.Config, error) {
	home, err := hub.Home()
	if err != nil {
		return nil, tailnet.Config{}, err
	}
	cfg, err := tailnet.LoadConfig(home)
	if errors.Is(err, os.ErrNotExist) {
		return nil, cfg, errors.New("tailnet not initialised: run `protectorate hub tailnet init` first")
	}
	if err != nil {
		return nil, cfg, err
	}
	a := tailnet.NewAdmin(home, cfg)
	if err := a.Ensure(ctx); err != nil {
		return nil, cfg, err
	}
	return a, cfg, nil
}

// nodeEnrollCmds are the tailnet enrollment commands under `node`.
func nodeEnrollCmds(o *opts) []*cobra.Command {
	var tier string
	var expires time.Duration
	var pools []string
	invite := &cobra.Command{
		Use:   "invite <name>",
		Short: "Create a single-use enrollment key and print the join command",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			if !slices.Contains(tailnet.Tiers, tier) {
				return fmt.Errorf("--tier must be one of %s", strings.Join(tailnet.Tiers, ", "))
			}
			a, cfg, err := admin(cmd.Context())
			if err != nil {
				return err
			}
			for _, p := range pools {
				if !slices.Contains(cfg.Pools, p) {
					return fmt.Errorf("pool %q is not configured (`hub tailnet init --pool`)", p)
				}
			}
			tags := []string{tailnet.TagNode, tailnet.TierTag(tier)}
			for _, p := range pools {
				tags = append(tags, tailnet.PoolTag(p))
			}
			key, keyID, err := a.Invite(cmd.Context(), args[0], tags, expires)
			if err != nil {
				return err
			}
			fmt.Fprintf(cmd.OutOrStdout(), "protectorate node join --control %s --key %s --name %s\n", cfg.ServerURL, key, args[0])
			fmt.Fprintf(cmd.ErrOrStderr(), "key is single-use and expires in %s\n", expires)
			if err := recordInvite(cmd.Context(), o, args[0], tier, pools, keyID); err != nil {
				return fmt.Errorf("key issued but the node is not in the inventory (is the hub running?): %w", err)
			}
			fmt.Fprintf(cmd.ErrOrStderr(), "%s is in the inventory as invited, not yet joined\n", args[0])
			return nil
		},
	}
	invite.Flags().StringVar(&tier, "tier", "server", "trust tier: "+strings.Join(tailnet.Tiers, "|"))
	invite.Flags().DurationVar(&expires, "expires", time.Hour, "how long the key can be redeemed")
	invite.Flags().StringSliceVar(&pools, "pool", nil, "build pool to join (repeatable)")

	var control, key, name string
	join := &cobra.Command{
		Use:   "join",
		Short: "Enroll this machine; state goes to $PROTECTORATE_HOME/tsnet",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			home, err := hub.Home()
			if err != nil {
				return err
			}
			if name == "" {
				name = tailnet.SanitizeLabel(node.Hostname())
			}
			ctx, cancel := context.WithTimeout(cmd.Context(), 90*time.Second)
			defer cancel()
			if err := tailnet.Join(ctx, filepath.Join(home, "tsnet"), control, key, name); err != nil {
				return err
			}
			fmt.Fprintf(cmd.OutOrStdout(), "joined as %s; run `protectorate node serve`\n", name)
			return nil
		},
	}
	join.Flags().StringVar(&control, "control", "", "headscale URL")
	join.Flags().StringVar(&key, "key", "", "enrollment key from `node invite`")
	join.Flags().StringVar(&name, "name", "", "tailnet name (default: sanitised hostname)")
	_ = join.MarkFlagRequired("control")
	_ = join.MarkFlagRequired("key")

	revoke := &cobra.Command{
		Use:   "revoke <name>",
		Short: "Remove a node from the tailnet immediately",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			a, _, err := admin(cmd.Context())
			if err != nil {
				return err
			}
			if err := a.Revoke(cmd.Context(), args[0]); err != nil {
				return err
			}
			fmt.Fprintf(cmd.OutOrStdout(), "revoked %s\n", args[0])
			return nil
		},
	}

	renew := &cobra.Command{
		Use:   "renew <name>",
		Short: "Restart a node's key lifetime from now",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			a, _, err := admin(cmd.Context())
			if err != nil {
				return err
			}
			at, err := a.Renew(cmd.Context(), args[0])
			if err != nil {
				return err
			}
			fmt.Fprintf(cmd.OutOrStdout(), "%s valid until %s\n", args[0], at.Format(time.DateOnly))
			return nil
		},
	}

	return []*cobra.Command{invite, join, revoke, renew}
}

// recordInvite puts the invited node into the hub's inventory so it shows in
// `node ls` before it joins.
func recordInvite(ctx context.Context, o *opts, name, tier string, pools []string, keyID uint64) error {
	cl, err := o.nodeClient()
	if err != nil {
		return err
	}
	n := &pb.NodeInfo{Name: name, Address: name, Tier: tier, Invited: true, InviteKeyId: keyID}
	for _, p := range pools {
		n.Tags = append(n.Tags, "pool-"+p)
	}
	_, err = cl.AddNode(ctx, connect.NewRequest(&pb.AddNodeRequest{Node: n, Update: true}))
	return err
}

func clientCmd() *cobra.Command {
	c := &cobra.Command{Use: "client", Short: "Enroll phones and laptops"}
	var name string
	var expires time.Duration
	invite := &cobra.Command{
		Use:   "invite",
		Short: "Create a single-use key for the stock Tailscale app or CLI",
		Args:  cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			a, cfg, err := admin(cmd.Context())
			if err != nil {
				return err
			}
			key, _, err := a.Invite(cmd.Context(), name, []string{tailnet.TagClient}, expires)
			if err != nil {
				return err
			}
			fmt.Fprintf(cmd.OutOrStdout(), `Laptop or desktop (Tailscale installed):
  tailscale up --login-server %[1]s --authkey %[2]s --hostname %[3]s

Phone (Tailscale app): open the account menu, choose "Use an alternate
server" (Android: Settings > Accounts; iOS: Settings > Account), enter
  %[1]s
and sign in. The app cannot take a key, so it shows a registration key; run
  protectorate client register <that key>

Then reach the hub at http://%[4]s:%[5]d. The key is single-use and expires in %[6]s.
`, cfg.ServerURL, key, name, cfg.HubName, tailnet.HubAPIPort, expires)
			return nil
		},
	}
	invite.Flags().StringVar(&name, "name", "", "device name")
	invite.Flags().DurationVar(&expires, "expires", time.Hour, "how long the key can be redeemed")
	_ = invite.MarkFlagRequired("name")

	register := &cobra.Command{
		Use:   "register <registration-key>",
		Short: "Approve a phone that signed in through the Tailscale app",
		Args:  cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			a, _, err := admin(cmd.Context())
			if err != nil {
				return err
			}
			if err := a.RegisterClient(cmd.Context(), args[0]); err != nil {
				return err
			}
			fmt.Fprintln(cmd.OutOrStdout(), "registered as tag:client")
			return nil
		},
	}
	c.AddCommand(invite, register)
	return c
}

// startTailnet brings up headscale and the hub's tailnet listener when the
// tailnet has been initialised; otherwise the hub serves localhost only.
func startTailnet(ctx context.Context, logw io.Writer) (tailnetHub, error) {
	home, err := hub.Home()
	if err != nil {
		return tailnetHub{}, err
	}
	cfg, err := tailnet.LoadConfig(home)
	if errors.Is(err, os.ErrNotExist) {
		return tailnetHub{Close: func() {}}, nil
	}
	if err != nil {
		return tailnetHub{}, err
	}
	logf := func(format string, args ...any) { fmt.Fprintf(logw, "tailnet: "+format+"\n", args...) }
	svc, err := tailnet.Start(ctx, home, cfg, logf)
	if err != nil {
		return tailnetHub{}, err
	}
	ln, err := svc.Net.Listen(tailnet.HubAPIPort)
	if err != nil {
		svc.Close()
		return tailnetHub{}, err
	}
	logf("hub %s serving on tailnet port %d", cfg.HubName, tailnet.HubAPIPort)
	return tailnetHub{
		Extra:  []hub.Extra{{Listener: ln, Identify: svc.Net.IdentifyAPI()}},
		Client: svc.Net.HTTPClient(),
		Peers:  func(ctx context.Context) ([]hub.Peer, error) { return peers(ctx, svc.Admin) },
		Close:  func() { svc.Close() },
	}, nil
}

// tailnetHub is what the hub gets from its tailnet; zero apart from Close
// when none is configured.
type tailnetHub struct {
	Extra  []hub.Extra
	Client *http.Client
	Peers  func(context.Context) ([]hub.Peer, error)
	Close  func()
}

func peers(ctx context.Context, a *tailnet.Admin) ([]hub.Peer, error) {
	nodes, err := a.Nodes(ctx)
	if err != nil {
		return nil, err
	}
	out := make([]hub.Peer, len(nodes))
	for i, n := range nodes {
		out[i] = hub.Peer{Name: n.GivenName, KeyID: n.EnrolledWith()}
	}
	return out, nil
}
