package main

import (
	"bufio"
	"fmt"
	"io"
	"net"
	"net/netip"
	"os"
	"runtime"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"golang.org/x/net/dns/dnsmessage"
	"gvisor.dev/gvisor/pkg/tcpip"
	"gvisor.dev/gvisor/pkg/tcpip/adapters/gonet"
	"gvisor.dev/gvisor/pkg/tcpip/header"
	"gvisor.dev/gvisor/pkg/tcpip/link/fdbased"
	"gvisor.dev/gvisor/pkg/tcpip/network/arp"
	"gvisor.dev/gvisor/pkg/tcpip/network/ipv4"
	"gvisor.dev/gvisor/pkg/tcpip/stack"
	"gvisor.dev/gvisor/pkg/tcpip/transport/icmp"
	"gvisor.dev/gvisor/pkg/tcpip/transport/tcp"
	"gvisor.dev/gvisor/pkg/tcpip/transport/udp"
	"gvisor.dev/gvisor/pkg/waiter"
)

type natConfig struct {
	Allow           []string
	GatewayLoopback bool
	GatewayPorts    []uint16 // when set, only these host loopback ports are reachable via the gateway
}

type nat struct {
	st       *stack.Stack
	cfg      natConfig
	resolver string

	mu      sync.Mutex
	allowed map[netip.Addr]string // answered IPs of allowed names

	tcpConns, denied, udpFlows atomic.Int64
	bytesIn, bytesOut          atomic.Int64
}

const nicID = 1

func newNAT(tapFD int, cfg natConfig) (*nat, error) {
	mac, _ := net.ParseMAC("52:54:00:12:34:56")
	ep, err := fdbased.New(&fdbased.Options{
		FDs:                []int{tapFD},
		MTU:                1500,
		EthernetHeader:     true,
		Address:            tcpip.LinkAddress(mac),
		PacketDispatchMode: fdbased.Readv,
	})
	if err != nil {
		return nil, fmt.Errorf("fdbased: %w", err)
	}
	st := stack.New(stack.Options{
		NetworkProtocols:   []stack.NetworkProtocolFactory{ipv4.NewProtocol, arp.NewProtocol},
		TransportProtocols: []stack.TransportProtocolFactory{tcp.NewProtocol, udp.NewProtocol, icmp.NewProtocol4},
	})
	if e := st.CreateNIC(nicID, ep); e != nil {
		return nil, fmt.Errorf("CreateNIC: %v", e)
	}
	for _, ip := range []string{gwIP, dnsIP} {
		a := netip.MustParseAddr(ip).As4()
		pa := tcpip.ProtocolAddress{Protocol: ipv4.ProtocolNumber, AddressWithPrefix: tcpip.AddressWithPrefix{Address: tcpip.AddrFrom4(a), PrefixLen: 24}}
		if e := st.AddProtocolAddress(nicID, pa, stack.AddressProperties{}); e != nil {
			return nil, fmt.Errorf("AddProtocolAddress: %v", e)
		}
	}
	st.SetPromiscuousMode(nicID, true)
	st.SetSpoofing(nicID, true)
	st.SetRouteTable([]tcpip.Route{{Destination: header.IPv4EmptySubnet, NIC: nicID}})

	n := &nat{st: st, cfg: cfg, allowed: map[netip.Addr]string{}, resolver: hostResolver()}
	tf := tcp.NewForwarder(st, 0, 1024, n.onTCP)
	st.SetTransportProtocolHandler(tcp.ProtocolNumber, tf.HandlePacket)
	uf := udp.NewForwarder(st, n.onUDP)
	st.SetTransportProtocolHandler(udp.ProtocolNumber, uf.HandlePacket)
	return n, nil
}

func hostResolver() string {
	f, err := os.Open("/etc/resolv.conf")
	if err == nil {
		defer f.Close()
		sc := bufio.NewScanner(f)
		for sc.Scan() {
			if fl := strings.Fields(sc.Text()); len(fl) == 2 && fl[0] == "nameserver" {
				return net.JoinHostPort(fl[1], "53")
			}
		}
	}
	return "127.0.0.1:53"
}

func (n *nat) stats(w io.Writer) {
	var ms runtime.MemStats
	runtime.ReadMemStats(&ms)
	fmt.Fprintf(w, "netns: heap=%dKiB sys=%dKiB\n", ms.HeapAlloc>>10, ms.Sys>>10)
	fmt.Fprintf(w, "netns: tcp=%d denied=%d udp=%d out=%dB in=%dB\n", n.tcpConns.Load(), n.denied.Load(), n.udpFlows.Load(), n.bytesOut.Load(), n.bytesIn.Load())
}

// hostAddr maps a destination seen in the sleeve to a host address, or "" to deny.
func (n *nat) hostAddr(ip netip.Addr, port uint16) string {
	if ip == netip.MustParseAddr(gwIP) {
		if !n.cfg.GatewayLoopback {
			return ""
		}
		if len(n.cfg.GatewayPorts) > 0 && !slices.Contains(n.cfg.GatewayPorts, port) {
			return ""
		}
		return net.JoinHostPort("127.0.0.1", fmt.Sprint(port))
	}
	if len(n.cfg.Allow) > 0 {
		n.mu.Lock()
		_, ok := n.allowed[ip]
		n.mu.Unlock()
		if !ok {
			return ""
		}
	}
	if ip.IsLoopback() || ip.IsLinkLocalUnicast() {
		return ""
	}
	return net.JoinHostPort(ip.String(), fmt.Sprint(port))
}

