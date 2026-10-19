package envoy

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/hotschmoe/protectorate/internal/config"
	"github.com/hotschmoe/protectorate/internal/protocol"
)

type WorkspaceManager struct {
	mu             sync.RWMutex
	cfg            *config.EnvoyConfig
	jobs           map[string]*protocol.CloneJob
	sleeveGetter   func() []*protocol.SleeveInfo
	onCloneProgress func(jobID, status string, progress int, errMsg string)
}

func NewWorkspaceManager(cfg *config.EnvoyConfig, sleeveGetter func() []*protocol.SleeveInfo) *WorkspaceManager {
	wm := &WorkspaceManager{
		cfg:          cfg,
		jobs:         make(map[string]*protocol.CloneJob),
		sleeveGetter: sleeveGetter,
	}
	go wm.cleanupExpiredJobs()
	return wm
}

func (wm *WorkspaceManager) List() ([]protocol.WorkspaceInfo, error) {
	wsRoot := wm.cfg.Docker.WorkspaceRoot

	if err := os.MkdirAll(wsRoot, 0755); err != nil {
		return nil, err
	}

	entries, err := os.ReadDir(wsRoot)
	if err != nil {
		return nil, err
	}

	sleeves := wm.sleeveGetter()
	wsToSleeve := make(map[string]string)
	for _, sl := range sleeves {
		wsToSleeve[sl.Workspace] = sl.Name
	}

	workspaces := make([]protocol.WorkspaceInfo, 0)
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}

		wsPath := filepath.Join(wsRoot, entry.Name())
		sleeveName := wsToSleeve[wsPath]

		ws := protocol.WorkspaceInfo{
			Name:       entry.Name(),
			Path:       wsPath,
			InUse:      sleeveName != "",
			SleeveName: sleeveName,
		}

		if gitInfo := getGitInfo(wsPath); gitInfo != nil {
			ws.Git = gitInfo
		}

		if cstackInfo := getCstackInfo(wsPath); cstackInfo != nil {
			ws.Cstack = cstackInfo
		}

		ws.SizeBytes = getWorkspaceSize(wsPath)
		const sizeWarningThreshold = 10 * 1024 * 1024 * 1024   // 10GB
		const sizeCriticalThreshold = 20 * 1024 * 1024 * 1024  // 20GB
		ws.SizeWarning = ws.SizeBytes > sizeWarningThreshold
		ws.SizeCritical = ws.SizeBytes > sizeCriticalThreshold

		workspaces = append(workspaces, ws)
	}

	return workspaces, nil
}

func (wm *WorkspaceManager) Create(name string) (*protocol.WorkspaceInfo, error) {
	if name == "" {
		return nil, fmt.Errorf("workspace name required")
	}

	if strings.ContainsAny(name, "/\\..") {
		return nil, fmt.Errorf("invalid workspace name")
	}

	wsPath := filepath.Join(wm.cfg.Docker.WorkspaceRoot, name)

	if _, err := os.Stat(wsPath); err == nil {
		return nil, fmt.Errorf("workspace %q already exists", name)
	}

	if err := os.MkdirAll(wsPath, 0755); err != nil {
		return nil, fmt.Errorf("failed to create workspace: %w", err)
	}

	return &protocol.WorkspaceInfo{
		Name:  name,
		Path:  wsPath,
		InUse: false,
	}, nil
}

func (wm *WorkspaceManager) Clone(req protocol.CloneWorkspaceRequest) (*protocol.CloneJob, error) {
	if req.RepoURL == "" {
		return nil, fmt.Errorf("repo_url required")
	}

	// Accept HTTPS or SSH URLs
	if !strings.HasPrefix(req.RepoURL, "https://") && !strings.HasPrefix(req.RepoURL, "git@") {
		return nil, fmt.Errorf("URL must start with https:// or git@")
	}

	// Convert to SSH if configured (default)
	cloneURL := req.RepoURL
	if os.Getenv("GIT_CLONE_PROTOCOL") != "https" && strings.HasPrefix(req.RepoURL, "https://") {
		cloneURL = convertToSSHURL(req.RepoURL)
	}

	wsName := req.Name
	if wsName == "" {
		wsName = repoNameFromURL(req.RepoURL)
		if wsName == "" {
			return nil, fmt.Errorf("could not derive workspace name from repo URL")
		}
	}

	wsPath := filepath.Join(wm.cfg.Docker.WorkspaceRoot, wsName)

	if _, err := os.Stat(wsPath); err == nil {
		return nil, fmt.Errorf("workspace %q already exists", wsName)
	}

	jobID := generateJobID()
	job := &protocol.CloneJob{
		ID:        jobID,
		RepoURL:   cloneURL, // Use converted URL (SSH if configured)
		Workspace: wsPath,
		Status:    "cloning",
		StartTime: time.Now(),
	}

	wm.mu.Lock()
	wm.jobs[jobID] = job
	wm.mu.Unlock()

	go wm.runClone(job)

	return job, nil
}

