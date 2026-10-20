package hub

import (
	"bytes"
	"cmp"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net/netip"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"sort"
	"strconv"
	"strings"
	"sync"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/fsutil"
)

// Tiers. A shared node is part of the compute pool; a care node is a machine
// kept for a person and is never scheduled or leased.
const (
	TierShared = "shared"
	TierCare   = "care"
)

var (
	ErrNodeExists   = errors.New("node already exists")
	ErrNodeNotFound = errors.New("node not found")
	ErrInvalidNode  = errors.New("invalid node")

	nodeNameRe = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{0,62}$`)
	sshAliasRe = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._@-]{0,127}$`)
	hostRe     = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]*$`)
	tagRe      = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{0,31}$`)
)

type nodeRecord struct {
	Name  string   `json:"name"`
	Addr  string   `json:"addr"`
	OS    string   `json:"os,omitempty"`
	Arch  string   `json:"arch,omitempty"`
	Tags  []string `json:"tags,omitempty"`
	SSH   string   `json:"ssh,omitempty"`
	Tier  string   `json:"tier"`
	Notes string   `json:"notes,omitempty"`

	Invited     bool   `json:"invited,omitempty"`
	InviteKeyID uint64 `json:"invite_key_id,omitempty"`
}

type inventoryFile struct {
	Nodes []nodeRecord `json:"nodes"`
}

// Inventory is the persisted node list. Every change rewrites nodes.json and
// hosts.md atomically.
type Inventory struct {
	dir   string
	local *pb.NodeInfo

	mu    sync.RWMutex
	nodes []nodeRecord
}

// OpenInventory loads $dir/nodes.json, which may not exist yet. local is the
// hub's own node: it is listed first in hosts.md and its name is reserved. A
// corrupt file is an error and is never overwritten.
func OpenInventory(dir string, local *pb.NodeInfo) (*Inventory, error) {
	inv := &Inventory{dir: dir, local: local}
	raw, err := os.ReadFile(inv.path())
	if errors.Is(err, os.ErrNotExist) {
		return inv, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read inventory: %w", err)
	}
	var f inventoryFile
	if err := decodeState(inv.path(), raw, &f); err != nil {
		return nil, err
	}
	if n := retiredTiers(f.Nodes); n > 0 {
		return nil, fmt.Errorf("%w: %s has %d node(s) with tier server or family; stop the hub and run `protectorate node migrate-tiers`", ErrRetiredTiers, inv.path(), n)
	}
	seen := map[string]bool{}
	for i := range f.Nodes {
		n, err := normalize(&f.Nodes[i])
		if err != nil {
			return nil, fmt.Errorf("%s: %w", inv.path(), err)
		}
		if seen[n.Name] || n.Name == local.GetName() {
			return nil, fmt.Errorf("%s: duplicate node %q", inv.path(), n.Name)
		}
		seen[n.Name] = true
		f.Nodes[i] = *n
	}
	sort.Slice(f.Nodes, func(a, b int) bool { return f.Nodes[a].Name < f.Nodes[b].Name })
	inv.nodes = f.Nodes
	return inv, nil
}

func (i *Inventory) path() string { return filepath.Join(i.dir, "nodes.json") }

// decodeState rejects unknown fields while accepting an empty state file.
func decodeState(path string, raw []byte, out any) error {
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.DisallowUnknownFields()
	if err := dec.Decode(out); err != nil && !errors.Is(err, io.EOF) {
		return fmt.Errorf("parse %s: %w", path, err)
	}
	return nil
}

// ErrRetiredTiers marks an inventory written before the tiers were renamed.
var ErrRetiredTiers = errors.New("retired tier names")

// retiredTierNames maps the tiers before 2026-10-20 to their new names.
var retiredTierNames = map[string]string{"server": TierShared, "family": TierCare}

func retiredTiers(nodes []nodeRecord) int {
	n := 0
	for _, r := range nodes {
		if _, ok := retiredTierNames[strings.ToLower(strings.TrimSpace(r.Tier))]; ok {
			n++
		}
	}
	return n
}

// TierChange is one node MigrateTiers renamed.
type TierChange struct{ Node, From, To string }

