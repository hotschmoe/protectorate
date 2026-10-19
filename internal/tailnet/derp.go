package tailnet

import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/hex"
	"encoding/json"
	"encoding/pem"
	"errors"
	"fmt"
	"io"
	"math/big"
	"net"
	"net/http"
	"net/netip"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"tailscale.com/derp/derpserver"
	"tailscale.com/net/stun"
	"tailscale.com/types/key"
)

const (
	derpRegionID = 999
	certLifetime = 10 * 365 * 24 * time.Hour
	// A cert this close to expiry is regenerated; every client then learns the
	// new pin from the regenerated DERP map.
	certRenewBefore = 30 * 24 * time.Hour
)

func derpKeyPath(home string) string  { return filepath.Join(Dir(home), "derp_private.key") }
func derpCertPath(home string) string { return filepath.Join(Dir(home), "derp_cert.pem") }
func derpTLSKeyPath(home string) string {
	return filepath.Join(Dir(home), "derp_cert_key.pem")
}
func derpMapPath(home string) string { return filepath.Join(Dir(home), "derpmap.json") }

// derpHost is the name or address clients dial for DERP: the host of the
// control URL.
func (c Config) derpHost() string {
	u, _ := url.Parse(c.ServerURL)
	return u.Hostname()
}

// loadDERPCert returns the hub's self-signed DERP certificate, creating it if
// it is missing, near expiry, or does not cover host.
func loadDERPCert(home, host string) (tls.Certificate, error) {
	certPEM, err1 := os.ReadFile(derpCertPath(home))
	keyPEM, err2 := os.ReadFile(derpTLSKeyPath(home))
	if err1 == nil && err2 == nil {
		if cert, err := tls.X509KeyPair(certPEM, keyPEM); err == nil && certUsable(cert, host) {
			return cert, nil
		}
	}
	key, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		return tls.Certificate{}, fmt.Errorf("generate DERP key: %w", err)
	}
	serial, err := rand.Int(rand.Reader, new(big.Int).Lsh(big.NewInt(1), 62))
	if err != nil {
		return tls.Certificate{}, fmt.Errorf("generate DERP cert serial: %w", err)
	}
	tmpl := &x509.Certificate{
		SerialNumber: serial,
		Subject:      pkix.Name{CommonName: "protectorate-derp"},
		NotBefore:    time.Now().Add(-time.Hour),
		NotAfter:     time.Now().Add(certLifetime),
		KeyUsage:     x509.KeyUsageDigitalSignature,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
	}
	if ip, err := netip.ParseAddr(host); err == nil {
		tmpl.IPAddresses = []net.IP{ip.AsSlice()}
	} else {
		tmpl.DNSNames = []string{host}
	}
	der, err := x509.CreateCertificate(rand.Reader, tmpl, tmpl, &key.PublicKey, key)
	if err != nil {
		return tls.Certificate{}, fmt.Errorf("create DERP cert: %w", err)
	}
	keyDER, err := x509.MarshalPKCS8PrivateKey(key)
	if err != nil {
		return tls.Certificate{}, fmt.Errorf("marshal DERP key: %w", err)
	}
	certPEM = pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: der})
	keyPEM = pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: keyDER})
	if err := writeFile(derpTLSKeyPath(home), keyPEM, 0o600); err != nil {
		return tls.Certificate{}, err
	}
	if err := writeFile(derpCertPath(home), certPEM, 0o644); err != nil {
		return tls.Certificate{}, err
	}
	return tls.X509KeyPair(certPEM, keyPEM)
}

func certUsable(cert tls.Certificate, host string) bool {
	leaf, err := x509.ParseCertificate(cert.Certificate[0])
	if err != nil {
		return false
	}
	return time.Until(leaf.NotAfter) > certRenewBefore && leaf.VerifyHostname(host) == nil
}

func certPin(cert tls.Certificate) string {
	sum := sha256.Sum256(cert.Certificate[0])
	return "sha256-raw:" + hex.EncodeToString(sum[:])
}

// derpMap renders the DERP map file headscale serves. Headscale decodes it
// with a YAML library that ignores JSON tags (lowercased field names) and
// needs an integer region key, so it is emitted as a JSON-style YAML flow
// document.
func (c Config) derpMap(pin string) ([]byte, error) {
	_, port, err := net.SplitHostPort(c.DERPListen)
	if err != nil {
		return nil, fmt.Errorf("derp_listen %q: %w", c.DERPListen, err)
	}
	derpPort, _ := strconv.Atoi(port)
	node := map[string]any{
		"name":     fmt.Sprintf("%da", derpRegionID),
		"regionid": derpRegionID,
		"hostname": c.derpHost(),
		"derpport": derpPort,
		"certname": pin,
	}
	if c.DERPIPv4 != "" {
		node["ipv4"] = c.DERPIPv4
	}
	if _, stunPort, err := net.SplitHostPort(c.STUNListen); err == nil {
		node["stunport"], _ = strconv.Atoi(stunPort)
	} else {
		node["stunport"] = -1
	}
	region, err := json.Marshal(map[string]any{
		"regionid":   derpRegionID,
		"regioncode": "protectorate",
		"regionname": "Protectorate DERP",
		"nodes":      []any{node},
	})
	if err != nil {
		return nil, err
	}
	return fmt.Appendf(nil, `{"regions": {%d: %s}}`+"\n", derpRegionID, region), nil
}

