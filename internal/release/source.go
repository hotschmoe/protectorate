// Package release fetches and verifies the builds published as GitHub releases.
package release

import (
	"archive/tar"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

var safeVersion = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._+-]{0,159}$`)
var safeRepo = regexp.MustCompile(`^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$`)

func validVersion(version string) bool {
	return safeVersion.MatchString(version) && version != "current" && version != "previous"
}

var Binaries = []string{"protectorate-linux-amd64", "protectorate-linux-arm64", "protectorate-windows-amd64.exe", "protectorate-windows-arm64.exe", "protectorate-android-arm64"}

type Asset struct {
	Name string `json:"name"`
	URL  string `json:"browser_download_url"`
}
type Info struct {
	Tag        string  `json:"tag_name"`
	Name       string  `json:"name"`
	NotesURL   string  `json:"html_url"`
	Prerelease bool    `json:"prerelease"`
	Draft      bool    `json:"draft"`
	Assets     []Asset `json:"assets"`
	Version    string  `json:"version"`
	Channel    string  `json:"channel"`
}
type cached struct {
	ETag string
	Info Info
}

type Source struct {
	Repo, Channel, API, Dir string
	Client                  *http.Client
	mu                      sync.Mutex
	checkMu                 sync.Mutex
	cache                   map[string]cached
}

func DefaultDir() string {
	data := os.Getenv("XDG_DATA_HOME")
	if data == "" {
		home, _ := os.UserHomeDir()
		data = filepath.Join(home, ".local", "share")
	}
	return filepath.Join(data, "protectorate", "releases")
}

func New(repo, channel, api, dir string) (*Source, error) {
	if !safeRepo.MatchString(repo) {
		return nil, fmt.Errorf("release repo %q must be OWNER/REPO", repo)
	}
	if err := ValidateChannel(channel); err != nil {
		return nil, err
	}
	if api == "" {
		api = "https://api.github.com"
	}
	u, err := url.Parse(api)
	if err != nil || u.Host == "" || (u.Scheme != "http" && u.Scheme != "https") || u.User != nil || u.RawQuery != "" || u.Fragment != "" {
		return nil, fmt.Errorf("invalid release API URL %q", api)
	}
	if dir == "" {
		dir = DefaultDir()
	}
	dir, err = filepath.Abs(dir)
	if err != nil {
		return nil, fmt.Errorf("release directory: %w", err)
	}
	client := &http.Client{Timeout: 10 * time.Minute, CheckRedirect: func(req *http.Request, via []*http.Request) error {
		if len(via) >= 10 {
			return errors.New("release request exceeded ten redirects")
		}
		if req.URL.Scheme != "https" && !(u.Scheme == "http" && req.URL.Scheme == "http") {
			return errors.New("release redirect requires HTTPS")
		}
		return nil
	}}
	return &Source{Repo: repo, Channel: channel, API: strings.TrimRight(api, "/"), Dir: dir, Client: client, cache: map[string]cached{}}, nil
}

func ValidateChannel(channel string) error {
	if channel != "stable" && channel != "dev" {
		return fmt.Errorf("update channel %q must be stable or dev", channel)
	}
	return nil
}
func (s *Source) CurrentDir() string { return filepath.Join(s.Dir, "current") }
func (s *Source) Current() Info {
	var r Info
	b, err := os.ReadFile(filepath.Join(s.CurrentDir(), "release.json"))
	if err == nil {
		_ = json.Unmarshal(b, &r)
	}
	return r
}
func (s *Source) Available() Info { s.mu.Lock(); defer s.mu.Unlock(); return s.cache[s.Channel].Info }

func (s *Source) get(ctx context.Context, raw, etag string, maxBytes int64) ([]byte, string, bool, error) {
	u, err := url.Parse(raw)
	if err != nil || u.Host == "" || (u.Scheme != "https" && !(strings.HasPrefix(s.API, "http://") && u.Scheme == "http")) {
		return nil, "", false, fmt.Errorf("invalid asset URL %q", raw)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, raw, nil)
	if err != nil {
		return nil, "", false, fmt.Errorf("release request: %w", err)
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	req.Header.Set("User-Agent", "protectorate-release")
	if etag != "" {
		req.Header.Set("If-None-Match", etag)
	}
	res, err := s.Client.Do(req)
	if err != nil {
		return nil, "", false, fmt.Errorf("fetch %s: %w", raw, err)
	}
	defer res.Body.Close()
	if res.StatusCode == http.StatusNotModified {
		return nil, etag, true, nil
	}
	if res.StatusCode != http.StatusOK {
		return nil, "", false, fmt.Errorf("fetch %s: HTTP %d", raw, res.StatusCode)
	}
	b, err := io.ReadAll(io.LimitReader(res.Body, maxBytes+1))
	if err != nil {
		return nil, "", false, fmt.Errorf("read %s: %w", raw, err)
	}
	if int64(len(b)) > maxBytes {
		return nil, "", false, fmt.Errorf("asset exceeds %d bytes", maxBytes)
	}
	return b, res.Header.Get("ETag"), false, nil
}
func (r Info) asset(name string) (string, error) {
	found := ""
	for _, a := range r.Assets {
		if a.Name == name {
			if found != "" {
				return "", fmt.Errorf("duplicate release asset %s", name)
			}
			found = a.URL
		}
	}
	if found == "" {
		return "", fmt.Errorf("release %s has no %s", r.Tag, name)
	}
	return found, nil
}
func (s *Source) asset(ctx context.Context, r Info, name string, limit int64) ([]byte, error) {
	raw, err := r.asset(name)
	if err != nil {
		return nil, err
	}
	b, _, _, err := s.get(ctx, raw, "", limit)
	return b, err
}

func (s *Source) Check(ctx context.Context, channel, version string) (Info, error) {
	if channel == "" {
		channel = s.Channel
	}
	if err := ValidateChannel(channel); err != nil {
		return Info{}, err
	}
	if version != "" && !validVersion(version) {
		return Info{}, fmt.Errorf("invalid release version %q", version)
	}
	key, path := channel, "latest"
	if channel == "dev" {
		path = "tags/dev"
	}
	if version != "" {
		key = channel + ":" + version
		path = "tags/" + url.PathEscape(version)
	}
	s.checkMu.Lock()
	defer s.checkMu.Unlock()
	s.mu.Lock()
	prev := s.cache[key]
	s.mu.Unlock()
	ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	b, etag, same, err := s.get(ctx, s.API+"/repos/"+s.Repo+"/releases/"+path, prev.ETag, 4<<20)
	if err != nil {
		return Info{}, err
	}
	if same {
		return prev.Info, nil
	}
	var r Info
	if err := json.Unmarshal(b, &r); err != nil {
		return Info{}, fmt.Errorf("parse release: %w", err)
	}
	if r.Draft || (version == "" && ((channel == "stable" && r.Prerelease) || (channel == "dev" && (!r.Prerelease || r.Tag != "dev")))) {
		return Info{}, fmt.Errorf("release %s does not match channel %s", r.Tag, channel)
	}
	r.Channel = channel
	// Stable releases predating VERSION use their immutable version tag.
	r.Version = r.Tag
	if _, err := r.asset("VERSION"); err == nil {
		sums, err := s.asset(ctx, r, "SHA256SUMS", 1<<20)
		if err != nil {
			return Info{}, err
		}
		v, err := s.asset(ctx, r, "VERSION", 1024)
		if err != nil {
			return Info{}, err
		}
		if err := verify("VERSION", v, sums); err != nil {
			return Info{}, err
		}
		r.Version = strings.TrimSpace(string(v))
	} else if r.Tag == "dev" {
		return Info{}, err
	}
	if !validVersion(r.Version) {
		return Info{}, fmt.Errorf("invalid release build version %q", r.Version)
	}
	s.mu.Lock()
	s.cache[key] = cached{ETag: etag, Info: r}
	s.mu.Unlock()
	return r, nil
}

func verify(name string, b, sums []byte) error {
	want := ""
	for _, line := range strings.Split(string(sums), "\n") {
		f := strings.Fields(line)
		if len(f) != 2 || strings.TrimPrefix(f[1], "*") != name {
			continue
		}
		if want != "" {
			return fmt.Errorf("duplicate checksum for %s", name)
		}
		want = f[0]
	}
	hash, err := hex.DecodeString(want)
	if err != nil || len(hash) != sha256.Size {
		return fmt.Errorf("missing or invalid checksum for %s", name)
	}
	actual := sha256.Sum256(b)
	if !strings.EqualFold(hex.EncodeToString(actual[:]), want) {
		return fmt.Errorf("checksum mismatch for %s", name)
	}
	return nil
}

// Fetch publishes a directory only after every fleet platform and the web
// archive have passed verification. Activation is a separate atomic step.
func (s *Source) Fetch(ctx context.Context, r Info, progress func(string, uint64, uint64) error) (string, error) {
	if !validVersion(r.Version) {
		return "", fmt.Errorf("invalid release version %q", r.Version)
	}
	if err := os.MkdirAll(s.Dir, 0700); err != nil {
		return "", fmt.Errorf("create release root: %w", err)
	}
	stage, err := os.MkdirTemp(s.Dir, ".fetch-")
	if err != nil {
		return "", fmt.Errorf("stage release: %w", err)
	}
	defer os.RemoveAll(stage)
	sums, err := s.asset(ctx, r, "SHA256SUMS", 1<<20)
	if err != nil {
		return "", err
	}
	names := append([]string{}, Binaries...)
	names = append(names, "protectorate-web-"+r.Version+".tar.gz")
	if _, err := r.asset("VERSION"); err == nil {
		names = append(names, "VERSION")
	}
	for _, name := range names {
		if err := progress(name, 0, 0); err != nil {
			return "", err
		}
		b, err := s.asset(ctx, r, name, 512<<20)
		if err != nil {
			return "", err
		}
		if err := verify(name, b, sums); err != nil {
			return "", err
		}
		mode := os.FileMode(0755)
		if strings.HasSuffix(name, ".tar.gz") || name == "VERSION" {
			mode = 0644
		}
		if err := fsutil.WriteFile(filepath.Join(stage, name), b, mode); err != nil {
			return "", fmt.Errorf("write asset %s: %w", name, err)
		}
		if strings.HasSuffix(name, ".tar.gz") {
			if err := extractWeb(filepath.Join(stage, name), filepath.Join(stage, "web")); err != nil {
				return "", err
			}
		}
		if err := progress(name, uint64(len(b)), uint64(len(b))); err != nil {
			return "", err
		}
	}
	metadata, err := json.Marshal(r)
	if err != nil {
		return "", fmt.Errorf("encode release: %w", err)
	}
	if err := fsutil.WriteFile(filepath.Join(stage, "release.json"), metadata, 0644); err != nil {
		return "", err
	}
	if err := fsutil.WriteFile(filepath.Join(stage, "SHA256SUMS"), sums, 0644); err != nil {
		return "", err
	}
	dest := filepath.Join(s.Dir, r.Version)
	if _, err := os.Stat(dest); err == nil {
		// Never replace a release still used by current or a rollback trial.
		existing, err := os.ReadFile(filepath.Join(dest, "SHA256SUMS"))
		if err != nil || string(existing) != string(sums) {
			return "", fmt.Errorf("release version %s already exists with different checksums", r.Version)
		}
		for _, name := range names {
			b, err := os.ReadFile(filepath.Join(dest, name))
			if err != nil {
				return "", fmt.Errorf("read cached asset: %w", err)
			}
			if err := verify(name, b, sums); err != nil {
				return "", err
			}
		}
		return dest, nil
	}
	if err := os.Rename(stage, dest); err != nil {
		return "", fmt.Errorf("publish release: %w", err)
	}
	return dest, nil
}
func extractWeb(archive, dir string) error {
	f, err := os.Open(archive)
	if err != nil {
		return fmt.Errorf("open web archive: %w", err)
	}
	defer f.Close()
	gz, err := gzip.NewReader(f)
	if err != nil {
		return fmt.Errorf("open web gzip: %w", err)
	}
	defer gz.Close()
	tr := tar.NewReader(gz)
	var total int64
	for {
		h, err := tr.Next()
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return fmt.Errorf("read web tar: %w", err)
		}
		name := filepath.Clean(filepath.FromSlash(h.Name))
		if filepath.IsAbs(name) || name == ".." || strings.HasPrefix(name, ".."+string(filepath.Separator)) || strings.Contains(h.Name, "\\") {
			return fmt.Errorf("unsafe web path %q", h.Name)
		}
		path := filepath.Join(dir, name)
		switch h.Typeflag {
		case tar.TypeDir:
			if err := os.MkdirAll(path, 0755); err != nil {
				return err
			}
		case tar.TypeReg:
			total += h.Size
			if total > 1<<30 || h.Size < 0 {
				return errors.New("web archive exceeds size limit")
			}
			if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
				return err
			}
			out, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0644)
			if err != nil {
				return fmt.Errorf("create web file: %w", err)
			}
			_, copyErr := io.Copy(out, tr)
			closeErr := out.Close()
			if copyErr != nil {
				return copyErr
			}
			if closeErr != nil {
				return closeErr
			}
		default:
			return fmt.Errorf("web archive contains unsupported entry %q", h.Name)
		}
	}
	if _, err := os.Stat(filepath.Join(dir, "index.html")); err != nil {
		return fmt.Errorf("web archive has no index.html: %w", err)
	}
	return nil
}

func (s *Source) Activate(version string) error {
	if !validVersion(version) {
		return fmt.Errorf("invalid release version %q", version)
	}
	if _, err := os.Stat(filepath.Join(s.Dir, version, "release.json")); err != nil {
		return fmt.Errorf("release metadata: %w", err)
	}
	return s.setPointer("current", version)
}

func (s *Source) setPointer(name, version string) error {
	path := filepath.Join(s.Dir, name)
	if version == "" {
		fi, err := os.Lstat(path)
		if errors.Is(err, os.ErrNotExist) {
			return nil
		}
		if err != nil {
			return fmt.Errorf("inspect %s release pointer: %w", name, err)
		}
		if fi.Mode()&os.ModeSymlink == 0 {
			return fmt.Errorf("refusing to remove %s release pointer that is not a symlink", name)
		}
		if err := os.Remove(path); err != nil {
			return fmt.Errorf("remove %s release pointer: %w", name, err)
		}
		return nil
	}
	tmp, err := os.MkdirTemp(s.Dir, ".pointer-")
	if err != nil {
		return err
	}
	defer os.RemoveAll(tmp)
	link := filepath.Join(tmp, name)
	if err := os.Symlink(version, link); err != nil {
		return fmt.Errorf("stage %s release pointer: %w", name, err)
	}
	if err := os.Rename(link, path); err != nil {
		return fmt.Errorf("switch %s release pointer: %w", name, err)
	}
	return nil
}

func (s *Source) Prune(previous string) error {
	current := s.Current().Version
	if current == "" {
		return nil
	}
	if !validVersion(current) || (previous != "" && !validVersion(previous)) {
		return errors.New("invalid current or previous release version")
	}
	entries, err := os.ReadDir(s.Dir)
	if err != nil {
		return fmt.Errorf("list releases: %w", err)
	}
	type entry struct {
		name string
		mod  time.Time
	}
	var releases []entry
	for _, e := range entries {
		if !e.IsDir() || !validVersion(e.Name()) {
			continue
		}
		if _, err := os.Stat(filepath.Join(s.Dir, e.Name(), "release.json")); err != nil {
			continue
		}
		fi, err := e.Info()
		if err != nil {
			return err
		}
		releases = append(releases, entry{e.Name(), fi.ModTime()})
	}
	sort.Slice(releases, func(i, j int) bool { return releases[i].mod.After(releases[j].mod) })
	keep := map[string]bool{current: true}
	for _, e := range releases {
		if previous != "" && previous != current && e.name == previous {
			keep[previous] = true
		}
	}
	for _, e := range releases {
		if len(keep) < 2 {
			keep[e.name] = true
		}
	}
	previous = ""
	for name := range keep {
		if name != current {
			previous = name
		}
	}
	if err := s.setPointer("previous", previous); err != nil {
		return err
	}
	for _, e := range releases {
		if !keep[e.name] {
			if err := os.RemoveAll(filepath.Join(s.Dir, e.name)); err != nil {
				return fmt.Errorf("prune release %s: %w", e.name, err)
			}
		}
	}
	return nil
}

func (s *Source) Restore(version string) error {
	if version != "" {
		return s.Activate(version)
	}
	return s.setPointer("current", "")
}
