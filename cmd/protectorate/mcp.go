package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"net/netip"
	"os"
	"os/signal"
	"time"

	"github.com/hotschmoe/protectorate/internal/mcpbridge"
)

func mcpCmd(o *opts) *command {
	var httpAddr string
	var list bool
	var f mcpbridge.Filter
	c := &command{
		Use:   "mcp",
		Short: "Serve the hub API as MCP tools (stdio, or streamable HTTP with --http)",
		Long: `Serve every unary hub RPC as an MCP tool named <service>_<method>.

Without --allow only read-only tools are served. --allow replaces that default
with glob patterns over tool names (use '*' for everything); --deny removes
matches and always wins. These filters only limit what the client is offered.
What the bridge may actually do is set by its token: run it with a scoped token
(--token-file, or env PROTECTORATE_TOKEN) created by 'protectorate token
create'. See docs/mcp.md and docs/auth.md. --list prints the catalog the
bridge would serve as JSON and exits, without contacting the hub.`,
		Args: noArgs,
		RunE: func(cmd *command, _ []string) error {
			if list {
				srv, err := mcpbridge.NewServer(mcpbridge.Config{Version: version, Filter: f})
				if err != nil {
					return err
				}
				b, err := json.MarshalIndent(srv.Tools(), "", "  ")
				if err != nil {
					return err
				}
				_, err = fmt.Fprintf(cmd.OutOrStdout(), "%s\n", b)
				return err
			}
			tok, err := o.token()
			if err != nil {
				return err
			}
			hc := &http.Client{Transport: bearer{tok: tok, next: http.DefaultTransport}}
			srv, err := mcpbridge.NewServer(mcpbridge.Config{
				HubURL: o.hubURL, HTTPClient: hc, Version: version, Filter: f,
			})
			if err != nil {
				return err
			}
			ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt)
			defer stop()
			if httpAddr == "" {
				return srv.Run(ctx, os.Stdin, cmd.OutOrStdout())
			}
			if err := checkMCPListen(httpAddr); err != nil {
				return err
			}
			l, err := net.Listen("tcp", httpAddr)
			if err != nil {
				return err
			}
			fmt.Fprintf(cmd.ErrOrStderr(), "mcp listening on http://%s\n", l.Addr())
			hs := &http.Server{Handler: mcpbridge.HTTPHandler(srv, tok), ReadHeaderTimeout: 10 * time.Second}
			go func() {
				<-ctx.Done()
				sctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
				defer cancel()
				hs.Shutdown(sctx)
			}()
			if err := hs.Serve(l); err != http.ErrServerClosed {
				return err
			}
			return nil
		},
	}
	c.Flags().StringVar(&httpAddr, "http", "", "serve streamable HTTP on a loopback or tailnet address instead of stdio, e.g. 127.0.0.1:7471; clients must send the bridge's token as a bearer")
	c.Flags().StringSliceVar(&f.Allow, "allow", nil, "glob of tool names to serve (default: read-only tools); repeatable")
	c.Flags().StringSliceVar(&f.Deny, "deny", nil, "glob of tool names to hide; wins over --allow; repeatable")
	c.Flags().BoolVar(&list, "list", false, "print the served tool catalog as JSON and exit")
	return c
}

var tailnetPrefixes = []netip.Prefix{netip.MustParsePrefix("100.64.0.0/10"), netip.MustParsePrefix("fd7a:115c:a1e0::/48")}

// checkMCPListen refuses an HTTP bind where the bearer would cross a network
// in cleartext: only loopback and tailnet (WireGuard-encrypted) addresses are
// allowed, and an empty host means every interface.
func checkMCPListen(addr string) error {
	host, _, err := net.SplitHostPort(addr)
	if err != nil {
		return fmt.Errorf("--http %q: %w", addr, err)
	}
	if host == "localhost" {
		return nil
	}
	ip, err := netip.ParseAddr(host)
	if err == nil {
		ip = ip.Unmap()
		if ip.IsLoopback() {
			return nil
		}
		for _, p := range tailnetPrefixes {
			if p.Contains(ip) {
				return nil
			}
		}
	}
	return fmt.Errorf("--http %q: serve MCP over plain HTTP only on a loopback or tailnet IP; the bearer token would otherwise cross the network in cleartext", addr)
}
