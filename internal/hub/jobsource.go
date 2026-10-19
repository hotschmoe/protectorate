package hub

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
)

const maxPrerequisites = 8

var keyChars = regexp.MustCompile(`[^a-z0-9._-]+`)

func gitHub(ctx context.Context, repo string, stdin io.Reader, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, "git", append([]string{"-C", repo}, args...)...)
	cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0")
	cmd.Stdin = stdin
	var out, errb bytes.Buffer
	cmd.Stdout, cmd.Stderr = &out, &errb
	if err := cmd.Run(); err != nil {
		if errors.Is(err, exec.ErrNotFound) {
			return "", errors.New("git is not installed on the hub")
		}
		return "", fmt.Errorf("git %s: %w: %s", args[0], err, strings.TrimSpace(errb.String()))
	}
	return strings.TrimSpace(out.String()), nil
}

// resolveRepoCommit returns the absolute repo path and the full commit id of
// rev (HEAD when empty).
func resolveRepoCommit(ctx context.Context, repo, rev string) (string, string, error) {
	abs, err := filepath.Abs(repo)
	if err != nil {
		return "", "", err
	}
	if fi, err := os.Stat(abs); err != nil || !fi.IsDir() {
		return "", "", fmt.Errorf("repo %q is not a directory on the hub", repo)
	}
	if rev == "" {
		rev = "HEAD"
	}
	commit, err := gitHub(ctx, abs, nil, "rev-parse", "--verify", "--end-of-options", rev+"^{commit}")
	if err != nil {
		return "", "", fmt.Errorf("resolve %q in %s: %w", rev, abs, err)
	}
	if len(commit) != 40 {
		return "", "", errors.New("only SHA-1 repositories are supported")
	}
	return abs, commit, nil
}

// repoKey names the node's cache directory for a hub repository: a readable
// base name plus a hash of the full path so two repos never share a cache.
func repoKey(absRepo string) string {
	base := strings.Trim(keyChars.ReplaceAllString(strings.ToLower(filepath.Base(absRepo)), "-"), "-._")
	if base == "" {
		base = "repo"
	}
	if len(base) > 40 {
		base = base[:40]
	}
	sum := sha256.Sum256([]byte(absRepo))
	return base + "-" + hex.EncodeToString(sum[:4])
}

// usablePrerequisites keeps the node's known commits that the hub repo also
// has, at most maxPrerequisites. A bundle made against them carries only the
// objects the node lacks.
func usablePrerequisites(ctx context.Context, repo, commit string, known []string) []string {
	var out []string
	seen := map[string]bool{commit: true}
	for _, k := range known {
		if len(out) == maxPrerequisites {
			break
		}
		if len(k) != 40 || seen[k] {
			continue
		}
		seen[k] = true
		if _, err := gitHub(ctx, repo, nil, "cat-file", "-e", k+"^{commit}"); err == nil {
			out = append(out, k)
		}
	}
	return out
}

// bundleHeader is the v2 bundle preamble for one ref at commit, listing the
// prerequisites the receiver must already hold. The ref is refs/pc/<commit>,
// the name the node fetches into.
func bundleHeader(commit string, prereqs []string) []byte {
	var b bytes.Buffer
	b.WriteString("# v2 git bundle\n")
	for _, p := range prereqs {
		fmt.Fprintf(&b, "-%s prerequisite\n", p)
	}
	fmt.Fprintf(&b, "%s refs/pc/%s\n\n", commit, commit)
	return b.Bytes()
}

// openBundle streams a git bundle of commit minus everything reachable from
// prereqs. `git bundle create` cannot bundle a bare commit id, so the header
// is written here and the pack comes from pack-objects. finish must be called
// once the stream is drained; it reports a failed pack-objects.
func openBundle(ctx context.Context, repo, commit string, prereqs []string) (io.Reader, func() error, error) {
	var revs bytes.Buffer
	revs.WriteString(commit + "\n")
	for _, p := range prereqs {
		revs.WriteString("^" + p + "\n")
	}
	cmd := exec.CommandContext(ctx, "git", "-C", repo, "pack-objects", "--revs", "--stdout", "--thin", "--delta-base-offset", "-q")
	cmd.Stdin = &revs
	var errb bytes.Buffer
	cmd.Stderr = &errb
	out, err := cmd.StdoutPipe()
	if err != nil {
		return nil, nil, err
	}
	if err := cmd.Start(); err != nil {
		if errors.Is(err, exec.ErrNotFound) {
			return nil, nil, errors.New("git is not installed on the hub")
		}
		return nil, nil, err
	}
	finish := func() error {
		if err := cmd.Wait(); err != nil {
			return fmt.Errorf("git pack-objects: %w: %s", err, strings.TrimSpace(errb.String()))
		}
		return nil
	}
	return io.MultiReader(bytes.NewReader(bundleHeader(commit, prereqs)), out), finish, nil
}