// MigrateTiers rewrites the retired tiers in dir/nodes.json (server becomes
// shared, family becomes care). It first copies the file to
// nodes.json.pre-tiers, refusing to overwrite an earlier copy, then replaces
// nodes.json atomically and regenerates hosts.md. An inventory without
// retired tiers is left alone. The hub must not be running.
func MigrateTiers(dir string, local *pb.NodeInfo) ([]TierChange, error) {
	path := filepath.Join(dir, "nodes.json")
	raw, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read inventory: %w", err)
	}
	var f inventoryFile
	if err := decodeState(path, raw, &f); err != nil {
		return nil, err
	}
	var changes []TierChange
	for i, r := range f.Nodes {
		if to, ok := retiredTierNames[strings.ToLower(strings.TrimSpace(r.Tier))]; ok {
			changes = append(changes, TierChange{Node: r.Name, From: r.Tier, To: to})
			f.Nodes[i].Tier = to
		}
	}
	if len(changes) == 0 {
		return nil, nil
	}
	backup := path + ".pre-tiers"
	bf, err := os.OpenFile(backup, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
	if err != nil {
		return nil, fmt.Errorf("keep a copy of the old inventory: %w", err)
	}
	if _, err := bf.Write(raw); err != nil {
		bf.Close()
		return nil, fmt.Errorf("write %s: %w", backup, err)
	}
	if err := bf.Close(); err != nil {
		return nil, fmt.Errorf("write %s: %w", backup, err)
	}
	inv := &Inventory{dir: dir, local: local}
	next := make([]nodeRecord, len(f.Nodes))
	for i := range f.Nodes {
		n, err := normalize(&f.Nodes[i])
		if err != nil {
			return nil, fmt.Errorf("%s: %w", path, err)
		}
		next[i] = *n
	}
	inv.mu.Lock()
	defer inv.mu.Unlock()
	if err := inv.commit(next); err != nil {
		return nil, err
	}
	return changes, nil
}

// normalize validates r in place semantics and returns the cleaned record.
func normalize(r *nodeRecord) (*nodeRecord, error) {
	n := *r
	n.Name = strings.ToLower(strings.TrimSpace(n.Name))
	n.Addr = strings.TrimSpace(n.Addr)
	n.SSH = strings.TrimSpace(n.SSH)
	n.Tier = strings.ToLower(strings.TrimSpace(n.Tier))
	n.Notes = strings.TrimSpace(n.Notes)
	if n.Tier == "" {
		n.Tier = TierShared
	}
	switch {
	case !nodeNameRe.MatchString(n.Name):
		return nil, fmt.Errorf("%w: name %q must be lowercase letters, digits, '.', '_' or '-'", ErrInvalidNode, n.Name)
	case !validNodeAddr(n.Addr):
		return nil, fmt.Errorf("%w: %s: address %q must be host, host:port or http(s)://host[:port] with no path or credentials", ErrInvalidNode, n.Name, n.Addr)
	case n.SSH != "" && !sshAliasRe.MatchString(n.SSH):
		return nil, fmt.Errorf("%w: %s: ssh alias %q has invalid characters", ErrInvalidNode, n.Name, n.SSH)
	case !ValidTier(n.Tier):
		return nil, fmt.Errorf("%w: %s: tier %q must be %q or %q", ErrInvalidNode, n.Name, n.Tier, TierShared, TierCare)
	}
	tags := make([]string, 0, len(n.Tags))
	seen := map[string]bool{}
	for _, t := range n.Tags {
		t = strings.ToLower(strings.TrimSpace(t))
		if !tagRe.MatchString(t) {
			return nil, fmt.Errorf("%w: %s: tag %q has invalid characters", ErrInvalidNode, n.Name, t)
		}
		if !seen[t] {
			seen[t] = true
			tags = append(tags, t)
		}
	}
	sort.Strings(tags)
	n.Tags = tags
	return &n, nil
}

// ValidTier reports whether t names a tier.
func ValidTier(t string) bool { return t == TierShared || t == TierCare }

// validNodeAddr accepts only a bare host or IP with an optional port and an
// optional http or https scheme. The node token is sent to this address, so
// paths, credentials, queries and other schemes are refused.
func validNodeAddr(addr string) bool {
	rest := addr
	for _, scheme := range []string{"http://", "https://"} {
		if r, ok := strings.CutPrefix(addr, scheme); ok {
			rest = r
			break
		}
	}
	u, err := url.Parse("http://" + rest)
	if err != nil || u.User != nil || u.Path != "" || u.RawQuery != "" || u.Fragment != "" || u.Host != rest || u.ForceQuery {
		return false
	}
	if p := u.Port(); p != "" {
		if n, err := strconv.Atoi(p); err != nil || n < 1 || n > 65535 {
			return false
		}
	}
	host := u.Hostname()
	if _, err := netip.ParseAddr(host); err == nil {
		return true
	}
	return hostRe.MatchString(host) && !strings.Contains(host, "..")
}

