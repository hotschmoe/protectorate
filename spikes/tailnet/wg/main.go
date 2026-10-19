package main

import (
	"crypto/rand"
	"encoding/base64"
	"encoding/hex"
	"flag"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/netip"
	"os"
	"runtime"
	"time"

	"golang.org/x/crypto/curve25519"
	"golang.zx2c4.com/wireguard/conn"
	"golang.zx2c4.com/wireguard/device"
	"golang.zx2c4.com/wireguard/tun/netstack"
)

func hexKey(b64 string) string {
	b, err := base64.StdEncoding.DecodeString(b64)
	if err != nil || len(b) != 32 {
		log.Fatal("bad key")
	}
	return hex.EncodeToString(b)
}

func genkey() {
	var priv, pub [32]byte
	rand.Read(priv[:])
	priv[0] &= 248
	priv[31] = (priv[31] & 127) | 64
	curve25519.ScalarBaseMult(&pub, &priv)
	fmt.Printf("private=%s\npublic=%s\n", base64.StdEncoding.EncodeToString(priv[:]), base64.StdEncoding.EncodeToString(pub[:]))
}

func main() {
	mode := flag.String("mode", "", "genkey | hub | spoke")
	priv := flag.String("priv", "", "own private key (base64)")
	peer := flag.String("peer-pub", "", "peer public key (base64)")
	peerIP := flag.String("peer-ip", "", "peer tunnel IP")
	ip := flag.String("ip", "", "own tunnel IP")
	listen := flag.Int("listen", 0, "UDP listen port (hub)")
	endpoint := flag.String("endpoint", "", "hub host:port (spoke)")
	target := flag.String("target", "", "hub: URL to GET through the tunnel")
	timeout := flag.Duration("timeout", 30*time.Second, "exit after this long")
	flag.Parse()

	if *mode == "genkey" {
		genkey()
		return
	}
	tun, tnet, err := netstack.CreateNetTUN([]netip.Addr{netip.MustParseAddr(*ip)}, nil, 1280)
	if err != nil {
		log.Fatal(err)
	}
	dev := device.NewDevice(tun, conn.NewDefaultBind(), device.NewLogger(device.LogLevelError, "wg "))
	defer dev.Close()
	cfg := "private_key=" + hexKey(*priv) + "\n"
	if *listen > 0 {
		cfg += fmt.Sprintf("listen_port=%d\n", *listen)
	}
	cfg += "public_key=" + hexKey(*peer) + "\nallowed_ip=" + *peerIP + "/32\n"
	if *endpoint != "" {
		cfg += "endpoint=" + *endpoint + "\npersistent_keepalive_interval=15\n"
	}
	if err := dev.IpcSet(cfg); err != nil {
		log.Fatal(err)
	}
	if err := dev.Up(); err != nil {
		log.Fatal(err)
	}

	switch *mode {
	case "spoke":
		ln, err := tnet.ListenTCP(&net.TCPAddr{Port: 8081})
		if err != nil {
			log.Fatal(err)
		}
		mux := http.NewServeMux()
		mux.HandleFunc("/metrics", func(w http.ResponseWriter, r *http.Request) {
			h, _ := os.Hostname()
			log.Printf("ACCEPT %s (only the hub peer can reach us: allowed_ip cryptokey routing)", r.RemoteAddr)
			fmt.Fprintf(w, `{"hostname":%q,"os":%q,"arch":%q,"numcpu":%d}`+"\n", h, runtime.GOOS, runtime.GOARCH, runtime.NumCPU())
		})
		hs := &http.Server{Handler: mux}
		go func() { time.Sleep(*timeout); hs.Close() }()
		hs.Serve(ln)
	case "hub":
		time.Sleep(3 * time.Second)
		cl := &http.Client{Transport: &http.Transport{DialContext: tnet.DialContext}, Timeout: 10 * time.Second}
		var resp *http.Response
		for i := 0; i < 20; i++ {
			resp, err = cl.Get(*target)
			if err == nil {
				break
			}
			time.Sleep(time.Second)
		}
		if err != nil {
			log.Fatal(err)
		}
		b, _ := io.ReadAll(resp.Body)
		fmt.Printf("HTTP %d %s", resp.StatusCode, b)
	}
}
