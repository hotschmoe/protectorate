package sleeve

import (
	"context"
	"fmt"
	"io/fs"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"time"
)

var gitObjectID = regexp.MustCompile(`^[0-9a-f]{40}$`)

// Git sees only a fresh bare repository with literal refs and an object
// alternate. Never use -C or --git-dir pointing at an agent-written .git:
// command-line overrides cannot disable arbitrary config includes safely.
type pruneGit struct {
	dir string
	env []string
}

func (g pruneGit) run(ctx context.Context, args ...string) (string, error) {
	base := []string{"--git-dir=" + g.dir, "-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false", "-c", "core.attributesFile=/dev/null", "-c", "protocol.allow=never", "-c", "protocol.file.allow=always", "-c", "protocol.https.allow=always", "-c", "protocol.http.allow=always", "-c", "protocol.ssh.allow=always", "-c", "protocol.git.allow=always"}
	cmd := exec.CommandContext(ctx, "git", append(base, args...)...)
	configurePruneGit(cmd)
	cmd.Env = g.env
	cmd.Dir = g.dir
	out, err := cmd.CombinedOutput()
	if err != nil {
		return "", fmt.Errorf("git %s: %w: %s", args[0], err, strings.TrimSpace(string(out)))
	}
	return string(out), nil
}

func newPruneGit(ctx context.Context) (pruneGit, error) {
	dir, err := os.MkdirTemp("", "protectorate-prune-git-")
	if err != nil {
		return pruneGit{}, fmt.Errorf("create verification directory: %w", err)
	}
	g := pruneGit{dir: dir, env: []string{
		"PATH=" + os.Getenv("PATH"), "HOME=" + dir, "XDG_CONFIG_HOME=" + dir,
		"GIT_CONFIG_NOSYSTEM=1", "GIT_CONFIG_SYSTEM=/dev/null", "GIT_CONFIG_GLOBAL=/dev/null",
		"GIT_TERMINAL_PROMPT=0", "GIT_NO_REPLACE_OBJECTS=1", "GIT_ATTR_NOSYSTEM=1",
		"GIT_SSH_COMMAND=ssh -F /dev/null -oBatchMode=yes -oStrictHostKeyChecking=yes", "LC_ALL=C",
	}}
	if _, err := g.run(ctx, "init", "--bare", "--template=", dir); err != nil {
		os.RemoveAll(dir)
		return pruneGit{}, err
	}
	return g, nil
}

// snapshot imports only plain SHA-1 object stores and literal branch/tag refs.
// Linked worktrees, alternates, shallow repositories and submodules stay put.
func (g pruneGit) snapshot(ctx context.Context, workspace string) error {
	gd := filepath.Join(workspace, ".git")
	return g.snapshotGitDir(ctx, gd)
}