func (r nodeRecord) info() *pb.NodeInfo {
	return &pb.NodeInfo{
		Name: r.Name, Address: r.Addr, Os: r.OS, Arch: r.Arch, Tags: append([]string(nil), r.Tags...),
		SshAlias: r.SSH, Tier: r.Tier, Notes: r.Notes, Invited: r.Invited, InviteKeyId: r.InviteKeyID,
	}
}

// List returns the stored nodes sorted by name, without the local node.
func (i *Inventory) List() []*pb.NodeInfo {
	i.mu.RLock()
	defer i.mu.RUnlock()
	out := make([]*pb.NodeInfo, len(i.nodes))
	for k, r := range i.nodes {
		out[k] = r.info()
	}
	return out
}

func (i *Inventory) Get(name string) (*pb.NodeInfo, error) {
	name = strings.ToLower(name)
	i.mu.RLock()
	defer i.mu.RUnlock()
	for _, r := range i.nodes {
		if r.Name == name {
			return r.info(), nil
		}
	}
	return nil, fmt.Errorf("%w: %s", ErrNodeNotFound, name)
}

func (i *Inventory) Add(n *pb.NodeInfo) (*pb.NodeInfo, error) { return i.put(n, false) }

// Upsert adds n or, when a node of that name exists, replaces its record.
// Address, platform, ssh alias and notes left empty in n keep their stored
// values.
func (i *Inventory) Upsert(n *pb.NodeInfo) (*pb.NodeInfo, error) { return i.put(n, true) }

func (i *Inventory) put(n *pb.NodeInfo, update bool) (*pb.NodeInfo, error) {
	raw := nodeRecord{
		Name: n.GetName(), Addr: n.GetAddress(), OS: n.GetOs(), Arch: n.GetArch(), Tags: n.GetTags(),
		SSH: n.GetSshAlias(), Tier: n.GetTier(), Notes: n.GetNotes(),
		Invited: n.GetInvited(), InviteKeyID: n.GetInviteKeyId(),
	}
	i.mu.Lock()
	defer i.mu.Unlock()
	name := strings.ToLower(strings.TrimSpace(raw.Name))
	at := slices.IndexFunc(i.nodes, func(r nodeRecord) bool { return r.Name == name })
	if at >= 0 && update {
		old := i.nodes[at]
		raw.Addr, raw.OS, raw.Arch = cmp.Or(raw.Addr, old.Addr), cmp.Or(raw.OS, old.OS), cmp.Or(raw.Arch, old.Arch)
		raw.SSH, raw.Notes = cmp.Or(raw.SSH, old.SSH), cmp.Or(raw.Notes, old.Notes)
	}
	rec, err := normalize(&raw)
	if err != nil {
		return nil, err
	}
	if rec.Name == i.local.GetName() {
		return nil, fmt.Errorf("%w: %s is the hub's own node", ErrNodeExists, rec.Name)
	}
	next := append([]nodeRecord(nil), i.nodes...)
	switch {
	case at < 0:
		next = append(next, *rec)
	case update:
		next[at] = *rec
	default:
		return nil, fmt.Errorf("%w: %s", ErrNodeExists, rec.Name)
	}
	return rec.info(), i.commit(next)
}

// ClearInvited records that an invited node has answered a poll.
func (i *Inventory) ClearInvited(name string) error {
	i.mu.Lock()
	defer i.mu.Unlock()
	for k, r := range i.nodes {
		if r.Name != name || !r.Invited {
			continue
		}
		next := append([]nodeRecord(nil), i.nodes...)
		next[k].Invited, next[k].InviteKeyID = false, 0
		return i.commit(next)
	}
	return nil
}

func (i *Inventory) Remove(name string) error {
	name = strings.ToLower(name)
	i.mu.Lock()
	defer i.mu.Unlock()
	for k, r := range i.nodes {
		if r.Name == name {
			next := append(append([]nodeRecord(nil), i.nodes[:k]...), i.nodes[k+1:]...)
			return i.commit(next)
		}
	}
	return fmt.Errorf("%w: %s", ErrNodeNotFound, name)
}

