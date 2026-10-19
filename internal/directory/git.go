package directory

import (
	"bytes"
	"context"
	"fmt"
	"net/url"
	"os/exec"
	"path"
	"strings"
	"time"
)

const gitTimeout = 10 * time.Second

// Git runs a read-only git command in dir. Repository-controlled hooks and
// helpers are switched off, since the repository is written by a sandboxed
// agent: nothing in a workspace's config may make the hub run a program.
// Only commands that never read the worktree belong here (refs, log, tree to
// tree diffs without textconv): filters run on worktree files, and their
// command comes from the repository config.
func Git(ctx context.Context, dir string, args ...string) (string, error) {
	ctx, cancel := context.WithTimeout(ctx, gitTimeout)
	defer cancel()
	full := append([]string{
		"-C", dir, "--no-pager",
		"-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "-c", "core.pager=cat",
		"-c", "diff.external=", "-c", "core.sshCommand=false", "-c", "safe.directory=" + dir,
		"-c", "log.showSignature=false",
	}, args...)
	cmd := exec.CommandContext(ctx, "git", full...)
	cmd.Env = append(cmd.Environ(), "GIT_OPTIONAL_LOCKS=0", "GIT_TERMINAL_PROMPT=0", "GIT_CONFIG_NOSYSTEM=1", "GIT_EXTERNAL_DIFF=")
	var stdout, stderr bytes.Buffer
	cmd.Stdout, cmd.Stderr = &stdout, &stderr
	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("git %s: %w: %s", args[0], err, strings.TrimSpace(stderr.String()))
	}
	return stdout.String(), nil
}

// Workspace is what the directory learns about one sleeve's workspace.
type Workspace struct {
	Path      string
	Remote    string // normalized origin URL, empty without one
	Branch    string // empty on a detached HEAD
	Head      string
	Manifests []Manifest
}

// Inspect reads the origin remote, branch, HEAD and package manifests of a
// workspace. A directory that is not a git repository still yields manifests.
func Inspect(ctx context.Context, dir string) Workspace {
	w := Workspace{Path: dir, Manifests: ReadManifests(dir)}
	if out, err := Git(ctx, dir, "config", "--get", "remote.origin.url"); err == nil {
		w.Remote = NormalizeRemote(strings.TrimSpace(out))
	}
	if out, err := Git(ctx, dir, "rev-parse", "HEAD"); err == nil {
		w.Head = strings.TrimSpace(out)
	}
	if out, err := Git(ctx, dir, "symbolic-ref", "--short", "-q", "HEAD"); err == nil {
		w.Branch = strings.TrimSpace(out)
	}
	return w
}

// NormalizeRemote reduces the common spellings of one repository to
// "host/owner/repo": scp-like and ssh:// and https:// URLs, with or without
// credentials, a port or a ".git" suffix, in any case. A remote that is a
// filesystem path is returned as a cleaned path.
func NormalizeRemote(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	var host, p string
	switch {
	case strings.Contains(raw, "://"):
		u, err := url.Parse(raw)
		if err != nil {
			return strings.ToLower(strings.TrimSuffix(raw, ".git"))
		}
		if u.Scheme == "file" {
			return path.Clean(strings.TrimSuffix(u.Path, ".git"))
		}
		host, p = u.Hostname(), u.Path
	case strings.HasPrefix(raw, "/") || strings.HasPrefix(raw, "."):
		return path.Clean(strings.TrimSuffix(raw, ".git"))
	default:
		rest := raw
		if at := strings.LastIndex(rest, "@"); at >= 0 {
			rest = rest[at+1:]
		}
		var ok bool
		host, p, ok = strings.Cut(rest, ":")
		if !ok {
			return strings.ToLower(strings.TrimSuffix(raw, ".git"))
		}
	}
	p = strings.Trim(strings.TrimSuffix(strings.Trim(p, "/"), ".git"), "/")
	return strings.ToLower(host + "/" + p)
}
