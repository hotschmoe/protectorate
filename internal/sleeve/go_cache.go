package sleeve

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

const DefaultGoCacheMaxBytes = 20 << 30
const goCacheHorizon = 5 * 24 * time.Hour
const goCacheRefresh = 24 * time.Hour

var goSnapshotName = regexp.MustCompile(`^snapshot-[0-9]+-[a-z2-7]+$`)

type goSnapshot struct {
	Name     string    `json:"name"`
	Source   string    `json:"source"`
	Prepared time.Time `json:"prepared"`
	Bytes    uint64    `json:"bytes"`
	Entries  uint64    `json:"entries"`
	stage    string
}

type GoCacheStatus struct {
	Lower          string
	Prepared       time.Time
	Bytes, Entries uint64
	LimitBytes     uint64
	Preparing      bool
	Error          string
}

type goCacheSnapshots struct {
	cfg  Config
	busy func(string) bool
	mu   sync.Mutex
	goSnapshot
	preparing bool
	err       string
}

func (c Config) goCacheDir() string { return filepath.Join(c.Home, "cache", "go-build") }

func (m *Manager) initGoCaches() {
	s := &goCacheSnapshots{cfg: m.cfg, busy: m.cacheSleeveBusy}
	if s.enabled() {
		var current goSnapshot
		if b, err := os.ReadFile(filepath.Join(s.cfg.goCacheDir(), "current.json")); err == nil && json.Unmarshal(b, &current) == nil && goSnapshotName.MatchString(current.Name) {
			if meta, err := s.readSnapshot(current.Name); err == nil && meta.Source == s.cfg.GoCache && meta.Bytes <= s.cfg.GoCacheMaxBytes {
				s.goSnapshot = meta
			}
		}
	}
	m.cfg.goCaches = s
	log.Printf("sleeve runtime: bwrap=%s version=%q overlays=%t nested=%t", m.cfg.bwrap(), m.cfg.BwrapVersion, m.cfg.Overlays, m.cfg.NestedUnsupported == "")
	if m.cfg.NestedUnsupported != "" {
		log.Printf("sleeve runtime: --nested unavailable: %s", m.cfg.NestedUnsupported)
	}
}

func (s *goCacheSnapshots) enabled() bool {
	return s.cfg.Overlays && s.cfg.GoCache != "" && s.cfg.GoCacheMaxBytes > 0
}

func (m *Manager) cacheSleeveBusy(name string) bool {
	m.mu.Lock()
	_, pending := m.pending[name]
	e := m.sleeves[name]
	running := e != nil && e.State == Running
	m.mu.Unlock()
	return pending || running || m.cfg.scopeAlive(name)
}