func (g pruneGit) snapshotGitDir(ctx context.Context, gd string) error {
	if fi, err := os.Lstat(gd); err != nil || !fi.IsDir() {
		return fmt.Errorf("not a plain clone")
	}
	for _, name := range []string{"shallow", "commondir", "objects/info/alternates", "objects/info/http-alternates", "info/grafts", "refs/stash"} {
		if _, err := os.Lstat(filepath.Join(gd, name)); !os.IsNotExist(err) {
			return fmt.Errorf("unverifiable repository metadata: %s", name)
		}
	}
	objects := filepath.Join(gd, "objects")
	if strings.ContainsAny(objects, "\n\r") {
		return fmt.Errorf("unsupported object path")
	}
	if err := filepath.WalkDir(objects, func(_ string, d fs.DirEntry, err error) error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if err != nil {
			return err
		}
		if !d.IsDir() && !d.Type().IsRegular() {
			return fmt.Errorf("non-regular object store entry")
		}
		return nil
	}); err != nil {
		return fmt.Errorf("inspect object store: %w", err)
	}
	if err := os.WriteFile(filepath.Join(g.dir, "objects/info/alternates"), []byte(objects+"\n"), 0600); err != nil {
		return fmt.Errorf("install trusted object alternate: %w", err)
	}
	refs := map[string]string{}
	packed, err := readPlainGitFile(gd, "packed-refs")
	if err != nil && !os.IsNotExist(err) {
		return err
	}
	for _, line := range strings.Split(string(packed), "\n") {
		if line == "" || strings.HasPrefix(line, "#") || strings.HasPrefix(line, "^") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) != 2 || !gitObjectID.MatchString(fields[0]) {
			return fmt.Errorf("malformed packed refs")
		}
		if fields[1] == "refs/stash" {
			return fmt.Errorf("repository contains stashed work")
		}
		if len(fields) == 2 && (strings.HasPrefix(fields[1], "refs/heads/") || strings.HasPrefix(fields[1], "refs/tags/")) {
			refs[fields[1]] = fields[0]
		}
	}
	for _, namespace := range []string{"heads", "tags"} {
		path := filepath.Join(gd, "refs", namespace)
		if err := filepath.WalkDir(path, func(p string, d fs.DirEntry, err error) error {
			if os.IsNotExist(err) {
				return nil
			}
			if err != nil {
				return err
			}
			if d.IsDir() {
				return nil
			}
			rel, _ := filepath.Rel(gd, p)
			data, err := readPlainGitFile(gd, rel)
			if err != nil {
				return err
			}
			refs[filepath.ToSlash(rel)] = strings.TrimSpace(string(data))
			return nil
		}); err != nil {
			return fmt.Errorf("read clone refs: %w", err)
		}
	}
	for ref, oid := range refs {
		if !gitObjectID.MatchString(oid) {
			return fmt.Errorf("unverifiable ref %q", ref)
		}
		if _, err := g.run(ctx, "update-ref", ref, oid); err != nil {
			return err
		}
	}
	head, err := readPlainGitFile(gd, "HEAD")
	if err != nil {
		return fmt.Errorf("read clone HEAD: %w", err)
	}
	h := strings.TrimSpace(string(head))
	if ref, ok := strings.CutPrefix(h, "ref: "); ok {
		if !strings.HasPrefix(ref, "refs/heads/") || refs[ref] == "" {
			return fmt.Errorf("unverifiable HEAD")
		}
	} else if !gitObjectID.MatchString(h) {
		return fmt.Errorf("unverifiable HEAD")
	}
	if err := os.WriteFile(filepath.Join(g.dir, "HEAD"), head, 0600); err != nil {
		return fmt.Errorf("install verification HEAD: %w", err)
	}
	return nil
}

func readPlainGitFile(gd, path string) ([]byte, error) {
	root, err := os.OpenRoot(gd)
	if err != nil {
		return nil, err
	}
	defer root.Close()
	part := ""
	for _, component := range strings.Split(filepath.ToSlash(path), "/") {
		part = filepath.Join(part, component)
		fi, err := root.Lstat(part)
		if err != nil {
			return nil, err
		}
		if fi.Mode()&os.ModeSymlink != 0 || (!fi.IsDir() && !fi.Mode().IsRegular()) {
			return nil, fmt.Errorf("non-regular git metadata %s", path)
		}
	}
	return root.ReadFile(path)
}

