package main

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/signal"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"
	"github.com/spf13/cobra"

	"github.com/hotschmoe/protectorate/internal/mcpbridge"
)

func mcpCmd(o *opts) *cobra.Command {
	var httpAddr string
	var f mcpbridge.Filter
	c := &cobra.Command{
		Use:   "mcp",
		Short: "Serve the hub API as MCP tools (stdio, or streamable HTTP with --http)",
		Long: `Serve every unary hub RPC as an MCP tool named <service>_<method>.

Without --allow only read-only tools are served. --allow replaces that default
with glob patterns over tool names (use '*' for everything); --deny removes
matches and always wins. These filters only limit what the client is offered.
What the bridge may actually do is set by its token: run it with a scoped token
(--token-file, or env PROTECTORATE_TOKEN) created by 'protectorate token
create'. See docs/mcp.md and docs/auth.md.`,
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			hc, err := o.httpClient()
			if err != nil {
				return err
			}
			srv, err := mcpbridge.NewServer(mcpbridge.Config{
				HubURL: o.hubURL, HTTPClient: hc, Version: version, Filter: f,
			})
			if err != nil {
				return err
			}
			ctx, stop := signal.NotifyContext(cmd.Context(), os.Interrupt)
			defer stop()
			if httpAddr == "" {
				return srv.Run(ctx, &mcp.StdioTransport{})
			}
			l, err := net.Listen("tcp", httpAddr)
			if err != nil {
				return err
			}
			if host, _, _ := net.SplitHostPort(l.Addr().String()); !net.ParseIP(host).IsLoopback() {
				fmt.Fprintln(cmd.ErrOrStderr(), "warning: MCP HTTP endpoint has no authentication and acts with its token; bind to loopback")
			}
			fmt.Fprintf(cmd.ErrOrStderr(), "mcp listening on http://%s\n", l.Addr())
			hs := &http.Server{Handler: mcpbridge.HTTPHandler(srv), ReadHeaderTimeout: 10 * time.Second}
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
	c.Flags().StringVar(&httpAddr, "http", "", "serve streamable HTTP on this address instead of stdio, e.g. 127.0.0.1:7480")
	c.Flags().StringSliceVar(&f.Allow, "allow", nil, "glob of tool names to serve (default: read-only tools); repeatable")
	c.Flags().StringSliceVar(&f.Deny, "deny", nil, "glob of tool names to hide; wins over --allow; repeatable")
	return c
}
