package hub

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"

	"gopkg.in/yaml.v3"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/fsutil"
)

const (
	TierServer = "server"
	TierFamily = "family"
)

var (
	ErrNodeExists   = errors.New("node already exists")
	ErrNodeNotFound = errors.New("node not found")
	ErrInvalidNode  = errors.New("invalid node")

	nodeNameRe = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{0,62}$`)
	sshAliasRe = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._@-]{0,127}$`)
	addrRe     = regexp.MustCompile(`^[A-Za-z0-9._:\[\]/-]+$`)
	tagRe      = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{0,31}$`)
)

type nodeRecord struct {
	Name  string   `yaml:"name"`
	Addr  string   `yaml:"addr"`
	OS    string   `yaml:"os,omitempty"`
	Arch  string   `yaml:"arch,omitempty"`
	Tags  []string `yaml:"tags,omitempty"`
	SSH   string   `yaml:"ssh,omitempty"`
	Tier  string   `yaml:"tier"`
	Notes string   `yaml:"notes,omitempty"`
}

type inventoryFile struct {
	Nodes []nodeRecord `yaml:"nodes"`
}

// Inventory is the persisted node list. Every change rewrites nodes.yaml and
// hosts.md atomically.
type Inventory struct {
	dir   string
	local *pb.NodeInfo

	mu    sync.RWMutex
	nodes []nodeRecord
}

// OpenInventory loads $dir/nodes.yaml, which may not exist yet. local is the
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
	dec := yaml.NewDecoder(bytes.NewReader(raw))
	dec.KnownFields(true)
	if err := dec.Decode(&f); err != nil && !errors.Is(err, io.EOF) {
		return nil, fmt.Errorf("parse %s: %w", inv.path(), err)
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

func (i *Inventory) path() string { return filepath.Join(i.dir, "nodes.yaml") }

// normalize validates r in place semantics and returns the cleaned record.
func normalize(r *nodeRecord) (*nodeRecord, error) {
	n := *r
	n.Name = strings.ToLower(strings.TrimSpace(n.Name))
	n.Addr = strings.TrimSpace(n.Addr)
	n.SSH = strings.TrimSpace(n.SSH)
	n.Tier = strings.ToLower(strings.TrimSpace(n.Tier))
	n.Notes = strings.TrimSpace(n.Notes)
	if n.Tier == "" {
		n.Tier = TierServer
	}
	switch {
	case !nodeNameRe.MatchString(n.Name):
		return nil, fmt.Errorf("%w: name %q must be lowercase letters, digits, '.', '_' or '-'", ErrInvalidNode, n.Name)
	case n.Addr == "" || !addrRe.MatchString(n.Addr):
		return nil, fmt.Errorf("%w: %s: address %q is empty or has invalid characters", ErrInvalidNode, n.Name, n.Addr)
	case n.SSH != "" && !sshAliasRe.MatchString(n.SSH):
		return nil, fmt.Errorf("%w: %s: ssh alias %q has invalid characters", ErrInvalidNode, n.Name, n.SSH)
	case n.Tier != TierServer && n.Tier != TierFamily:
		return nil, fmt.Errorf("%w: %s: tier %q must be %q or %q", ErrInvalidNode, n.Name, n.Tier, TierServer, TierFamily)
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

func (r nodeRecord) info() *pb.NodeInfo {
	return &pb.NodeInfo{
		Name: r.Name, Address: r.Addr, Os: r.OS, Arch: r.Arch, Tags: append([]string(nil), r.Tags...),
		SshAlias: r.SSH, Tier: r.Tier, Notes: r.Notes,
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

func (i *Inventory) Add(n *pb.NodeInfo) (*pb.NodeInfo, error) {
	rec, err := normalize(&nodeRecord{
		Name: n.GetName(), Addr: n.GetAddress(), OS: n.GetOs(), Arch: n.GetArch(), Tags: n.GetTags(),
		SSH: n.GetSshAlias(), Tier: n.GetTier(), Notes: n.GetNotes(),
	})
	if err != nil {
		return nil, err
	}
	i.mu.Lock()
	defer i.mu.Unlock()
	if rec.Name == i.local.GetName() {
		return nil, fmt.Errorf("%w: %s is the hub's own node", ErrNodeExists, rec.Name)
	}
	for _, r := range i.nodes {
		if r.Name == rec.Name {
			return nil, fmt.Errorf("%w: %s", ErrNodeExists, rec.Name)
		}
	}
	return rec.info(), i.commit(append(append([]nodeRecord(nil), i.nodes...), *rec))
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
	raw, err := yaml.Marshal(inventoryFile{Nodes: next})
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
	b.WriteString("Generated by protectorate from nodes.yaml. Do not edit.\n\n")
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
		row(n.Name, n.OS, n.Arch, n.Addr, n.SSH, n.Tier, n.Tags, n.Notes)
	}
	b.WriteString("\n")
	b.WriteString("- Reach a node with `ssh <SSH alias>` when one is set.\n")
	b.WriteString("- Tiers: `server` (machines for unattended work), `family` (machines used by people).\n")
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