func verifyPruneWorkspace(ctx context.Context, path, workspaces string) error {
	ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	g, err := newPruneGit(ctx)
	if err != nil {
		return err
	}
	defer os.RemoveAll(g.dir)
	if err := g.snapshot(ctx, path); err != nil {
		return err
	}
	tree, err := g.run(ctx, "ls-tree", "-r", "HEAD")
	if err != nil {
		return err
	}
	for _, line := range strings.Split(tree, "\n") {
		if strings.HasPrefix(line, "160000 ") {
			return fmt.Errorf("submodule checkout cannot be verified")
		}
	}
	index, err := readPlainGitFile(filepath.Join(path, ".git"), "index")
	if err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("read clone index: %w", err)
	}
	if err == nil {
		if err := os.WriteFile(filepath.Join(g.dir, "index"), index, 0600); err != nil {
			return fmt.Errorf("copy clone index: %w", err)
		}
		if _, err := g.run(ctx, "--work-tree="+path, "diff", "--cached", "--no-ext-diff", "--no-textconv", "--quiet", "HEAD", "--"); err != nil {
			return fmt.Errorf("staged changes or unreadable index: %w", err)
		}
		if err := os.Remove(filepath.Join(g.dir, "index")); err != nil {
			return fmt.Errorf("reset verification index: %w", err)
		}
	}
	if _, err := g.run(ctx, "read-tree", "HEAD"); err != nil {
		return err
	}
	if _, err := g.run(ctx, "--work-tree="+path, "diff", "--no-ext-diff", "--no-textconv", "--quiet", "HEAD", "--"); err != nil {
		return fmt.Errorf("uncommitted changes or unreadable worktree: %w", err)
	}
	untracked, err := g.run(ctx, "--work-tree="+path, "ls-files", "--others", "-z", "--")
	if err != nil {
		return err
	}
	if untracked != "" {
		return fmt.Errorf("untracked or ignored files")
	}
	remote, _ := gitDetails(path)
	if remote == "" || strings.HasPrefix(remote, "-") || strings.ContainsAny(remote, "\n\r") {
		return fmt.Errorf("no verifiable origin")
	}
	local := remote
	if u, err := url.Parse(remote); err == nil && u.Scheme == "file" {
		if u.Host != "" && u.Host != "localhost" {
			return fmt.Errorf("non-local file origin")
		}
		local = u.Path
	}
	if filepath.IsAbs(local) {
		real, err := filepath.EvalSymlinks(local)
		if err != nil {
			return fmt.Errorf("resolve origin: %w", err)
		}
		if within(real, workspaces) || within(path, real) || within(real, path) {
			return fmt.Errorf("origin is not independent of hub clones")
		}
		// A local fetch normally runs upload-pack with the source config. Use a
		// second trusted snapshot so source hooks/config cannot run either.
		source, err := newPruneGit(ctx)
		if err != nil {
			return err
		}
		defer os.RemoveAll(source.dir)
		if fi, err := os.Lstat(filepath.Join(real, ".git")); err == nil && fi.IsDir() {
			if err := source.snapshot(ctx, real); err != nil {
				return fmt.Errorf("snapshot local origin: %w", err)
			}
		} else {
			if err := source.snapshotGitDir(ctx, real); err != nil {
				return err
			}
		}
		remote = source.dir
	} else if !strings.Contains(remote, "://") && !scpURL.MatchString(remote) {
		return fmt.Errorf("relative origin cannot be verified")
	}
	if _, err := g.run(ctx, "fetch", "--quiet", "--no-tags", "--no-write-fetch-head", "--", remote, "+refs/heads/*:refs/verified/heads/*", "+refs/tags/*:refs/verified/tags/*", "+HEAD:refs/verified/HEAD"); err != nil {
		return fmt.Errorf("fetch origin (30s timeout): %w", err)
	}
	tips, err := g.run(ctx, "for-each-ref", "--format=%(objectname)", "refs/heads", "refs/tags")
	if err != nil {
		return err
	}
	for _, tip := range strings.Fields(tips) {
		if _, err := g.run(ctx, "rev-parse", "--verify", tip+"^{commit}"); err != nil {
			return fmt.Errorf("non-commit tag cannot be verified: %w", err)
		}
	}
	args := append([]string{"rev-list", "--max-count=1", "HEAD"}, strings.Fields(tips)...)
	args = append(args, "--not", "--glob=refs/verified/*", "--")
	missing, err := g.run(ctx, args...)
	if err != nil {
		return err
	}
	if strings.TrimSpace(missing) != "" {
		return fmt.Errorf("unpushed HEAD or local branch/tag tip")
	}
	return nil
}