// writeDERPMap makes sure the DERP cert exists and writes the map pinning it.
func (c Config) writeDERPMap(home string) error {
	cert, err := loadDERPCert(home, c.derpHost())
	if err != nil {
		return err
	}
	m, err := c.derpMap(certPin(cert))
	if err != nil {
		return err
	}
	return writeFile(derpMapPath(home), m, 0o600)
}

func loadDERPKey(home string) (key.NodePrivate, error) {
	var k key.NodePrivate
	b, err := os.ReadFile(derpKeyPath(home))
	if errors.Is(err, os.ErrNotExist) {
		k = key.NewNode()
		text, err := k.MarshalText()
		if err != nil {
			return k, err
		}
		return k, writeFile(derpKeyPath(home), text, 0o600)
	}
	if err != nil {
		return k, fmt.Errorf("read DERP key: %w", err)
	}
	if err := k.UnmarshalText(b); err != nil {
		return k, fmt.Errorf("parse DERP key %s: %w", derpKeyPath(home), err)
	}
	return k, nil
}

// DERPServer is the hub's relay: DERP over TLS with a self-signed certificate
// that clients pin by hash through the DERP map, plus STUN. It admits only
// node keys headscale knows.
type DERPServer struct {
	srv  *derpserver.Server
	http *http.Server
	stun net.PacketConn
	wg   sync.WaitGroup
}

// StartDERP serves DERP on cfg.DERPListen and STUN on cfg.STUNListen until
// Close. The headscale config and DERP map must already be written.
func StartDERP(home string, cfg Config, logf func(string, ...any)) (*DERPServer, error) {
	if logf == nil {
		logf = func(string, ...any) {}
	}
	cert, err := loadDERPCert(home, cfg.derpHost())
	if err != nil {
		return nil, err
	}
	priv, err := loadDERPKey(home)
	if err != nil {
		return nil, err
	}
	ln, err := net.Listen("tcp", cfg.DERPListen)
	if err != nil {
		return nil, fmt.Errorf("listen DERP on %s: %w", cfg.DERPListen, err)
	}
	pc, err := net.ListenPacket("udp", cfg.STUNListen)
	if err != nil {
		ln.Close()
		return nil, fmt.Errorf("listen STUN on %s: %w", cfg.STUNListen, err)
	}
	s := derpserver.New(priv, quietDERPLog(logf))
	s.SetVerifyClientURL("http://" + loopback(cfg.Listen) + "/verify")
	mux := http.NewServeMux()
	mux.Handle("/derp", derpserver.Handler(s))
	mux.HandleFunc("/derp/probe", derpserver.ProbeHandler)
	mux.HandleFunc("/derp/latency-check", derpserver.ProbeHandler)
	mux.HandleFunc("/generate_204", derpserver.ServeNoContent)
	d := &DERPServer{
		srv:  s,
		http: &http.Server{Handler: mux, ReadHeaderTimeout: 10 * time.Second},
		stun: pc,
	}
	tlsLn := tls.NewListener(ln, &tls.Config{Certificates: []tls.Certificate{cert}, MinVersion: tls.VersionTLS12})
	d.wg.Go(func() { d.http.Serve(tlsLn) })
	d.wg.Go(func() { serveSTUN(pc) })
	logf("DERP relay on %s (TLS, pinned self-signed cert), STUN on %s", cfg.DERPListen, cfg.STUNListen)
	return d, nil
}

// quietDERPLog drops the per-connection errors derpserver logs when a client
// simply goes away (killed process, dropped link): EOF, reset, closed
// connection. Anything else, including all non-connection messages, passes.
func quietDERPLog(logf func(string, ...any)) func(string, ...any) {
	return func(format string, args ...any) {
		if strings.HasPrefix(format, "derp: ") && len(args) > 0 {
			if err, ok := args[len(args)-1].(error); ok && expectedDisconnect(err) {
				return
			}
		}
		logf(format, args...)
	}
}

func expectedDisconnect(err error) bool {
	return errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF) || errors.Is(err, net.ErrClosed) ||
		errors.Is(err, syscall.ECONNRESET) || errors.Is(err, syscall.EPIPE)
}

func serveSTUN(pc net.PacketConn) {
	buf := make([]byte, 64<<10)
	for {
		n, addr, err := pc.ReadFrom(buf)
		if err != nil {
			return
		}
		ua, ok := addr.(*net.UDPAddr)
		if !ok || !stun.Is(buf[:n]) {
			continue
		}
		tx, err := stun.ParseBindingRequest(buf[:n])
		if err != nil {
			continue
		}
		pc.WriteTo(stun.Response(tx, ua.AddrPort()), addr)
	}
}

func (d *DERPServer) Close() error {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	d.http.Shutdown(ctx)
	d.http.Close()
	d.stun.Close()
	err := d.srv.Close()
	d.wg.Wait()
	return err
}
