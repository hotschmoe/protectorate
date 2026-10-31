// Command nettest is test tooling for scripts/net-*.sh, never shipped:
//
//	nettest echo  --listen <unix path|127.0.0.1:port>    node agent stand-in
//	nettest proxy --listen <addr> --upstream <url> --cert <pem> --key <pem>
//	nettest probe --state-dir <dir> --control-url <url> --auth-key <key> --target <ip:port>...
//	nettest dial  --socket <path> --target <host:port> --send <text>
//	nettest status --socket <path>
package main

import (
	"bufio"
	"context"
	"crypto/tls"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/http/httputil"
	"net/netip"
	"net/url"
	"os"
	"slices"
	"strings"
	"time"

	"tailscale.com/net/netns"
	"tailscale.com/tsnet"
)

func main() {
	log.SetFlags(0)
	log.SetPrefix("nettest: ")
	if len(os.Args) < 2 {
		log.Fatal("usage: nettest echo|proxy|probe|dial|status [flags]")
	}
	fs := flag.NewFlagSet(os.Args[1], flag.ExitOnError)
	var err error
	switch os.Args[1] {
	case "echo":
		listen := fs.String("listen", "", "unix socket path or 127.0.0.1:port")
		fs.Parse(os.Args[2:])
		err = echo(*listen)
	case "proxy":
		listen := fs.String("listen", "", "TLS listen address")
		upstream := fs.String("upstream", "", "headscale base URL")
		cert := fs.String("cert", "", "certificate PEM")
		key := fs.String("key", "", "key PEM")
		fs.Parse(os.Args[2:])
		err = proxy(*listen, *upstream, *cert, *key)
	case "probe":
		dir := fs.String("state-dir", "", "tsnet state dir")
		control := fs.String("control-url", "", "control URL")
		authKey := fs.String("auth-key", "", "pre-auth key")
		hold := fs.Duration("hold", time.Minute, "keep serving :7475 this long after probing")
		var targets multi
		fs.Var(&targets, "target", "ip:port to try (repeatable)")
		fs.Parse(os.Args[2:])
		err = probe(*dir, *control, *authKey, targets, *hold)
	case "dial":
		sock := fs.String("socket", "", "member control socket")
		target := fs.String("target", "", "host:port")
		send := fs.String("send", "", "text to send, then half-close")
		fs.Parse(os.Args[2:])
		err = dial(*sock, *target, *send)
	case "status":
		sock := fs.String("socket", "", "member control socket")
		fs.Parse(os.Args[2:])
		err = status(*sock)
	default:
		err = fmt.Errorf("unknown subcommand %q", os.Args[1])
	}
	if err != nil {
		log.Fatal(err)
	}
}

type multi []string

func (m *multi) String() string     { return strings.Join(*m, ",") }
func (m *multi) Set(v string) error { *m = append(*m, v); return nil }

// echo requires the member's header to name tag:hub, writes the header back,
// then echoes every byte.
func echo(listen string) error {
	network := "unix"
	if _, err := netip.ParseAddrPort(listen); err == nil {
		network = "tcp"
	}
	ln, err := net.Listen(network, listen)
	if err != nil {
		return err
	}
	log.Printf("echo listening on %s %s", network, listen)
	for {
		c, err := ln.Accept()
		if err != nil {
			return err
		}
		go func() {
			defer c.Close()
			br := bufio.NewReader(c)
			line, err := br.ReadBytes('\n')
			if err != nil {
				log.Printf("echo: no header: %v", err)
				return
			}
			var h struct{ Tags []string }
			if err := json.Unmarshal(line, &h); err != nil || !slices.Contains(h.Tags, "tag:hub") {
				log.Printf("echo: refusing header %q", line)
				return
			}
			c.Write(line)
			io.Copy(c, br)
			if cw, ok := c.(interface{ CloseWrite() error }); ok {
				cw.CloseWrite()
			}
		}()
	}
}

