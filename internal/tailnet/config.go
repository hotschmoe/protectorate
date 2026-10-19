package tailnet

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"sort"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

const (
	TagHub    = "tag:hub"
	TagNode   = "tag:node"
	TagClient = "tag:client"

	HubAPIPort = 7470
	NodePort   = 7475

	// policyUser owns every tag; all enrolled machines belong to it.
	policyUser = "protectorate"

	publicDERPMap = "https://controlplane.tailscale.com/derpmap/default"
)

var validLabel = regexp.MustCompile(`^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$`)

// Config holds the tailnet settings chosen at `hub tailnet init`. Everything
// headscale needs is derived from it on each start.
type Config struct {
	ServerURL     string   `json:"server_url"`
	Listen        string   `json:"listen"`
	GRPCListen    string   `json:"grpc_listen"`
	MetricsListen string   `json:"metrics_listen"`
	BaseDomain    string   `json:"base_domain"`
	HubName       string   `json:"hub_name"`
	EmbeddedDERP  bool     `json:"embedded_derp"`
	DERPListen    string   `json:"derp_listen"`
	STUNListen    string   `json:"stun_listen"`
	DERPIPv4      string   `json:"derp_ipv4,omitempty"`
	PublicDERP    bool     `json:"public_derp"`
	TLSCert       string   `json:"tls_cert,omitempty"`
	TLSKey        string   `json:"tls_key,omitempty"`
	KeyExpiryDays int      `json:"key_expiry_days"`
	Pools         []string `json:"pools,omitempty"`
}

// DefaultConfig returns the defaults for serverURL.
func DefaultConfig(serverURL string) Config {
	return Config{
		ServerURL:     serverURL,
		Listen:        "0.0.0.0:7480",
		GRPCListen:    "127.0.0.1:7482",
		MetricsListen: "127.0.0.1:7481",
		BaseDomain:    "tn.protectorate.internal",
		HubName:       "protectorate-hub",
		EmbeddedDERP:  true,
		DERPListen:    "0.0.0.0:7483",
		STUNListen:    "0.0.0.0:3478",
		KeyExpiryDays: 180,
	}
}

func (c Config) Validate() error {
	u, err := url.Parse(c.ServerURL)
	if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Host == "" {
		return fmt.Errorf("server_url %q must be an http(s) URL", c.ServerURL)
	}
	for name, addr := range map[string]string{"listen": c.Listen, "grpc_listen": c.GRPCListen, "metrics_listen": c.MetricsListen} {
		if _, _, err := net.SplitHostPort(addr); err != nil {
			return fmt.Errorf("%s %q: %w", name, addr, err)
		}
	}
	if c.EmbeddedDERP {
		if _, _, err := net.SplitHostPort(c.DERPListen); err != nil {
			return fmt.Errorf("derp_listen %q: %w", c.DERPListen, err)
		}
		if _, _, err := net.SplitHostPort(c.STUNListen); err != nil {
			return fmt.Errorf("stun_listen %q: %w", c.STUNListen, err)
		}
	}
	if !c.EmbeddedDERP && !c.PublicDERP {
		return errors.New("no DERP source: enable embedded_derp or public_derp")
	}
	if (c.TLSCert == "") != (c.TLSKey == "") {
		return errors.New("tls_cert and tls_key must be set together")
	}
	if c.KeyExpiryDays < 1 {
		return errors.New("key_expiry_days must be at least 1")
	}
	if !validLabel.MatchString(c.HubName) {
		return fmt.Errorf("hub_name %q is not a valid DNS label", c.HubName)
	}
	for _, p := range c.Pools {
		if !validLabel.MatchString(p) {
			return fmt.Errorf("pool %q is not a valid DNS label", p)
		}
	}
	return nil
}

// Dir is the tailnet state directory under home.
func Dir(home string) string { return filepath.Join(home, "tailnet") }

func configPath(home string) string { return filepath.Join(Dir(home), "config.json") }

// LoadConfig reads the saved config; errors.Is(err, os.ErrNotExist) means the
// tailnet was never initialised.
func LoadConfig(home string) (Config, error) {
	b, err := os.ReadFile(configPath(home))
	if err != nil {
		return Config{}, fmt.Errorf("read tailnet config: %w", err)
	}
	c := DefaultConfig("")
	if err := json.Unmarshal(b, &c); err != nil {
		return Config{}, fmt.Errorf("parse tailnet config: %w", err)
	}
	return c, c.Validate()
}

func (c Config) Save(home string) error {
	if err := c.Validate(); err != nil {
		return err
	}
	b, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}
	return writeFile(configPath(home), append(b, '\n'), 0o600)
}

func writeFile(path string, data []byte, perm os.FileMode) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return fmt.Errorf("create %s: %w", filepath.Dir(path), err)
	}
	if err := fsutil.WriteFile(path, data, perm); err != nil {
		return fmt.Errorf("write %s: %w", path, err)
	}
	return nil
}

// socketPath keeps the unix socket under the 108-byte sun_path limit, which a
// deep PROTECTORATE_HOME can exceed.
func socketPath(home string) string {
	p := filepath.Join(Dir(home), "headscale.sock")
	if len(p) < 100 {
		return p
	}
	sum := sha256.Sum256([]byte(home))
	return filepath.Join(os.TempDir(), "prot-hs-"+hex.EncodeToString(sum[:4])+".sock")
}

