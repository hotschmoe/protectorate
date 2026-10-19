package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"runtime"
	"slices"
	"strings"
	"time"

	"tailscale.com/tsnet"
)

type metrics struct {
	Hostname string `json:"hostname"`
	OS       string `json:"os"`
	Arch     string `json:"arch"`
	NumCPU   int    `json:"numcpu"`
	Load     string `json:"load"`
	Caller   string `json:"caller"`
}

func load() string {
	b, err := os.ReadFile("/proc/loadavg")
	if err != nil {
		return "n/a"
	}
	return strings.Fields(string(b))[0]
}

func rssKB() string {
	b, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return "n/a"
	}
	for _, l := range strings.Split(string(b), "\n") {
		if strings.HasPrefix(l, "VmRSS:") {
			return strings.TrimSpace(strings.TrimPrefix(l, "VmRSS:"))
		}
	}
	return "n/a"
}

func main() {
	mode := flag.String("mode", "node", "node | call")
	control := flag.String("control", "http://192.168.10.44:8080", "coordination server URL")
	key := flag.String("authkey", "", "preauth key (only needed on first start)")
	host := flag.String("hostname", "", "tailnet hostname")
	state := flag.String("state", "", "state dir")
	target := flag.String("target", "", "call mode: URL to GET over the tailnet")
	port := flag.String("port", "8081", "node mode: tailnet listen port")
	timeout := flag.Duration("timeout", 0, "exit after this long (0 = run until killed)")
	flag.Parse()

	ctx := context.Background()
	if *timeout > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, *timeout)
		defer cancel()
	}

	srv := &tsnet.Server{Hostname: *host, Dir: *state, ControlURL: *control, AuthKey: *key}
	defer srv.Close()

	upCtx, cancel := context.WithTimeout(ctx, 60*time.Second)
	st, err := srv.Up(upCtx)
	cancel()
	if err != nil {
		log.Fatalf("up: %v", err)
	}
	log.Printf("joined: %s ips=%v tags=%v", st.Self.DNSName, st.TailscaleIPs, st.Self.Tags)

	switch *mode {
	case "call":
		cl := srv.HTTPClient()
		cl.Timeout = 30 * time.Second
		var resp *http.Response
		for i := 0; i < 20; i++ {
			resp, err = cl.Get(*target)
			if err == nil {
				break
			}
			time.Sleep(time.Second)
		}
		if err != nil {
			log.Fatalf("call: %v", err)
		}
		defer resp.Body.Close()
		body, _ := io.ReadAll(resp.Body)
		fmt.Printf("HTTP %d %s\n", resp.StatusCode, strings.TrimSpace(string(body)))
	case "node":
		runNode(ctx, srv, *port)
	}
}

func runNode(ctx context.Context, srv *tsnet.Server, port string) {
	lc, err := srv.LocalClient()
	if err != nil {
		log.Fatal(err)
	}
	ln, err := srv.Listen("tcp", ":"+port)
	if err != nil {
		log.Fatal(err)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/metrics", func(w http.ResponseWriter, r *http.Request) {
		who, err := lc.WhoIs(r.Context(), r.RemoteAddr)
		if err != nil || who.Node == nil || !slices.Contains(who.Node.Tags, "tag:hub") {
			caller := r.RemoteAddr
			if err == nil && who.Node != nil {
				caller = who.Node.Name
			}
			log.Printf("REJECT %s", caller)
			http.Error(w, "forbidden: not the hub", http.StatusForbidden)
			return
		}
		h, _ := os.Hostname()
		log.Printf("ACCEPT %s", who.Node.Name)
		json.NewEncoder(w).Encode(metrics{h, runtime.GOOS, runtime.GOARCH, runtime.NumCPU(), load(), who.Node.Name})
	})
	hs := &http.Server{Handler: mux}
	go func() {
		t := time.NewTicker(10 * time.Second)
		defer t.Stop()
		for {
			select {
			case <-t.C:
				log.Printf("rss %s", rssKB())
			case <-ctx.Done():
				hs.Close()
				return
			}
		}
	}()
	log.Printf("serving :%s on tailnet", port)
	if err := hs.Serve(ln); err != nil && ctx.Err() == nil {
		log.Fatal(err)
	}
	log.Printf("timeout reached, exiting")
}