func (wm *WorkspaceManager) GetJob(id string) (*protocol.CloneJob, error) {
	wm.mu.RLock()
	defer wm.mu.RUnlock()

	job, ok := wm.jobs[id]
	if !ok {
		return nil, fmt.Errorf("job %q not found", id)
	}
	return job, nil
}

func (wm *WorkspaceManager) runClone(job *protocol.CloneJob) {
	err := cloneRepo(job.RepoURL, job.Workspace)

	wm.mu.Lock()
	job.EndTime = time.Now()
	if err != nil {
		job.Status = "failed"
		job.Error = err.Error()
		os.RemoveAll(job.Workspace)
	} else {
		job.Status = "completed"
	}
	status := job.Status
	errMsg := job.Error
	wm.mu.Unlock()

	// Broadcast progress via SSE
	if wm.onCloneProgress != nil {
		progress := 100
		if status == "failed" {
			progress = 0
		}
		wm.onCloneProgress(job.ID, status, progress, errMsg)
	}
}

// SetOnCloneProgress sets the callback for clone progress updates
func (wm *WorkspaceManager) SetOnCloneProgress(fn func(jobID, status string, progress int, errMsg string)) {
	wm.onCloneProgress = fn
}

func (wm *WorkspaceManager) cleanupExpiredJobs() {
	ticker := time.NewTicker(10 * time.Minute)
	defer ticker.Stop()

	for range ticker.C {
		wm.mu.Lock()
		cutoff := time.Now().Add(-1 * time.Hour)
		for id, job := range wm.jobs {
			if job.Status == "completed" || job.Status == "failed" {
				if job.EndTime.Before(cutoff) {
					delete(wm.jobs, id)
				}
			}
		}
		wm.mu.Unlock()
	}
}

func repoNameFromURL(url string) string {
	url = strings.TrimSuffix(url, ".git")
	url = strings.TrimSuffix(url, "/")
	parts := strings.Split(url, "/")
	if len(parts) > 0 {
		return parts[len(parts)-1]
	}
	return ""
}

// convertToSSHURL converts HTTPS GitHub/GitLab URLs to SSH format
// https://github.com/user/repo.git -> git@github.com:user/repo.git
// https://gitlab.com/user/repo.git -> git@gitlab.com:user/repo.git
func convertToSSHURL(url string) string {
	url = strings.TrimSuffix(url, "/")
	if !strings.HasSuffix(url, ".git") {
		url = url + ".git"
	}

	// Handle github.com
	if strings.HasPrefix(url, "https://github.com/") {
		path := strings.TrimPrefix(url, "https://github.com/")
		return "git@github.com:" + path
	}

	// Handle gitlab.com
	if strings.HasPrefix(url, "https://gitlab.com/") {
		path := strings.TrimPrefix(url, "https://gitlab.com/")
		return "git@gitlab.com:" + path
	}

	// Handle generic https://host/path format
	if strings.HasPrefix(url, "https://") {
		remainder := strings.TrimPrefix(url, "https://")
		slashIdx := strings.Index(remainder, "/")
		if slashIdx > 0 {
			host := remainder[:slashIdx]
			path := remainder[slashIdx+1:]
			return "git@" + host + ":" + path
		}
	}

	return url
}

func cloneRepo(url, destPath string) error {
	cmd := exec.Command("git", "clone", url, destPath)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}

func generateJobID() string {
	b := make([]byte, 8)
	rand.Read(b)
	return hex.EncodeToString(b)
}

