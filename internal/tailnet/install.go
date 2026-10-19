package tailnet

import (
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"github.com/hotschmoe/protectorate/internal/fsutil"
)

// HeadscaleVersion is the pinned control server. It must be upgraded together
// with the tailscale.com module: headscale rejects clients that are too old.
const HeadscaleVersion = "0.29.5"

// pinnedSHA256 are the release checksums for HeadscaleVersion, compiled in so
// a compromised release page cannot swap the binary and its checksums.txt.
var pinnedSHA256 = map[string]string{
	"linux_amd64": "e979d27fe17d924d33bb6ab7504f6338d0635989a0cfdcfbc0ceb637dbcbe980",
	"linux_arm64": "3afddd207b89ac22cfad4235cc163c6e538cc59ada139e7c0bf47640c5d9e945",
}

const releaseBase = "https://github.com/juanfont/headscale/releases/download/v"

// BinaryPath is where the headscale binary for version lives under home.
func BinaryPath(home, version string) string {
	return filepath.Join(home, "bin", "headscale-"+version)
}

// Install downloads headscale for the host platform into home/bin, verifying
// its sha256 against the published checksums (and the compiled-in pin for the
// pinned version). A binary that fails verification is never moved into place.
func Install(ctx context.Context, home, version string, hc *http.Client) (string, error) {
	return install(ctx, home, version, releaseBase+version, runtime.GOOS, runtime.GOARCH, hc)
}

func install(ctx context.Context, home, version, base, goos, goarch string, hc *http.Client) (string, error) {
	if goos != "linux" {
		return "", fmt.Errorf("headscale runs on the hub, which must be linux (this is %s)", goos)
	}
	if hc == nil {
		hc = &http.Client{Timeout: 10 * time.Minute}
	}
	platform := goos + "_" + goarch
	asset := "headscale_" + version + "_" + platform

	sums, err := fetchText(ctx, hc, base+"/checksums.txt")
	if err != nil {
		return "", err
	}
	want, err := parseChecksum(sums, asset)
	if err != nil {
		return "", err
	}
	if version == HeadscaleVersion {
		pin, ok := pinnedSHA256[platform]
		if !ok {
			return "", fmt.Errorf("no pinned checksum for %s", platform)
		}
		if pin != want {
			return "", fmt.Errorf("published checksum for %s does not match the pinned one", asset)
		}
	}

	dest := BinaryPath(home, version)
	if err := os.MkdirAll(filepath.Dir(dest), 0o755); err != nil {
		return "", fmt.Errorf("create %s: %w", filepath.Dir(dest), err)
	}
	tmp, err := os.CreateTemp(filepath.Dir(dest), ".headscale-download-*")
	if err != nil {
		return "", err
	}
	defer os.Remove(tmp.Name())

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, base+"/"+asset, nil)
	if err != nil {
		return "", err
	}
	res, err := hc.Do(req)
	if err != nil {
		return "", fmt.Errorf("download %s: %w", asset, err)
	}
	defer res.Body.Close()
	if res.StatusCode != http.StatusOK {
		return "", fmt.Errorf("download %s: %s", asset, res.Status)
	}
	h := sha256.New()
	if _, err := io.Copy(io.MultiWriter(tmp, h), res.Body); err != nil {
		return "", fmt.Errorf("download %s: %w", asset, err)
	}
	if err := tmp.Close(); err != nil {
		return "", err
	}
	if got := hex.EncodeToString(h.Sum(nil)); got != want {
		return "", fmt.Errorf("sha256 mismatch for %s: got %s, want %s", asset, got, want)
	}
	if err := os.Chmod(tmp.Name(), 0o755); err != nil {
		return "", err
	}
	if err := fsutil.Rename(tmp.Name(), dest); err != nil {
		return "", fmt.Errorf("install %s: %w", dest, err)
	}
	return dest, nil
}

func fetchText(ctx context.Context, hc *http.Client, url string) (string, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return "", err
	}
	res, err := hc.Do(req)
	if err != nil {
		return "", fmt.Errorf("fetch %s: %w", url, err)
	}
	defer res.Body.Close()
	if res.StatusCode != http.StatusOK {
		return "", fmt.Errorf("fetch %s: %s", url, res.Status)
	}
	b, err := io.ReadAll(io.LimitReader(res.Body, 1<<20))
	if err != nil {
		return "", fmt.Errorf("fetch %s: %w", url, err)
	}
	return string(b), nil
}

// parseChecksum finds asset in sha256sum-format text.
func parseChecksum(sums, asset string) (string, error) {
	sc := bufio.NewScanner(strings.NewReader(sums))
	for sc.Scan() {
		f := strings.Fields(sc.Text())
		if len(f) == 2 && f[1] == asset {
			if _, err := hex.DecodeString(f[0]); err != nil || len(f[0]) != 64 {
				return "", fmt.Errorf("malformed checksum for %s", asset)
			}
			return strings.ToLower(f[0]), nil
		}
	}
	return "", errors.Join(sc.Err(), fmt.Errorf("no checksum published for %s", asset))
}
