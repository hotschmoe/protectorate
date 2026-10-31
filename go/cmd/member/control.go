package main

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"os"
	"slices"
	"strings"
	"sync"
	"time"

	"tailscale.com/ipn/ipnstate"
)

const (
	maxLine     = 1024
	lineTimeout = 10 * time.Second
	dialTimeout = 30 * time.Second
)

// listenUnix never removes an existing path: a leftover socket is the
// parent's to clean up. A clean shutdown unlinks the socket it created.
func listenUnix(path string) (net.Listener, error) {
	if _, err := os.Lstat(path); err == nil {
		return nil, fmt.Errorf("socket %s already exists", path)
	}
	ln, err := net.Listen("unix", path)
	if err != nil {
		return nil, fmt.Errorf("listen on %s: %w", path, err)
	}
	if err := os.Chmod(path, 0o600); err != nil {
		ln.Close()
		return nil, fmt.Errorf("chmod %s: %w", path, err)
	}
	return ln, nil
}

func (m *member) serveControl(ctx context.Context, ln net.Listener) error {
	go func() {
		<-ctx.Done()
		ln.Close()
	}()
	for {
		conn, err := ln.Accept()
		if err != nil {
			if ctx.Err() != nil {
				return nil
			}
			return fmt.Errorf("accept on control socket: %w", err)
		}
		go m.handleControl(ctx, conn)
	}
}

func (m *member) handleControl(ctx context.Context, conn net.Conn) {
	defer conn.Close()
	conn.SetReadDeadline(time.Now().Add(lineTimeout))
	br := bufio.NewReaderSize(conn, maxLine)
	line, err := br.ReadSlice('\n')
	if err != nil {
		fmt.Fprintf(conn, "ERR read command: %v\n", err)
		return
	}
	conn.SetReadDeadline(time.Time{})
	cmd, arg, _ := strings.Cut(strings.TrimRight(string(line), "\r\n"), " ")
	switch cmd {
	case "STATUS":
		b, err := m.status(ctx)
		if err != nil {
			fmt.Fprintf(conn, "ERR %v\n", err)
			return
		}
		conn.Write(append(b, '\n'))
	case "DIAL":
		if m.c.mode != "hub" {
			fmt.Fprintf(conn, "ERR a node member never dials\n")
			return
		}
		up, err := m.dialNode(ctx, arg)
		if err != nil {
			log.Warn("dial", "target", arg, "err", err)
			fmt.Fprintf(conn, "ERR %v\n", err)
			return
		}
		if _, err := conn.Write([]byte("OK\n")); err != nil {
			up.Close()
			return
		}
		log.Info("dial", "target", arg, "remote", up.RemoteAddr())
		// Bytes the caller sent after its DIAL line are already buffered.
		if n := br.Buffered(); n > 0 {
			pending, _ := br.Peek(n)
			if _, err := up.Write(pending); err != nil {
				up.Close()
				return
			}
		}
		splice(conn, up)
	default:
		fmt.Fprintf(conn, "ERR unknown command %q\n", cmd)
	}
}

// dialNode dials only peers that control tagged tag:node. host may be a
// tailnet IP, the peer's hostname or the first label of its DNS name.
func (m *member) dialNode(ctx context.Context, target string) (net.Conn, error) {
	host, port, err := net.SplitHostPort(target)
	if err != nil {
		return nil, fmt.Errorf("DIAL wants <host>:<port>: %w", err)
	}
	st, err := m.lc.Status(ctx)
	if err != nil {
		return nil, fmt.Errorf("tsnet status: %w", err)
	}
	var peer *ipnstate.PeerStatus
	for _, p := range st.Peer {
		if peerMatches(p, host) {
			peer = p
			break
		}
	}
	if peer == nil || len(peer.TailscaleIPs) == 0 {
		return nil, fmt.Errorf("no peer %q in the netmap", host)
	}
	if peer.Tags == nil || !slices.Contains(peer.Tags.AsSlice(), tagNode) {
		return nil, fmt.Errorf("peer %q is not %s", host, tagNode)
	}
	ctx, cancel := context.WithTimeout(ctx, dialTimeout)
	defer cancel()
	conn, err := m.srv.Dial(ctx, "tcp", net.JoinHostPort(peer.TailscaleIPs[0].String(), port))
	if err != nil {
		return nil, fmt.Errorf("dial %s (%s): %w", host, peer.TailscaleIPs[0], err)
	}
	return conn, nil
}