func getGitInfo(wsPath string) *protocol.WorkspaceGitInfo {
	gitDir := filepath.Join(wsPath, ".git")
	if _, err := os.Stat(gitDir); os.IsNotExist(err) {
		return nil
	}

	info := &protocol.WorkspaceGitInfo{}

	branch, isDetached := getGitBranch(wsPath)
	info.Branch = branch
	info.IsDetached = isDetached

	if !isDetached {
		info.RemoteBranch = getGitRemoteBranch(wsPath, branch)
		ahead, behind := getGitAheadBehind(wsPath, info.RemoteBranch)
		info.AheadCount = ahead
		info.BehindCount = behind
	}

	uncommitted := getGitUncommittedCount(wsPath)
	info.UncommittedCount = uncommitted
	info.IsDirty = uncommitted > 0

	hash, msg, timeAgo := getGitLastCommit(wsPath)
	info.LastCommitHash = hash
	info.LastCommitMsg = msg
	info.LastCommitTime = timeAgo

	return info
}

func getGitBranch(wsPath string) (string, bool) {
	branch, err := runGitCommand(wsPath, "rev-parse", "--abbrev-ref", "HEAD")
	if err != nil {
		return "", false
	}
	if branch == "HEAD" {
		hash, _ := runGitCommand(wsPath, "rev-parse", "--short", "HEAD")
		return hash, true
	}
	return branch, false
}

func getGitRemoteBranch(wsPath, branch string) string {
	upstream, err := runGitCommand(wsPath, "rev-parse", "--abbrev-ref", branch+"@{upstream}")
	if err != nil {
		return "origin/" + branch
	}
	return upstream
}

func getGitAheadBehind(wsPath, remoteBranch string) (int, int) {
	out, err := runGitCommand(wsPath, "rev-list", "--left-right", "--count", remoteBranch+"...HEAD")
	if err != nil {
		return 0, 0
	}

	parts := strings.Fields(out)
	if len(parts) != 2 {
		return 0, 0
	}

	behind, _ := strconv.Atoi(parts[0])
	ahead, _ := strconv.Atoi(parts[1])
	return ahead, behind
}

func getGitUncommittedCount(wsPath string) int {
	out, err := runGitCommand(wsPath, "status", "--porcelain")
	if err != nil {
		return 0
	}
	if out == "" {
		return 0
	}
	return len(strings.Split(strings.TrimSpace(out), "\n"))
}

func getGitLastCommit(wsPath string) (hash, msg, timeAgo string) {
	out, err := runGitCommand(wsPath, "log", "-1", "--format=%h|%s|%cr")
	if err != nil {
		return "", "", ""
	}
	parts := strings.SplitN(out, "|", 3)
	if len(parts) != 3 {
		return "", "", ""
	}
	return parts[0], parts[1], parts[2]
}