func (m *Manager) GoCacheStatus() GoCacheStatus {
	s := m.cfg.goCaches
	if s == nil {
		return GoCacheStatus{}
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	out := GoCacheStatus{Prepared: s.Prepared, Bytes: s.Bytes, Entries: s.Entries, LimitBytes: s.cfg.GoCacheMaxBytes, Preparing: s.preparing, Error: s.err}
	if s.Name != "" {
		out.Lower = filepath.Join(s.cfg.goCacheDir(), s.Name, "data")
	}
	return out
}

// Pins are outside the writable home. GC holds the selection lock while
// checking both pending launches and live scopes, including across hub restarts.
func (c Config) pinGoCache(name string) string {
	s := c.goCaches
	if s == nil {
		return ""
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	pin := struct {
		Name string `json:"name"`
	}{Name: s.Name}
	b, _ := json.Marshal(pin)
	if err := os.MkdirAll(c.sleeveDir(name), 0700); err != nil {
		log.Printf("sleeve %s: private GOCACHE (create cache reference: %v)", name, err)
		return ""
	}
	if err := fsutil.WriteFile(filepath.Join(c.sleeveDir(name), "go-cache.json"), b, 0600); err != nil {
		log.Printf("sleeve %s: private GOCACHE (save cache reference: %v)", name, err)
		return ""
	}
	if pin.Name == "" {
		log.Printf("sleeve %s: private GOCACHE (shared snapshot unavailable or preparing)", name)
		return ""
	}
	log.Printf("sleeve %s: GOCACHE overlay on %s", name, pin.Name)
	return filepath.Join(c.goCacheDir(), pin.Name, "data")
}

func (s *goCacheSnapshots) readSnapshot(name string) (goSnapshot, error) {
	var meta goSnapshot
	b, err := os.ReadFile(filepath.Join(s.cfg.goCacheDir(), name, "snapshot.json"))
	if err != nil {
		return meta, fmt.Errorf("read Go snapshot %s: %w", name, err)
	}
	if err := json.Unmarshal(b, &meta); err != nil {
		return meta, fmt.Errorf("decode Go snapshot %s: %w", name, err)
	}
	if meta.Name != name || !goSnapshotName.MatchString(name) || meta.Prepared.IsZero() {
		return meta, fmt.Errorf("invalid Go snapshot %q", name)
	}
	fi, err := os.Lstat(filepath.Join(s.cfg.goCacheDir(), name, "data"))
	if err != nil {
		return meta, fmt.Errorf("inspect Go snapshot data: %w", err)
	}
	if !fi.IsDir() {
		return meta, fmt.Errorf("Go snapshot %s data is not a directory", name)
	}
	return meta, nil
}

func (s *goCacheSnapshots) run(ctx context.Context) {
	if err := s.clearStaging(ctx); err != nil && ctx.Err() == nil {
		log.Printf("discard interrupted Go snapshot staging: %v", err)
	}
	for {
		if s.enabled() {
			s.refresh(ctx)
		} else {
			log.Printf("private GOCACHE: sharing disabled (overlays=%t source=%q limit=%d)", s.cfg.Overlays, s.cfg.GoCache, s.cfg.GoCacheMaxBytes)
		}
		if _, err := s.prune(ctx, PruneOptions{Yes: true, Caches: true}, false); err != nil && ctx.Err() == nil {
			log.Printf("prune Go snapshots: %v", err)
		}
		s.mu.Lock()
		next := s.Prepared.Add(goCacheRefresh)
		s.mu.Unlock()
		if next.Before(time.Now()) {
			next = time.Now().Add(goCacheRefresh)
		}
		timer := time.NewTimer(time.Until(next))
		select {
		case <-ctx.Done():
			timer.Stop()
			return
		case <-timer.C:
		}
	}
}

func (s *goCacheSnapshots) clearStaging(ctx context.Context) error {
	root, err := openDiskRoot(s.cfg.goCacheDir())
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}
	defer root.Close()
	entries, err := fs.ReadDir(root.FS(), ".")
	if err != nil {
		return err
	}
	for _, entry := range entries {
		if err := ctx.Err(); err != nil {
			return err
		}
		if entry.IsDir() && strings.HasPrefix(entry.Name(), ".prepare-") {
			if err := root.RemoveAll(entry.Name()); err != nil {
				return fmt.Errorf("remove interrupted preparation: %w", err)
			}
		}
	}
	return nil
}

func (s *goCacheSnapshots) refresh(ctx context.Context) {
	s.mu.Lock()
	s.preparing, s.err = true, ""
	s.mu.Unlock()
	start := time.Now()
	meta, err := s.prepare(ctx)
	if meta.stage != "" {
		defer os.RemoveAll(meta.stage)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.preparing = false
	if ctx.Err() != nil {
		return
	}
	if err == nil {
		err = os.Rename(meta.stage, filepath.Join(s.cfg.goCacheDir(), meta.Name))
	}
	if err == nil {
		b, _ := json.Marshal(meta)
		err = fsutil.WriteFile(filepath.Join(s.cfg.goCacheDir(), "current.json"), b, 0600)
	}
	if err != nil {
		s.goSnapshot = goSnapshot{}
		s.err = err.Error()
		if clearErr := fsutil.WriteFile(filepath.Join(s.cfg.goCacheDir(), "current.json"), []byte("{}\n"), 0600); clearErr != nil {
			log.Printf("clear failed Go snapshot selection: %v", clearErr)
		}
		log.Printf("private GOCACHE: shared snapshot unavailable after %s: %v (no copy fallback)", time.Since(start).Round(time.Millisecond), err)
		return
	}
	s.goSnapshot = meta
	log.Printf("Go snapshot %s ready in %s: %d indexes, %d logical bytes (hardlinked outputs, copied indexes)", meta.Name, time.Since(start).Round(time.Millisecond), meta.Entries, meta.Bytes)
}

type goCacheIndex struct {
	name, output string
	used         time.Time
	size         uint64
	data         []byte
}

func goCacheID(id string) bool {
	if len(id) != 64 {
		return false
	}
	_, err := hex.DecodeString(id)
	return err == nil
}

func readGoCacheIndex(path, name string, used time.Time) (goCacheIndex, error) {
	entry := goCacheIndex{name: name, used: used}
	b, err := os.ReadFile(path)
	if err != nil {
		return entry, err
	}
	fields := strings.Fields(string(b))
	if len(b) != 175 || len(fields) != 5 || fields[0] != "v1" || fields[1] != name[:64] || !goCacheID(fields[2]) {
		return entry, errors.New("invalid Go cache index")
	}
	entry.size, err = strconv.ParseUint(fields[3], 10, 63)
	if err != nil {
		return entry, fmt.Errorf("parse Go output size: %w", err)
	}
	if _, err := strconv.ParseInt(fields[4], 10, 64); err != nil {
		return entry, fmt.Errorf("parse Go cache time: %w", err)
	}
	entry.output, entry.data = fields[2], b
	return entry, nil
}

// Like Go's GetFile, accept an output only when the referenced size matches.
func goOutput(path string, size uint64) (string, error) {
	fi, err := os.Lstat(path)
	if err != nil {
		return "", err
	}
	if fi.IsDir() {
		entries, err := os.ReadDir(path)
		if err != nil {
			return "", err
		}
		if len(entries) != 1 {
			return "", errors.New("invalid executable cache directory")
		}
		path = filepath.Join(path, entries[0].Name())
		fi, err = os.Lstat(path)
		if err != nil {
			return "", err
		}
	}
	if !fi.Mode().IsRegular() || uint64(fi.Size()) != size {
		return "", errors.New("incomplete Go cache output")
	}
	return path, nil
}

func (s *goCacheSnapshots) prepare(ctx context.Context) (goSnapshot, error) {
	meta := goSnapshot{Source: s.cfg.GoCache}
	root := s.cfg.goCacheDir()
	if err := os.MkdirAll(root, 0700); err != nil {
		return meta, fmt.Errorf("create Go snapshots: %w", err)
	}
	stage, err := os.MkdirTemp(root, ".prepare-")
	if err != nil {
		return meta, fmt.Errorf("stage Go snapshot: %w", err)
	}
	keep := false
	defer func() {
		if !keep {
			os.RemoveAll(stage)
		}
	}()
	dataDir := filepath.Join(stage, "data")
	if err := os.Mkdir(dataDir, 0700); err != nil {
		return meta, fmt.Errorf("create Go snapshot data: %w", err)
	}
	cutoff := time.Now().Add(-goCacheHorizon)
	var candidates []goCacheIndex
	probed := false
	for i := 0; i < 256; i++ {
		if err := ctx.Err(); err != nil {
			return meta, err
		}
		dir := filepath.Join(s.cfg.GoCache, fmt.Sprintf("%02x", i))
		files, err := os.ReadDir(dir)
		if os.IsNotExist(err) {
			continue
		}
		if err != nil {
			return meta, fmt.Errorf("scan owner Go cache: %w", err)
		}
		for _, file := range files {
			if err := ctx.Err(); err != nil {
				return meta, err
			}
			name := file.Name()
			if len(name) != 66 || !strings.HasSuffix(name, "-a") || !goCacheID(name[:64]) || !file.Type().IsRegular() {
				continue
			}
			fi, err := file.Info()
			if err != nil || fi.ModTime().Before(cutoff) || fi.Size() != 175 {
				continue
			}
			entry, err := readGoCacheIndex(filepath.Join(dir, name), name, fi.ModTime())
			if err != nil {
				continue
			}
			if !probed {
				out, err := goOutput(filepath.Join(s.cfg.GoCache, entry.output[:2], entry.output+"-d"), entry.size)
				if err != nil {
					continue
				}
				probe := filepath.Join(stage, ".link-probe")
				if err := os.Link(out, probe); os.IsNotExist(err) {
					continue
				} else if err != nil {
					return meta, fmt.Errorf("hardlink owner Go cache (same filesystem required): %w", err)
				}
				if err := os.Remove(probe); err != nil {
					return meta, fmt.Errorf("remove Go link probe: %w", err)
				}
				probed = true
			}
			candidates = append(candidates, entry)
		}
	}
	slices.SortFunc(candidates, func(a, b goCacheIndex) int {
		if n := b.used.Compare(a.used); n != 0 {
			return n
		}
		return strings.Compare(a.name, b.name)
	})
	outputs := map[string]uint64{}
	for _, entry := range candidates {
		if err := ctx.Err(); err != nil {
			return meta, err
		}
		n := uint64(len(entry.data))
		outputSize, exists := outputs[entry.output]
		if exists && outputSize != entry.size {
			continue
		}
		if !exists {
			n += entry.size
		}
		if n > s.cfg.GoCacheMaxBytes-meta.Bytes {
			continue
		}
		outDir := filepath.Join(dataDir, entry.output[:2])
		if !exists {
			source := filepath.Join(s.cfg.GoCache, entry.output[:2], entry.output+"-d")
			out, err := goOutput(source, entry.size)
			if err != nil {
				continue
			}
			target := filepath.Join(outDir, entry.output+"-d")
			if out != source {
				if err := os.MkdirAll(target, 0700); err != nil {
					return meta, fmt.Errorf("create executable snapshot directory: %w", err)
				}
				target = filepath.Join(target, filepath.Base(out))
			} else if err := os.MkdirAll(outDir, 0700); err != nil {
				return meta, fmt.Errorf("create Go output directory: %w", err)
			}
			if err := os.Link(out, target); os.IsNotExist(err) {
				continue
			} else if err != nil {
				return meta, fmt.Errorf("hardlink Go output: %w", err)
			}
			if fi, err := os.Lstat(target); err != nil || !fi.Mode().IsRegular() || uint64(fi.Size()) != entry.size {
				if err := os.Remove(target); err != nil {
					return meta, fmt.Errorf("discard changed Go output: %w", err)
				}
				continue
			}
			outputs[entry.output] = entry.size
			meta.Bytes += entry.size
		}
		index := filepath.Join(dataDir, entry.name[:2], entry.name)
		if err := os.MkdirAll(filepath.Dir(index), 0700); err != nil {
			return meta, fmt.Errorf("create snapshot index directory: %w", err)
		}
		if err := os.WriteFile(index, entry.data, 0600); err != nil {
			return meta, fmt.Errorf("copy Go action index: %w", err)
		}
		meta.Bytes += uint64(len(entry.data))
		meta.Entries++
	}
	meta.Name = fmt.Sprintf("snapshot-%d-%s", time.Now().UnixNano(), strings.ToLower(rand.Text()))
	meta.Prepared = time.Now().UTC()
	b, _ := json.Marshal(meta)
	if err := os.WriteFile(filepath.Join(stage, "snapshot.json"), b, 0600); err != nil {
		return meta, fmt.Errorf("write snapshot metadata: %w", err)
	}
	meta.stage = stage
	keep = true
	return meta, nil
}

func (s *goCacheSnapshots) inUse(ctx context.Context, generation string) (string, error) {
	names := map[string]bool{}
	entries, err := os.ReadDir(filepath.Join(s.cfg.Home, "sleeves"))
	if err != nil && !os.IsNotExist(err) {
		return "", fmt.Errorf("list cache references: %w", err)
	}
	for _, entry := range entries {
		if ValidName(entry.Name()) == nil {
			names[entry.Name()] = true
		}
	}
	for _, name := range s.cfg.scopeNames() {
		names[name] = true
	}
	for name := range names {
		if err := ctx.Err(); err != nil {
			return "", err
		}
		if !s.busy(name) {
			continue
		}
		var pin struct {
			Name string `json:"name"`
		}
		b, err := os.ReadFile(filepath.Join(s.cfg.sleeveDir(name), "go-cache.json"))
		if err != nil || json.Unmarshal(b, &pin) != nil || (pin.Name != "" && !goSnapshotName.MatchString(pin.Name)) {
			return "running sleeve " + name + " has an unknown cache lower", nil
		}
		if pin.Name == generation {
			return "mounted or launching sleeve " + name, nil
		}
	}
	return "", nil
}

func (s *goCacheSnapshots) prune(ctx context.Context, opts PruneOptions, includeCurrent bool) ([]DiskEntry, error) {
	root, err := openDiskRoot(s.cfg.goCacheDir())
	if errors.Is(err, os.ErrNotExist) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("open Go snapshots: %w", err)
	}
	defer root.Close()
	entries, err := fs.ReadDir(root.FS(), ".")
	if err != nil {
		return nil, fmt.Errorf("list Go snapshots: %w", err)
	}
	var rows []DiskEntry
	for _, entry := range entries {
		name := entry.Name()
		if !entry.IsDir() || !goSnapshotName.MatchString(name) {
			continue
		}
		s.mu.Lock()
		if name == s.Name && !includeCurrent {
			s.mu.Unlock()
			continue
		}
		row := DiskEntry{Path: filepath.Join(s.cfg.goCacheDir(), name), Kind: "shared-go-cache"}
		if meta, err := s.readSnapshot(name); err == nil {
			row.Bytes = meta.Bytes
		}
		reason, err := s.inUse(ctx, name)
		if err != nil {
			s.mu.Unlock()
			return rows, err
		}
		if reason != "" {
			row.Reason = reason
		} else {
			row.Eligible, row.Reason = true, "unreferenced Go cache snapshot (logical retained bytes)"
		}
		if row.Eligible && opts.Yes {
			if name == s.Name {
				if err := fsutil.WriteFile(filepath.Join(s.cfg.goCacheDir(), "current.json"), []byte("{}\n"), 0600); err != nil {
					s.mu.Unlock()
					return rows, fmt.Errorf("clear current Go snapshot: %w", err)
				}
				s.goSnapshot = goSnapshot{}
			}
			// No future selection can reference this non-current generation.
			s.mu.Unlock()
			if err := root.RemoveAll(name); err != nil {
				return rows, fmt.Errorf("remove Go snapshot %s: %w", name, err)
			}
			row.Removed = true
		} else {
			s.mu.Unlock()
		}
		if reason == "" || opts.Running {
			rows = append(rows, row)
		}
	}
	return rows, nil
}