func peerMatches(p *ipnstate.PeerStatus, host string) bool {
	for _, ip := range p.TailscaleIPs {
		if ip.String() == host {
			return true
		}
	}
	label, _, _ := strings.Cut(p.DNSName, ".")
	return strings.EqualFold(p.HostName, host) || strings.EqualFold(label, host)
}

type peerJSON struct {
	Name          string     `json:"name"`
	DNSName       string     `json:"dns_name"`
	StableID      string     `json:"stable_id"`
	IPs           []string   `json:"ips"`
	Tags          []string   `json:"tags"`
	OS            string     `json:"os,omitempty"`
	Online        bool       `json:"online"`
	Active        bool       `json:"active"`
	CurAddr       string     `json:"cur_addr,omitempty"`
	Relay         string     `json:"relay,omitempty"`
	LastHandshake *time.Time `json:"last_handshake,omitempty"`
	RxBytes       int64      `json:"rx_bytes"`
	TxBytes       int64      `json:"tx_bytes"`
	PeerAPI       []string   `json:"peerapi,omitempty"`
}

func toPeerJSON(p *ipnstate.PeerStatus) peerJSON {
	j := peerJSON{
		Name: p.HostName, DNSName: p.DNSName, StableID: string(p.ID), OS: p.OS,
		Online: p.Online, Active: p.Active, CurAddr: p.CurAddr, Relay: p.Relay,
		RxBytes: p.RxBytes, TxBytes: p.TxBytes, PeerAPI: p.PeerAPIURL,
		IPs: []string{}, Tags: []string{},
	}
	for _, ip := range p.TailscaleIPs {
		j.IPs = append(j.IPs, ip.String())
	}
	if p.Tags != nil {
		j.Tags = p.Tags.AsSlice()
	}
	if !p.LastHandshake.IsZero() {
		t := p.LastHandshake.UTC()
		j.LastHandshake = &t
	}
	return j
}

func (m *member) status(ctx context.Context) ([]byte, error) {
	st, err := m.lc.Status(ctx)
	if err != nil {
		return nil, fmt.Errorf("tsnet status: %w", err)
	}
	out := struct {
		Mode         string     `json:"mode"`
		BackendState string     `json:"backend_state"`
		ControlKey   string     `json:"control_key"`
		Health       []string   `json:"health"`
		Self         *peerJSON  `json:"self,omitempty"`
		Peers        []peerJSON `json:"peers"`
	}{Mode: m.c.mode, BackendState: st.BackendState, ControlKey: m.controlKey, Health: st.Health, Peers: []peerJSON{}}
	if out.Health == nil {
		out.Health = []string{}
	}
	if st.Self != nil {
		self := toPeerJSON(st.Self)
		out.Self = &self
	}
	for _, p := range st.Peer {
		out.Peers = append(out.Peers, toPeerJSON(p))
	}
	slices.SortFunc(out.Peers, func(a, b peerJSON) int { return strings.Compare(a.Name, b.Name) })
	return json.Marshal(out)
}

type closeWriter interface{ CloseWrite() error }

// splice copies both ways. When one side reaches EOF its peer's write half is
// closed, so a request-then-EOF client still gets the full answer.
func splice(a, b net.Conn) {
	var wg sync.WaitGroup
	cp := func(dst, src net.Conn) {
		defer wg.Done()
		_, err := io.Copy(dst, src)
		if cw, ok := dst.(closeWriter); ok && err == nil {
			cw.CloseWrite()
		} else {
			dst.Close()
			src.Close()
		}
	}
	wg.Add(2)
	go cp(a, b)
	go cp(b, a)
	wg.Wait()
	a.Close()
	b.Close()
}