// SetTier changes a stored node's tier.
func (i *Inventory) SetTier(name, tier string) (*pb.NodeInfo, error) {
	if !ValidTier(tier) {
		return nil, fmt.Errorf("%w: tier %q must be %q or %q", ErrInvalidNode, tier, TierShared, TierCare)
	}
	name = strings.ToLower(name)
	i.mu.Lock()
	defer i.mu.Unlock()
	for k, r := range i.nodes {
		if r.Name != name {
			continue
		}
		next := append([]nodeRecord(nil), i.nodes...)
		next[k].Tier = tier
		if err := i.commit(next); err != nil {
			return nil, err
		}
		return next[k].info(), nil
	}
	return nil, fmt.Errorf("%w: %s", ErrNodeNotFound, name)
}

// SetPlatform records the os/arch a node reported; it is a no-op when equal.
func (i *Inventory) SetPlatform(name, osName, arch string) error {
	i.mu.Lock()
	defer i.mu.Unlock()
	for k, r := range i.nodes {
		if r.Name != name {
			continue
		}
		if r.OS == osName && r.Arch == arch {
			return nil
		}
		next := append([]nodeRecord(nil), i.nodes...)
		next[k].OS, next[k].Arch = osName, arch
		return i.commit(next)
	}
	return nil
}

// commit persists next and, only if that succeeded, makes it current.
func (i *Inventory) commit(next []nodeRecord) error {
	sort.Slice(next, func(a, b int) bool { return next[a].Name < next[b].Name })
	raw, err := json.MarshalIndent(inventoryFile{Nodes: next}, "", "  ")
	if err != nil {
		return fmt.Errorf("encode inventory: %w", err)
	}
	if err := os.MkdirAll(i.dir, 0o700); err != nil {
		return fmt.Errorf("create %s: %w", i.dir, err)
	}
	if err := fsutil.WriteFile(i.path(), raw, 0o600); err != nil {
		return fmt.Errorf("write inventory: %w", err)
	}
	i.nodes = next
	if err := fsutil.WriteFile(filepath.Join(i.dir, "hosts.md"), renderHosts(i.local, next), 0o644); err != nil {
		log.Printf("write hosts.md: %v", err)
	}
	return nil
}

// WriteHosts regenerates hosts.md from the current inventory.
func (i *Inventory) WriteHosts() error {
	i.mu.RLock()
	defer i.mu.RUnlock()
	if err := os.MkdirAll(i.dir, 0o700); err != nil {
		return err
	}
	return fsutil.WriteFile(filepath.Join(i.dir, "hosts.md"), renderHosts(i.local, i.nodes), 0o644)
}

func renderHosts(local *pb.NodeInfo, nodes []nodeRecord) []byte {
	var b bytes.Buffer
	b.WriteString("# Hosts\n\n")
	b.WriteString("Generated by protectorate from nodes.json. Do not edit.\n\n")
	b.WriteString("| Name | OS/Arch | Address | SSH alias | Tier | Tags | Notes |\n")
	b.WriteString("|------|---------|---------|-----------|------|------|-------|\n")
	row := func(name, os, arch, addr, ssh, tier string, tags []string, notes string) {
		plat := "?"
		if os != "" {
			plat = os + "/" + arch
		}
		fmt.Fprintf(&b, "| %s | %s | %s | %s | %s | %s | %s |\n",
			cell(name), cell(plat), cell(addr), cell(orDash(ssh)), cell(tier), cell(orDash(strings.Join(tags, ", "))), cell(orDash(notes)))
	}
	row(local.GetName()+" (this machine, hub)", local.GetOs(), local.GetArch(), "local", "", local.GetTier(), local.GetTags(), local.GetNotes())
	for _, n := range nodes {
		notes := n.Notes
		if n.Invited {
			notes = strings.TrimSpace("invited, not yet joined. " + notes)
		}
		row(n.Name, n.OS, n.Arch, n.Addr, n.SSH, n.Tier, n.Tags, notes)
	}
	b.WriteString("\n")
	b.WriteString("- Reach a node with `ssh <SSH alias>` when one is set.\n")
	b.WriteString("- Tiers: `shared` (the compute pool: jobs and leases, sandboxed), `care` (a machine kept for a person: never scheduled or leased).\n")
	b.WriteString("- Live CPU, GPU, memory and disk per node: `protectorate node ls`, `protectorate node show <name>`.\n")
	return b.Bytes()
}

func cell(s string) string {
	s = strings.NewReplacer("|", "/", "\r", " ", "\n", " ").Replace(s)
	return s
}

func orDash(s string) string {
	if s == "" {
		return "-"
	}
	return s
}