func (n *nat) onTCP(r *tcp.ForwarderRequest) {
	id := r.ID()
	dst, _ := netip.AddrFromSlice(id.LocalAddress.AsSlice())
	host := n.hostAddr(dst, id.LocalPort)
	if host == "" {
		n.denied.Add(1)
		r.Complete(true)
		return
	}
	out, err := net.DialTimeout("tcp", host, 10*time.Second)
	if err != nil {
		r.Complete(true)
		return
	}
	var wq waiter.Queue
	ep, e := r.CreateEndpoint(&wq)
	if e != nil {
		out.Close()
		r.Complete(true)
		return
	}
	r.Complete(false)
	n.tcpConns.Add(1)
	in := gonet.NewTCPConn(&wq, ep)
	go pipe(in, out, &n.bytesOut, &n.bytesIn)
}

func pipe(a, b net.Conn, ab, ba *atomic.Int64) {
	done := make(chan struct{}, 2)
	cp := func(dst, src net.Conn, c *atomic.Int64) {
		k, _ := io.Copy(dst, src)
		c.Add(k)
		if cw, ok := dst.(interface{ CloseWrite() error }); ok {
			cw.CloseWrite()
		}
		done <- struct{}{}
	}
	go cp(b, a, ab)
	go cp(a, b, ba)
	<-done
	<-done
	a.Close()
	b.Close()
}

func (n *nat) onUDP(r *udp.ForwarderRequest) bool {
	id := r.ID()
	var wq waiter.Queue
	ep, e := r.CreateEndpoint(&wq)
	if e != nil {
		return true
	}
	c := gonet.NewUDPConn(&wq, ep)
	n.udpFlows.Add(1)
	if id.LocalPort == 53 {
		go n.serveDNS(c)
		return true
	}
	dst, _ := netip.AddrFromSlice(id.LocalAddress.AsSlice())
	host := n.hostAddr(dst, id.LocalPort)
	if host == "" {
		c.Close()
		return true
	}
	go func() {
		defer c.Close()
		out, err := net.Dial("udp", host)
		if err != nil {
			return
		}
		defer out.Close()
		go func() {
			buf := make([]byte, 65535)
			for {
				k, err := out.Read(buf)
				if err != nil {
					return
				}
				c.Write(buf[:k])
			}
		}()
		buf := make([]byte, 65535)
		for {
			c.SetReadDeadline(time.Now().Add(30 * time.Second))
			k, err := c.Read(buf)
			if err != nil {
				return
			}
			out.Write(buf[:k])
		}
	}()
	return true
}

// serveDNS relays queries to the host resolver. With an allowlist, names that
// do not match get NXDOMAIN and answers for allowed names feed the IP allowlist.
func (n *nat) serveDNS(c *gonet.UDPConn) {
	defer c.Close()
	buf := make([]byte, 4096)
	for {
		c.SetReadDeadline(time.Now().Add(5 * time.Second))
		k, err := c.Read(buf)
		if err != nil {
			return
		}
		var q dnsmessage.Message
		if q.Unpack(buf[:k]) != nil || len(q.Questions) == 0 {
			continue
		}
		name := strings.TrimSuffix(strings.ToLower(q.Questions[0].Name.String()), ".")
		if len(n.cfg.Allow) > 0 && !n.nameAllowed(name) {
			n.denied.Add(1)
			q.Response, q.RCode = true, dnsmessage.RCodeNameError
			if b, err := q.Pack(); err == nil {
				c.Write(b)
			}
			continue
		}
		up, err := net.DialTimeout("udp", n.resolver, 2*time.Second)
		if err != nil {
			return
		}
		up.SetDeadline(time.Now().Add(3 * time.Second))
		up.Write(buf[:k])
		rb := make([]byte, 4096)
		rk, err := up.Read(rb)
		up.Close()
		if err != nil {
			continue
		}
		var resp dnsmessage.Message
		if resp.Unpack(rb[:rk]) == nil && len(n.cfg.Allow) > 0 {
			for _, a := range resp.Answers {
				if a.Header.Type == dnsmessage.TypeA {
					ip := netip.AddrFrom4(a.Body.(*dnsmessage.AResource).A)
					n.mu.Lock()
					n.allowed[ip] = name
					n.mu.Unlock()
				}
			}
		}
		c.Write(rb[:rk])
	}
}

func (n *nat) nameAllowed(name string) bool {
	for _, a := range n.cfg.Allow {
		if name == a || strings.HasSuffix(name, "."+a) {
			return true
		}
	}
	return false
}

// forward listens on laddr on the host and dials the sleeve's port through the stack.
func (n *nat) forward(laddr, port string) error {
	l, err := net.Listen("tcp", laddr)
	if err != nil {
		return fmt.Errorf("forward listen: %w", err)
	}
	fmt.Fprintf(os.Stderr, "netns: forwarding %s -> sleeve:%s\n", l.Addr(), port)
	var p int
	fmt.Sscan(port, &p)
	sleeve := netip.MustParseAddr(sleeveIP).As4()
	go func() {
		for {
			c, err := l.Accept()
			if err != nil {
				return
			}
			go func() {
				in, err := gonet.DialTCP(n.st, tcpip.FullAddress{NIC: nicID, Addr: tcpip.AddrFrom4(sleeve), Port: uint16(p)}, ipv4.ProtocolNumber)
				if err != nil {
					c.Close()
					return
				}
				pipe(c, in, &n.bytesIn, &n.bytesOut)
			}()
		}
	}()
	return nil
}