// loopback turns a wildcard listen address into one that can be dialed locally.
func loopback(addr string) string {
	host, port, err := net.SplitHostPort(addr)
	if err != nil {
		return addr
	}
	if ip := net.ParseIP(host); host == "" || (ip != nil && ip.IsUnspecified()) {
		host = "127.0.0.1"
	}
	return net.JoinHostPort(host, port)
}

// HeadscaleConfig renders headscale's config. JSON is valid YAML, so it is
// emitted as JSON from typed maps instead of templated text.
func (c Config) HeadscaleConfig(home string) ([]byte, error) {
	if err := c.Validate(); err != nil {
		return nil, err
	}
	dir := Dir(home)
	derpURLs := []string{}
	if c.PublicDERP {
		derpURLs = append(derpURLs, publicDERPMap)
	}
	derpPaths := []string{}
	if c.EmbeddedDERP {
		derpPaths = append(derpPaths, derpMapPath(home))
	}
	cfg := map[string]any{
		"server_url":          c.ServerURL,
		"listen_addr":         c.Listen,
		"metrics_listen_addr": c.MetricsListen,
		"grpc_listen_addr":    c.GRPCListen,
		"grpc_allow_insecure": false,
		"noise":               map[string]any{"private_key_path": filepath.Join(dir, "noise_private.key")},
		"prefixes": map[string]any{
			"v4":         "100.64.0.0/10",
			"v6":         "fd7a:115c:a1e0::/48",
			"allocation": "sequential",
		},
		"derp": map[string]any{
			"server":              map[string]any{"enabled": false},
			"urls":                derpURLs,
			"paths":               derpPaths,
			"auto_update_enabled": c.PublicDERP,
			"update_frequency":    "3h",
		},
		"disable_check_updates": true,
		"node":                  map[string]any{"expiry": "0"},
		"database": map[string]any{
			"type":   "sqlite",
			"sqlite": map[string]any{"path": filepath.Join(dir, "db.sqlite"), "write_ahead_log": true},
		},
		"log":    map[string]any{"level": "info", "format": "text"},
		"policy": map[string]any{"mode": "file", "path": filepath.Join(dir, "policy.json")},
		"dns": map[string]any{
			"magic_dns":          true,
			"base_domain":        c.BaseDomain,
			"override_local_dns": false,
			"nameservers":        map[string]any{"global": []string{}},
		},
		"unix_socket":            socketPath(home),
		"unix_socket_permission": "0700",
		"logtail":                map[string]any{"enabled": false},
		"taildrop":               map[string]any{"enabled": false},
	}
	if c.TLSCert != "" {
		cfg["tls_cert_path"] = c.TLSCert
		cfg["tls_key_path"] = c.TLSKey
	}
	return json.MarshalIndent(cfg, "", "  ")
}

type aclRule struct {
	Action string   `json:"action"`
	Src    []string `json:"src"`
	Dst    []string `json:"dst"`
}

// PoolTag is the tag that puts a node into build pool name.
func PoolTag(name string) string { return "tag:pool-" + name }

// TierTag is the tag recording a node's trust tier (server or family).
func TierTag(tier string) string { return "tag:tier-" + tier }

// Tiers are the accepted values of `node invite --tier`.
var Tiers = []string{"server", "family"}

// Policy renders the ACL policy. Anything not accepted below is denied.
//
//	hub    -> node:7475, node:22
//	client -> hub:7470
//	pool-X -> pool-X:*   (nodes tagged into the same build pool)
func (c Config) Policy() ([]byte, error) {
	owner := []string{policyUser + "@"}
	owners := map[string][]string{TagHub: owner, TagNode: owner, TagClient: owner}
	for _, t := range Tiers {
		owners[TierTag(t)] = owner
	}
	acls := []aclRule{
		{Action: "accept", Src: []string{TagHub}, Dst: []string{
			fmt.Sprintf("%s:%d", TagNode, NodePort), TagNode + ":22"}},
		{Action: "accept", Src: []string{TagClient}, Dst: []string{
			fmt.Sprintf("%s:%d", TagHub, HubAPIPort)}},
	}
	pools := slices.Clone(c.Pools)
	sort.Strings(pools)
	for _, p := range slices.Compact(pools) {
		t := PoolTag(p)
		owners[t] = owner
		acls = append(acls, aclRule{Action: "accept", Src: []string{t}, Dst: []string{t + ":*"}})
	}
	return json.MarshalIndent(map[string]any{"tagOwners": owners, "acls": acls}, "", "  ")
}

// WriteFiles renders headscale's config and policy into the tailnet dir.
func (c Config) WriteFiles(home string) error {
	hc, err := c.HeadscaleConfig(home)
	if err != nil {
		return err
	}
	pol, err := c.Policy()
	if err != nil {
		return err
	}
	dir := Dir(home)
	if err := writeFile(filepath.Join(dir, "headscale.yaml"), append(hc, '\n'), 0o600); err != nil {
		return err
	}
	if err := writeFile(filepath.Join(dir, "policy.json"), append(pol, '\n'), 0o600); err != nil {
		return err
	}
	if c.EmbeddedDERP {
		return c.writeDERPMap(home)
	}
	return nil
}