var controlPaths = []string{"/ts2021", "/key", "/machine/ping-response", "/bootstrap-dns", "/health"}

// proxy stands in for Traefik: TLS termination and one router that passes only
// headscale's control and DERP paths, upgrades included.
func proxy(listen, upstream, certFile, keyFile string) error {
	u, err := url.Parse(upstream)
	if err != nil {
		return err
	}
	rp := httputil.NewSingleHostReverseProxy(u)
	h := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ok := slices.Contains(controlPaths, r.URL.Path) || r.URL.Path == "/derp" || strings.HasPrefix(r.URL.Path, "/derp/")
		log.Printf("proxy %s %s upgrade=%q routed=%v", r.Method, r.URL.Path, r.Header.Get("Upgrade"), ok)
		if !ok {
			http.NotFound(w, r)
			return
		}
		rp.ServeHTTP(w, r)
	})
	cert, err := tls.LoadX509KeyPair(certFile, keyFile)
	if err != nil {
		return err
	}
	srv := &http.Server{Addr: listen, Handler: h, TLSConfig: &tls.Config{Certificates: []tls.Certificate{cert}, NextProtos: []string{"http/1.1"}}}
	log.Printf("proxy listening on %s -> %s", listen, upstream)
	return srv.ListenAndServeTLS("", "")
}

// probe joins as a tag:node peer, serves :7475 so the hub can prove it is
// reachable, and reports whether it could open a TCP flow to each target.
func probe(dir, control, authKey string, targets []string, hold time.Duration) error {
	netns.SetEnabled(false)
	s := &tsnet.Server{Dir: dir, Hostname: "probe", ControlURL: control, AuthKey: authKey, Ephemeral: true,
		UserLogf: func(string, ...any) {}}
	defer s.Close()
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	defer cancel()
	st, err := s.Up(ctx)
	if err != nil {
		return err
	}
	ln, err := s.Listen("tcp", ":7475")
	if err != nil {
		return err
	}
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			go func() { defer c.Close(); io.Copy(c, c) }()
		}
	}()
	out := json.NewEncoder(os.Stdout)
	out.Encode(map[string]any{"event": "up", "ips": st.TailscaleIPs})
	time.Sleep(3 * time.Second)
	for _, t := range targets {
		dctx, dcancel := context.WithTimeout(context.Background(), 5*time.Second)
		c, err := s.Dial(dctx, "tcp", t)
		dcancel()
		res := map[string]any{"event": "probe", "target": t, "connected": err == nil}
		if err != nil {
			res["err"] = err.Error()
		} else {
			c.Close()
		}
		out.Encode(res)
	}
	out.Encode(map[string]any{"event": "done"})
	time.Sleep(hold)
	return nil
}

func dial(sock, target, send string) error {
	c, err := net.DialTimeout("unix", sock, 5*time.Second)
	if err != nil {
		return err
	}
	defer c.Close()
	c.SetDeadline(time.Now().Add(45 * time.Second))
	fmt.Fprintf(c, "DIAL %s\n", target)
	br := bufio.NewReader(c)
	line, err := br.ReadString('\n')
	if err != nil {
		return fmt.Errorf("read DIAL answer: %w", err)
	}
	if line != "OK\n" {
		return errors.New(strings.TrimSpace(line))
	}
	if _, err := io.WriteString(c, send); err != nil {
		return err
	}
	c.(*net.UnixConn).CloseWrite()
	b, err := io.ReadAll(br)
	os.Stdout.Write(b)
	return err
}

func status(sock string) error {
	c, err := net.DialTimeout("unix", sock, 5*time.Second)
	if err != nil {
		return err
	}
	defer c.Close()
	c.SetDeadline(time.Now().Add(15 * time.Second))
	io.WriteString(c, "STATUS\n")
	_, err = io.Copy(os.Stdout, c)
	return err
}