func runGitCommand(wsPath string, args ...string) (string, error) {
	// Use -c safe.directory to handle mounted volumes with different ownership
	fullArgs := append([]string{"-c", "safe.directory=" + wsPath, "-C", wsPath}, args...)
	cmd := exec.Command("git", fullArgs...)
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func getWorkspaceSize(wsPath string) int64 {
	cmd := exec.Command("du", "-sb", wsPath)
	out, err := cmd.Output()
	if err != nil {
		return 0
	}
	parts := strings.Fields(string(out))
	if len(parts) == 0 {
		return 0
	}
	size, _ := strconv.ParseInt(parts[0], 10, 64)
	return size
}

func getCstackInfo(wsPath string) *protocol.CstackStats {
	cstackDir := filepath.Join(wsPath, ".cstack")
	if _, err := os.Stat(cstackDir); os.IsNotExist(err) {
		return nil
	}

	cmd := exec.Command("cs", "stats", "--json")
	cmd.Dir = wsPath
	out, err := cmd.Output()
	if err != nil {
		return &protocol.CstackStats{Exists: true}
	}

	var stats protocol.CstackStats
	if err := json.Unmarshal(out, &stats); err != nil {
		return &protocol.CstackStats{Exists: true}
	}

	stats.Exists = true
	return &stats
}

// InitCstack initializes cstack in a workspace
func (wm *WorkspaceManager) InitCstack(wsPath, mode string) (*protocol.CstackInitResult, error) {
	if _, err := os.Stat(wsPath); os.IsNotExist(err) {
		return nil, fmt.Errorf("workspace not found: %s", wsPath)
	}

	cstackDir := filepath.Join(wsPath, ".cstack")
	if _, err := os.Stat(cstackDir); err == nil {
		return &protocol.CstackInitResult{
			Success: false,
			Error:   "cstack already initialized",
		}, nil
	}

	cmd := exec.Command("cs", "init")
	cmd.Dir = wsPath
	out, err := cmd.CombinedOutput()
	if err != nil {
		return &protocol.CstackInitResult{
			Success: false,
			Error:   string(out),
		}, nil
	}

	if mode == "interview" {
		marker := filepath.Join(cstackDir, "INTERVIEW_PENDING.md")
		content := `# Cstack Interview Pending

This workspace needs project context setup. When a sleeve spawns,
Claude should run the cstack interview to gather project context.

See: /interview command or ask Claude to interview about the project.
`
		os.WriteFile(marker, []byte(content), 0644)
	}

	return &protocol.CstackInitResult{
		Success: true,
		Message: "cstack initialized",
	}, nil
}

// IsWorkspaceInUse checks if workspace is mounted to a running sleeve
func (wm *WorkspaceManager) IsWorkspaceInUse(wsPath string) (bool, string) {
	sleeves := wm.sleeveGetter()
	for _, sl := range sleeves {
		if sl.Workspace == wsPath {
			return true, sl.Name
		}
	}
	return false, ""
}

// ListBranches returns local and remote branches for a workspace
func (wm *WorkspaceManager) ListBranches(wsPath string) (*protocol.BranchListResponse, error) {
	gitDir := filepath.Join(wsPath, ".git")
	if _, err := os.Stat(gitDir); os.IsNotExist(err) {
		return nil, fmt.Errorf("workspace is not a git repository")
	}

	resp := &protocol.BranchListResponse{
		Local:  []string{},
		Remote: []string{},
	}

	// Get current branch
	branch, _ := getGitBranch(wsPath)
	resp.Current = branch

	// Get local branches
	localOut, err := runGitCommand(wsPath, "branch", "--list", "--format=%(refname:short)")
	if err == nil && localOut != "" {
		for _, b := range strings.Split(localOut, "\n") {
			b = strings.TrimSpace(b)
			if b != "" {
				resp.Local = append(resp.Local, b)
			}
		}
	}

	// Get remote branches
	remoteOut, err := runGitCommand(wsPath, "branch", "-r", "--format=%(refname:short)")
	if err == nil && remoteOut != "" {
		for _, b := range strings.Split(remoteOut, "\n") {
			b = strings.TrimSpace(b)
			if b != "" && !strings.HasSuffix(b, "/HEAD") {
				resp.Remote = append(resp.Remote, b)
			}
		}
	}

	return resp, nil
}

// SwitchBranch changes the current branch (validates not in use, clean tree)
func (wm *WorkspaceManager) SwitchBranch(wsPath, branch string) error {
	// Check workspace exists
	if _, err := os.Stat(wsPath); os.IsNotExist(err) {
		return fmt.Errorf("workspace not found")
	}

	// Check it's a git repository
	gitDir := filepath.Join(wsPath, ".git")
	if _, err := os.Stat(gitDir); os.IsNotExist(err) {
		return fmt.Errorf("workspace is not a git repository")
	}

	// Check not in use
	if inUse, sleeveName := wm.IsWorkspaceInUse(wsPath); inUse {
		return fmt.Errorf("workspace in use by sleeve: %s", sleeveName)
	}

	// Check clean working tree
	uncommitted := getGitUncommittedCount(wsPath)
	if uncommitted > 0 {
		return fmt.Errorf("workspace has uncommitted changes")
	}

	// For remote branches (origin/xxx), check out tracking branch
	checkoutTarget := branch
	if strings.HasPrefix(branch, "origin/") {
		localBranch := strings.TrimPrefix(branch, "origin/")
		// Try to checkout the local branch if it exists, or create tracking branch
		_, err := runGitCommand(wsPath, "checkout", localBranch)
		if err != nil {
			// Create tracking branch
			_, err = runGitCommand(wsPath, "checkout", "-b", localBranch, "--track", branch)
			if err != nil {
				return fmt.Errorf("git error: failed to checkout branch %s", branch)
			}
		}
		return nil
	}

	// Execute checkout for local branches
	_, err := runGitCommand(wsPath, "checkout", checkoutTarget)
	if err != nil {
		return fmt.Errorf("git error: failed to checkout branch %s", branch)
	}

	return nil
}

// FetchRemote fetches from origin
func (wm *WorkspaceManager) FetchRemote(wsPath string) (*protocol.FetchResult, error) {
	// Check workspace exists
	if _, err := os.Stat(wsPath); os.IsNotExist(err) {
		return nil, fmt.Errorf("workspace not found")
	}

	// Check it's a git repository
	gitDir := filepath.Join(wsPath, ".git")
	if _, err := os.Stat(gitDir); os.IsNotExist(err) {
		return nil, fmt.Errorf("workspace is not a git repository")
	}

	// Run git fetch origin
	_, err := runGitCommand(wsPath, "fetch", "origin")
	if err != nil {
		return &protocol.FetchResult{
			Success: false,
			Message: "git fetch failed",
		}, nil
	}

	return &protocol.FetchResult{
		Success: true,
		Message: "Fetched from origin",
	}, nil
}

// PullRemote pulls from origin (fast-forward only)
func (wm *WorkspaceManager) PullRemote(wsPath string) (*protocol.FetchResult, error) {
	// Check workspace exists
	if _, err := os.Stat(wsPath); os.IsNotExist(err) {
		return nil, fmt.Errorf("workspace not found")
	}

	// Check it's a git repository
	gitDir := filepath.Join(wsPath, ".git")
	if _, err := os.Stat(gitDir); os.IsNotExist(err) {
		return nil, fmt.Errorf("workspace is not a git repository")
	}

	// Check not in use
	if inUse, sleeveName := wm.IsWorkspaceInUse(wsPath); inUse {
		return nil, fmt.Errorf("workspace in use by sleeve: %s", sleeveName)
	}

	// Check clean working tree
	uncommitted := getGitUncommittedCount(wsPath)
	if uncommitted > 0 {
		return nil, fmt.Errorf("workspace has uncommitted changes")
	}

	// Run git pull --ff-only
	_, err := runGitCommand(wsPath, "pull", "--ff-only")
	if err != nil {
		return &protocol.FetchResult{
			Success: false,
			Message: "pull failed: not a fast-forward",
		}, nil
	}

	return &protocol.FetchResult{
		Success: true,
		Message: "Pulled from origin",
	}, nil
}

// CommitAll stages and commits all changes with a simple message
func (wm *WorkspaceManager) CommitAll(wsPath, message string) (*protocol.FetchResult, error) {
	if _, err := os.Stat(wsPath); os.IsNotExist(err) {
		return nil, fmt.Errorf("workspace not found")
	}

	gitDir := filepath.Join(wsPath, ".git")
	if _, err := os.Stat(gitDir); os.IsNotExist(err) {
		return nil, fmt.Errorf("workspace is not a git repository")
	}

	if inUse, sleeveName := wm.IsWorkspaceInUse(wsPath); inUse {
		return nil, fmt.Errorf("workspace in use by sleeve: %s", sleeveName)
	}

	uncommitted := getGitUncommittedCount(wsPath)
	if uncommitted == 0 {
		return &protocol.FetchResult{
			Success: false,
			Message: "no changes to commit",
		}, nil
	}

	// Stage all changes
	_, err := runGitCommand(wsPath, "add", "-A")
	if err != nil {
		return &protocol.FetchResult{
			Success: false,
			Message: "failed to stage changes",
		}, nil
	}

	// Get git identity - check env vars, then try to read from host gitconfig
	gitName := os.Getenv("GIT_COMMITTER_NAME")
	gitEmail := os.Getenv("GIT_COMMITTER_EMAIL")

	// Try to read from mounted gitconfig if env vars not set
	if gitName == "" || gitEmail == "" {
		if name, _ := exec.Command("git", "config", "--global", "user.name").Output(); len(name) > 0 {
			gitName = strings.TrimSpace(string(name))
		}
		if email, _ := exec.Command("git", "config", "--global", "user.email").Output(); len(email) > 0 {
			gitEmail = strings.TrimSpace(string(email))
		}
	}

	// Fall back to defaults if still empty
	if gitName == "" {
		gitName = "Protectorate Envoy"
	}
	if gitEmail == "" {
		gitEmail = "envoy@protectorate.local"
	}

	// Commit using env vars (more reliable than -c flags)
	cmd := exec.Command("git", "-c", "safe.directory="+wsPath, "-C", wsPath, "commit", "-m", message)
	cmd.Env = append(os.Environ(),
		"GIT_AUTHOR_NAME="+gitName,
		"GIT_AUTHOR_EMAIL="+gitEmail,
		"GIT_COMMITTER_NAME="+gitName,
		"GIT_COMMITTER_EMAIL="+gitEmail,
	)
	if out, err := cmd.CombinedOutput(); err != nil {
		return &protocol.FetchResult{
			Success: false,
			Message: "commit failed: " + string(out),
		}, nil
	}

	return &protocol.FetchResult{
		Success: true,
		Message: "committed changes",
	}, nil
}

// PushToRemote pushes commits to origin
func (wm *WorkspaceManager) PushToRemote(wsPath string) (*protocol.FetchResult, error) {
	if _, err := os.Stat(wsPath); os.IsNotExist(err) {
		return nil, fmt.Errorf("workspace not found")
	}

	gitDir := filepath.Join(wsPath, ".git")
	if _, err := os.Stat(gitDir); os.IsNotExist(err) {
		return nil, fmt.Errorf("workspace is not a git repository")
	}

	if inUse, sleeveName := wm.IsWorkspaceInUse(wsPath); inUse {
		return nil, fmt.Errorf("workspace in use by sleeve: %s", sleeveName)
	}

	// Check if there are commits to push
	info := getGitInfo(wsPath)
	if info == nil || info.AheadCount == 0 {
		return &protocol.FetchResult{
			Success: false,
			Message: "no commits to push",
		}, nil
	}

	cmd := exec.Command("git", "-c", "safe.directory="+wsPath, "-C", wsPath, "push")
	out, err := cmd.CombinedOutput()
	if err != nil {
		msg := string(out)
		// Provide helpful error for auth issues
		if strings.Contains(msg, "could not read Username") || strings.Contains(msg, "Authentication failed") {
			return &protocol.FetchResult{
				Success: false,
				Message: "push failed: authentication required. Use SSH URL (git remote set-url origin git@github.com:user/repo.git) or configure git-credentials",
			}, nil
		}
		return &protocol.FetchResult{
			Success: false,
			Message: "push failed: " + msg,
		}, nil
	}

	return &protocol.FetchResult{
		Success: true,
		Message: "pushed to origin",
	}, nil
}

// FetchAllRemotes fetches from origin for all git workspaces (parallel, with timeout)
func (wm *WorkspaceManager) FetchAllRemotes() *protocol.FetchResult {
	workspaces, err := wm.List()
	if err != nil {
		return &protocol.FetchResult{Success: false, Message: "failed to list workspaces"}
	}

	var gitWorkspaces []string
	for _, ws := range workspaces {
		if ws.Git != nil {
			gitWorkspaces = append(gitWorkspaces, ws.Path)
		}
	}

	if len(gitWorkspaces) == 0 {
		return &protocol.FetchResult{Success: true, Message: "no git workspaces to fetch"}
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	var wg sync.WaitGroup
	for _, wsPath := range gitWorkspaces {
		wg.Add(1)
		go func(path string) {
			defer wg.Done()
			select {
			case <-ctx.Done():
				return
			default:
				runGitCommand(path, "fetch", "origin")
			}
		}(wsPath)
	}

	done := make(chan struct{})
	go func() {
		wg.Wait()
		close(done)
	}()

	select {
	case <-done:
		return &protocol.FetchResult{
			Success: true,
			Message: fmt.Sprintf("fetched %d workspaces", len(gitWorkspaces)),
		}
	case <-ctx.Done():
		return &protocol.FetchResult{
			Success: true,
			Message: fmt.Sprintf("fetched workspaces (some timed out)"),
		}
	}
}
