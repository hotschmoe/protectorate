// Spike B: egress for a --unshare-net sleeve through unix sockets only.
//
//	proxyfwd proxy  -unix /path/proxy.sock [-allow a.com,b.com]   (hub side, outside the sleeve)
//	proxyfwd bridge -unix /path/proxy.sock -listen 127.0.0.1:3128 [-expose /path/expose.sock]  (inside the sleeve)
//	proxyfwd forward -unix /path/expose.sock -listen 127.0.0.1:18000 -port 8000  (hub side: reach a sleeve loopback port)
package main

import (
	"flag"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: proxyfwd proxy|bridge ...")
		os.Exit(2)
	}
	fs := flag.NewFlagSet(os.Args[1], flag.ExitOnError)
	sock := fs.String("unix", "", "unix socket path")
	listen := fs.String("listen", "127.0.0.1:3128", "bridge tcp listen address")
	expose := fs.String("expose", "", "bridge: unix socket on which the hub may request a dial to a sleeve loopback port")
	port := fs.String("port", "", "forward: sleeve loopback port")
	allow := fs.String("allow", "", "comma separated host allowlist (proxy only; empty allows all)")
	fs.Parse(os.Args[2:])
	var err error
	switch os.Args[1] {
	case "proxy":
		err = serveProxy(*sock, strings.Split(*allow, ","))
	case "bridge":
		if *expose != "" {
			go func() { fmt.Fprintln(os.Stderr, "proxyfwd:", serveExpose(*expose)) }()
		}
		err = serveBridge(*sock, *listen)
	case "forward":
		err = serveForward(*sock, *listen, *port)
	default:
		err = fmt.Errorf("unknown mode %q", os.Args[1])
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, "proxyfwd:", err)
		os.Exit(1)
	}
}

func serveBridge(sock, listen string) error {
	l, err := net.Listen("tcp", listen)
	if err != nil {
		return err
	}
	for {
		c, err := l.Accept()
		if err != nil {
			return err
		}
		go func() {
			u, err := net.Dial("unix", sock)
			if err != nil {
				c.Close()
				return
			}
			splice(c, u)
		}()
	}
}

type proxy struct {
	allow []string
	tr    *http.Transport
}

func serveProxy(sock string, allow []string) error {
	os.Remove(sock)
	l, err := net.Listen("unix", sock)
	if err != nil {
		return err
	}
	p := &proxy{tr: &http.Transport{Proxy: nil}}
	for _, a := range allow {
		if a != "" {
			p.allow = append(p.allow, a)
		}
	}
	return (&http.Server{Handler: p, ReadHeaderTimeout: 10 * time.Second}).Serve(l)
}

func (p *proxy) ok(hostport string) bool {
	h, _, err := net.SplitHostPort(hostport)
	if err != nil {
		h = hostport
	}
	if len(p.allow) == 0 {
		return true
	}
	for _, a := range p.allow {
		if h == a || strings.HasSuffix(h, "."+a) {
			return true
		}
	}
	return false
}

func (p *proxy) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	fmt.Fprintf(os.Stderr, "proxy: %s %s\n", r.Method, r.Host)
	if !p.ok(r.Host) {
		http.Error(w, "denied", http.StatusForbidden)
		return
	}
	if r.Method == http.MethodConnect {
		up, err := net.DialTimeout("tcp", r.Host, 10*time.Second)
		if err != nil {
			http.Error(w, err.Error(), http.StatusBadGateway)
			return
		}
		c, _, err := w.(http.Hijacker).Hijack()
		if err != nil {
			up.Close()
			return
		}
		c.Write([]byte("HTTP/1.1 200 Connection Established\r\n\r\n"))
		splice(c, up)
		return
	}
	r.RequestURI = ""
	resp, err := p.tr.RoundTrip(r)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadGateway)
		return
	}
	defer resp.Body.Close()
	for k, v := range resp.Header {
		w.Header()[k] = v
	}
	w.WriteHeader(resp.StatusCode)
	io.Copy(w, resp.Body)
}

func splice(a, b net.Conn) {
	var wg sync.WaitGroup
	wg.Add(2)
	cp := func(d, s net.Conn) {
		defer wg.Done()
		io.Copy(d, s)
		if cw, ok := d.(interface{ CloseWrite() error }); ok {
			cw.CloseWrite()
		}
	}
	go cp(a, b)
	go cp(b, a)
	wg.Wait()
	a.Close()
	b.Close()
}

// serveExpose runs inside the sleeve: each connection starts with "<port>\n"
// and is then spliced to 127.0.0.1:<port> in the sleeve's namespace.
func serveExpose(sock string) error {
	os.Remove(sock)
	l, err := net.Listen("unix", sock)
	if err != nil {
		return err
	}
	for {
		c, err := l.Accept()
		if err != nil {
			return err
		}
		go func() {
			var line []byte
			one := make([]byte, 1)
			for len(line) < 8 {
				if _, err := c.Read(one); err != nil {
					c.Close()
					return
				}
				if one[0] == '\n' {
					break
				}
				line = append(line, one[0])
			}
			u, err := net.Dial("tcp", "127.0.0.1:"+string(line))
			if err != nil {
				c.Close()
				return
			}
			splice(c, u)
		}()
	}
}

func serveForward(sock, listen, port string) error {
	l, err := net.Listen("tcp", listen)
	if err != nil {
		return err
	}
	for {
		c, err := l.Accept()
		if err != nil {
			return err
		}
		go func() {
			u, err := net.Dial("unix", sock)
			if err != nil {
				c.Close()
				return
			}
			fmt.Fprintf(u, "%s\n", port)
			splice(c, u)
		}()
	}
}
