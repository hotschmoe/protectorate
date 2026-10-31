package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/netip"
	"slices"
	"strings"
	"time"

	"tailscale.com/tailcfg"
)

// header is the one JSON line a node member writes to the agent before the
// spliced bytes: who opened this connection, as control knows it.
type header struct {
	Node     string   `json:"node"`
	StableID string   `json:"stable_id"`
	Tags     []string `json:"tags"`
	IP       string   `json:"ip"`
}

func (m *member) serveNode(ctx context.Context, ln net.Listener) error {
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
			return fmt.Errorf("accept on tailnet :%d: %w", nodePort, err)
		}
		go m.handleNode(ctx, conn)
	}
}

func (m *member) handleNode(ctx context.Context, conn net.Conn) {
	h, err := m.identify(ctx, conn.RemoteAddr().String())
	if err != nil {
		log.Warn("refused", "remote", conn.RemoteAddr(), "err", err)
		conn.Close()
		return
	}
	agent, err := dialAgent(ctx, m.c.forward)
	if err != nil {
		log.Error("forward", "agent", m.c.forward, "err", err)
		conn.Close()
		return
	}
	line, err := json.Marshal(h)
	if err != nil {
		conn.Close()
		agent.Close()
		return
	}
	if _, err := agent.Write(append(line, '\n')); err != nil {
		log.Error("forward header", "agent", m.c.forward, "err", err)
		conn.Close()
		agent.Close()
		return
	}
	log.Info("accepted", "from", h.Node, "ip", h.IP)
	splice(conn, agent)
}

// identify asks the local backend who owns the remote tailnet address. Only
// tag:hub may connect; the agent re-checks the header it gets.
func (m *member) identify(ctx context.Context, remote string) (header, error) {
	ctx, cancel := context.WithTimeout(ctx, lineTimeout)
	defer cancel()
	who, err := m.lc.WhoIs(ctx, remote)
	if err != nil {
		return header{}, fmt.Errorf("whois %s: %w", remote, err)
	}
	if who == nil || who.Node == nil {
		return header{}, fmt.Errorf("whois %s: unknown peer", remote)
	}
	if !slices.Contains(who.Node.Tags, tagHub) {
		return header{}, fmt.Errorf("%s has tags %v, not %s", who.Node.Name, who.Node.Tags, tagHub)
	}
	ap, err := netip.ParseAddrPort(remote)
	if err != nil {
		return header{}, fmt.Errorf("parse remote %q: %w", remote, err)
	}
	return header{
		Node:     strings.TrimSuffix(who.Node.Name, "."),
		StableID: string(who.Node.StableID),
		Tags:     who.Node.Tags,
		IP:       ap.Addr().String(),
	}, nil
}

// dialAgent reaches the local agent at 127.0.0.1:port (or [::1]:port), or
// else at a unix socket path.
func dialAgent(ctx context.Context, forward string) (net.Conn, error) {
	ctx, cancel := context.WithTimeout(ctx, lineTimeout)
	defer cancel()
	var d net.Dialer
	if ap, err := netip.ParseAddrPort(forward); err == nil {
		if !ap.Addr().IsLoopback() {
			return nil, fmt.Errorf("--forward %s is not a loopback address", forward)
		}
		return d.DialContext(ctx, "tcp", forward)
	}
	return d.DialContext(ctx, "unix", forward)
}

const (
	hubPingTimeout  = 10 * time.Second
	hubPingRetry    = 2 * time.Second
	hubPingInterval = 30 * time.Second
)

// keepHubReachable sends disco pings to the hub. A restarted member has a new
// disco key; the hub keeps probing with the old one, which this member cannot
// decrypt, until this side speaks first. Disco is magicsock traffic, below the
// packet filter, and opens no flow to any hub listener.
func (m *member) keepHubReachable(ctx context.Context) {
	wait, failing := time.Duration(0), false
	for {
		select {
		case <-ctx.Done():
			return
		case <-time.After(wait):
		}
		if err := m.pingHub(ctx); err != nil {
			if !failing && ctx.Err() == nil {
				log.Info("hub not reachable yet", "err", err)
			}
			wait, failing = hubPingRetry, true
			continue
		}
		if failing {
			log.Info("hub reachable")
		}
		wait, failing = hubPingInterval, false
	}
}

func (m *member) pingHub(ctx context.Context) error {
	st, err := m.lc.Status(ctx)
	if err != nil {
		return err
	}
	var hub netip.Addr
	for _, p := range st.Peer {
		if p.Tags != nil && slices.Contains(p.Tags.AsSlice(), tagHub) && len(p.TailscaleIPs) > 0 {
			hub = p.TailscaleIPs[0]
			break
		}
	}
	if !hub.IsValid() {
		return errors.New("no tag:hub peer in the netmap")
	}
	ctx, cancel := context.WithTimeout(ctx, hubPingTimeout)
	defer cancel()
	res, err := m.lc.Ping(ctx, hub, tailcfg.PingDisco)
	if err != nil {
		return err
	}
	if res.Err != "" {
		return errors.New(res.Err)
	}
	return nil
}
