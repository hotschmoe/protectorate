package envoy

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"strings"
	"time"

	"github.com/hotschmoe/protectorate/internal/protocol"
)

func (s *Server) handleHealth(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
}

func (s *Server) handleRestart(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]string{
		"status":  "restarting",
		"message": "Envoy will restart shortly. Docker will bring it back up.",
	})

	// Trigger shutdown after sending response
	go func() {
		time.Sleep(500 * time.Millisecond)
		close(s.shutdownCh)
	}()
}

func calculateIntegrityFromCstack(stats *protocol.CstackStats) float64 {
	if stats == nil || !stats.Exists || stats.Total == 0 {
		return 100.0
	}
	return float64(stats.Closed) / float64(stats.Total) * 100.0
}

func gitErrorStatus(err error) int {
	msg := err.Error()
	if strings.Contains(msg, "not found") {
		return http.StatusNotFound
	}
	if strings.Contains(msg, "in use") || strings.Contains(msg, "uncommitted") {
		return http.StatusConflict
	}
	if strings.Contains(msg, "not a git repository") {
		return http.StatusBadRequest
	}
	return http.StatusInternalServerError
}

func (s *Server) handleDockerContainers(w http.ResponseWriter, r *http.Request) {
	containers, err := s.docker.ListContainers()
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(containers)
}

func (s *Server) handleDockerNetworks(w http.ResponseWriter, r *http.Request) {
	networks, err := s.docker.ListNetworks()
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(networks)
}

func (s *Server) handleHostStats(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	ctx, cancel := context.WithTimeout(r.Context(), 5*time.Second)
	defer cancel()

	stats := s.hostStats.GetStats(ctx)
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(stats)
}

func (s *Server) handleHostLimits(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	memStats := s.hostStats.GetMemoryStats()
	cpuStats := s.hostStats.GetCPUStats()

	var totalMemoryMB int64
	var totalCPU int
	if memStats != nil {
		totalMemoryMB = int64(memStats.TotalBytes / (1024 * 1024))
	}
	if cpuStats != nil {
		totalCPU = cpuStats.Threads
	}

	memOptions := make([]int64, 0)
	for mb := int64(1024); mb < totalMemoryMB; mb *= 2 {
		memOptions = append(memOptions, mb)
	}

	cpuOptions := make([]int, 0)
	for cpu := 1; cpu < totalCPU; cpu *= 2 {
		cpuOptions = append(cpuOptions, cpu)
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"total_memory_mb": totalMemoryMB,
		"total_cpu":       totalCPU,
		"memory_options":  memOptions,
		"cpu_options":     cpuOptions,
	})
}

func (s *Server) handleSleeves(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		sleeves := s.sleeves.List()

		ctx, cancel := context.WithTimeout(r.Context(), 5*time.Second)
		defer cancel()

		containerNames := make([]string, 0, len(sleeves))
		for _, sleeve := range sleeves {
			containerNames = append(containerNames, sleeve.ContainerName)
		}

		sidecarStatuses := s.sidecar.BatchGetStatus(containerNames)

		for _, sleeve := range sleeves {
			sleeve.Integrity = 100.0

			if status, ok := sidecarStatuses[sleeve.ContainerName]; ok {
				sleeve.SidecarHealthy = true
				if status.DHF != nil {
					sleeve.DHF = status.DHF.Name
					sleeve.DHFVersion = status.DHF.Version
				}
				if status.Workspace != nil && status.Workspace.Cstack != nil {
					sleeve.Integrity = calculateIntegrityFromCstack(status.Workspace.Cstack)
				}
			}

			if stats, err := s.docker.GetContainerStats(ctx, sleeve.ContainerID); err == nil {
				sleeve.Resources = stats
			}
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(sleeves)

	case http.MethodPost:
		var req protocol.SpawnSleeveRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, "invalid request body", http.StatusBadRequest)
			return
		}

		sleeve, err := s.sleeves.Spawn(req)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusCreated)
		json.NewEncoder(w).Encode(sleeve)

	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (s *Server) handleSleeveByName(w http.ResponseWriter, r *http.Request) {
	path := strings.TrimPrefix(r.URL.Path, "/api/sleeves/")
	name := strings.Split(path, "/")[0]

	if name == "" {
		http.Error(w, "sleeve name required", http.StatusBadRequest)
		return
	}

	switch r.Method {
	case http.MethodGet:
		sleeve, err := s.sleeves.Get(name)
		if err != nil {
			http.Error(w, err.Error(), http.StatusNotFound)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(sleeve)

	case http.MethodDelete:
		if err := s.sleeves.Kill(name); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}

		w.WriteHeader(http.StatusNoContent)

	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (s *Server) handleSleeveTerminal(w http.ResponseWriter, r *http.Request) {
	path := strings.TrimPrefix(r.URL.Path, "/sleeves/")
	parts := strings.Split(path, "/")

	if len(parts) < 2 || parts[1] != "terminal" {
		http.Error(w, "invalid path", http.StatusBadRequest)
		return
	}

	name := parts[0]
	sleeve, err := s.sleeves.Get(name)
	if err != nil {
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}

	readOnly := r.URL.Query().Get("mode") == "observe"
	socketPath := "/home/agent/.dtach/session.sock"

	gateway := NewTerminalGateway(s.docker, sleeve.ContainerName, socketPath, readOnly)
	gateway.Start(w, r)
}

func (s *Server) handleEnvoyTerminal(w http.ResponseWriter, r *http.Request) {
	readOnly := r.URL.Query().Get("mode") == "observe"
	socketPath := "/home/agent/.dtach/session.sock"

	envoyContainer, err := s.docker.GetContainerByName("envoy-dev")
	if err != nil || envoyContainer == nil {
		envoyContainer, err = s.docker.GetContainerByName("envoy")
	}
	if err != nil || envoyContainer == nil {
		http.Error(w, "envoy container not found", http.StatusNotFound)
		return
	}

	containerName := envoyContainer.Names[0]
	if len(containerName) > 0 && containerName[0] == '/' {
		containerName = containerName[1:]
	}

	gateway := NewTerminalGateway(s.docker, containerName, socketPath, readOnly)
	gateway.Start(w, r)
}

func (s *Server) handleStatic(w http.ResponseWriter, r *http.Request) {
	// Strip "/static/" prefix to get the file path
	filePath := "web/static/" + r.URL.Path[len("/static/"):]

	// DEV_MODE: Serve from filesystem for hot-reload
	if os.Getenv("DEV_MODE") == "true" {
		devPaths := []string{
			"/app/" + filePath,                          // Mounted in container
			"./internal/envoy/" + filePath,              // Local development
		}
		for _, path := range devPaths {
			if _, err := os.Stat(path); err == nil {
				http.ServeFile(w, r, path)
				return
			}
		}
	}

	// PROD: Serve from embedded filesystem
	content, err := webFS.ReadFile(filePath)
	if err != nil {
		http.NotFound(w, r)
		return
	}

	// Set content type based on file extension
	switch {
	case strings.HasSuffix(filePath, ".css"):
		w.Header().Set("Content-Type", "text/css")
	case strings.HasSuffix(filePath, ".js"):
		w.Header().Set("Content-Type", "application/javascript")
	}

	w.Write(content)
}

func (s *Server) handleIndex(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path != "/" {
		http.NotFound(w, r)
		return
	}

	// DEV_MODE: Serve from filesystem for hot-reload (no rebuild needed)
	if os.Getenv("DEV_MODE") == "true" {
		devPaths := []string{
			"/app/web/templates/index.html",              // Mounted in container
			"./internal/envoy/web/templates/index.html",  // Local development
		}
		for _, path := range devPaths {
			if _, err := os.Stat(path); err == nil {
				http.ServeFile(w, r, path)
				return
			}
		}
	}

	// PROD: Serve from embedded filesystem
	w.Header().Set("Content-Type", "text/html")
	html, err := webFS.ReadFile("web/templates/index.html")
	if err != nil {
		http.Error(w, "template not found", http.StatusInternalServerError)
		return
	}
	w.Write(html)
}

func (s *Server) handleWorkspaces(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		workspaces, err := s.workspaces.List()
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(workspaces)

	case http.MethodPost:
		var req struct {
			Name string `json:"name"`
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, "invalid request body", http.StatusBadRequest)
			return
		}

		ws, err := s.workspaces.Create(req.Name)
		if err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusCreated)
		json.NewEncoder(w).Encode(ws)

	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (s *Server) handleCloneWorkspace(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		jobID := r.URL.Query().Get("id")
		if jobID == "" {
			http.Error(w, "job id required", http.StatusBadRequest)
			return
		}

		job, err := s.workspaces.GetJob(jobID)
		if err != nil {
			http.Error(w, err.Error(), http.StatusNotFound)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(job)

	case http.MethodPost:
		var req protocol.CloneWorkspaceRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, "invalid request body", http.StatusBadRequest)
			return
		}

		job, err := s.workspaces.Clone(req)
		if err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusAccepted)
		json.NewEncoder(w).Encode(job)

	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (s *Server) handleWorkspaceCstack(w http.ResponseWriter, r *http.Request) {
	workspace := r.URL.Query().Get("workspace")
	if workspace == "" {
		http.Error(w, "workspace parameter required", http.StatusBadRequest)
		return
	}

	switch r.Method {
	case http.MethodGet:
		stats := getCstackInfo(workspace)
		if stats == nil {
			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(map[string]bool{"exists": false})
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(stats)

	case http.MethodPost:
		action := r.URL.Query().Get("action")
		if action != "init" {
			http.Error(w, "action must be 'init'", http.StatusBadRequest)
			return
		}

		var req protocol.CstackInitRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			req.Mode = "minimal"
		}

		result, err := s.workspaces.InitCstack(workspace, req.Mode)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(result)

	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (s *Server) handleWorkspaceBranches(w http.ResponseWriter, r *http.Request) {
	workspace := r.URL.Query().Get("workspace")
	action := r.URL.Query().Get("action")

	// fetch-all doesn't require a workspace parameter
	if action == "fetch-all" && r.Method == http.MethodPost {
		result := s.workspaces.FetchAllRemotes()
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(result)
		return
	}

	if workspace == "" {
		http.Error(w, "workspace parameter required", http.StatusBadRequest)
		return
	}

	switch r.Method {
	case http.MethodGet:
		branches, err := s.workspaces.ListBranches(workspace)
		if err != nil {
			http.Error(w, err.Error(), gitErrorStatus(err))
			return
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(branches)

	case http.MethodPost:
		action := r.URL.Query().Get("action")

		switch action {
		case "switch":
			var req protocol.SwitchBranchRequest
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				http.Error(w, "invalid request body", http.StatusBadRequest)
				return
			}

			if req.Workspace == "" {
				req.Workspace = workspace
			}
			if req.Branch == "" {
				http.Error(w, "branch required", http.StatusBadRequest)
				return
			}

			if err := s.workspaces.SwitchBranch(req.Workspace, req.Branch); err != nil {
				http.Error(w, err.Error(), gitErrorStatus(err))
				return
			}

			workspaces, err := s.workspaces.List()
			if err != nil {
				http.Error(w, err.Error(), http.StatusInternalServerError)
				return
			}

			for _, ws := range workspaces {
				if ws.Path == req.Workspace {
					w.Header().Set("Content-Type", "application/json")
					json.NewEncoder(w).Encode(ws)
					return
				}
			}

			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(map[string]string{"status": "ok"})

		case "fetch":
			result, err := s.workspaces.FetchRemote(workspace)
			if err != nil {
				http.Error(w, err.Error(), gitErrorStatus(err))
				return
			}

			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(result)

		case "pull":
			result, err := s.workspaces.PullRemote(workspace)
			if err != nil {
				http.Error(w, err.Error(), gitErrorStatus(err))
				return
			}

			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(result)

		case "commit":
			result, err := s.workspaces.CommitAll(workspace, "envoy ui commit")
			if err != nil {
				http.Error(w, err.Error(), gitErrorStatus(err))
				return
			}

			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(result)

		case "push":
			result, err := s.workspaces.PushToRemote(workspace)
			if err != nil {
				http.Error(w, err.Error(), gitErrorStatus(err))
				return
			}

			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(result)

		default:
			http.Error(w, "invalid action: must be 'switch', 'fetch', 'pull', 'commit', or 'push'", http.StatusBadRequest)
		}

	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (s *Server) handleDoctor(w http.ResponseWriter, r *http.Request) {
	checks := []protocol.DoctorCheck{}

	checks = append(checks, checkSSHAgent())
	checks = append(checks, checkClaudeCredentials())
	checks = append(checks, checkGeminiCredentials())
	checks = append(checks, checkCodexCredentials())
	checks = append(checks, s.checkDocker())
	checks = append(checks, s.checkGitIdentity())
	checks = append(checks, s.checkRavenNetwork())

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(checks)
}

func checkSSHAgent() protocol.DoctorCheck {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	cmd := exec.CommandContext(ctx, "ssh-add", "-l")
	output, err := cmd.CombinedOutput()
	exitCode := 0
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			exitCode = exitErr.ExitCode()
		} else {
			return protocol.DoctorCheck{
				Name:       "SSH Agent",
				Status:     "fail",
				Message:    fmt.Sprintf("Failed to run ssh-add: %v", err),
				Suggestion: "Ensure SSH agent is running: eval $(ssh-agent) && ssh-add",
			}
		}
	}

	switch exitCode {
	case 0:
		lines := strings.Split(strings.TrimSpace(string(output)), "\n")
		keyCount := 0
		for _, line := range lines {
			if strings.TrimSpace(line) != "" {
				keyCount++
			}
		}
		return protocol.DoctorCheck{
			Name:    "SSH Agent",
			Status:  "pass",
			Message: fmt.Sprintf("SSH agent running with %d key(s) loaded", keyCount),
		}
	case 1:
		return protocol.DoctorCheck{
			Name:       "SSH Agent",
			Status:     "warning",
			Message:    "SSH agent running but no keys loaded",
			Suggestion: "Run: ssh-add ~/.ssh/id_ed25519 (or your key path)",
		}
	default:
		return protocol.DoctorCheck{
			Name:       "SSH Agent",
			Status:     "fail",
			Message:    "SSH agent not running or not accessible",
			Suggestion: "Run: eval $(ssh-agent) && ssh-add",
		}
	}
}

func checkClaudeCredentials() protocol.DoctorCheck {
	// Check for OAuth credentials (synced from host)
	oauthPath := "/home/agent/.creds/claude/.credentials.json"
	if _, err := os.Stat(oauthPath); err == nil {
		return protocol.DoctorCheck{
			Name:    "Claude Credentials",
			Status:  "pass",
			Message: "Authenticated via OAuth (synced from host)",
		}
	}

	// Check for API key credentials
	credPath := "/home/agent/.creds/claude/credentials.json"
	if _, err := os.Stat(credPath); err == nil {
		return protocol.DoctorCheck{
			Name:    "Claude Credentials",
			Status:  "pass",
			Message: "Authenticated via API key",
		}
	}

	return protocol.DoctorCheck{
		Name:       "Claude Credentials",
		Status:     "fail",
		Message:    "Credentials not found",
		Suggestion: "Add API key in Config tab, or run 'envoy auth sync' after 'claude auth login' on host",
	}
}

func checkGeminiCredentials() protocol.DoctorCheck {
	// Check for credentials in the agent-creds volume
	credPath := "/home/agent/.creds/gemini/credentials.json"
	if _, err := os.Stat(credPath); err == nil {
		return protocol.DoctorCheck{
			Name:    "Gemini Credentials",
			Status:  "pass",
			Message: "Authenticated via API key",
		}
	}

	// Check for OAuth tokens in standard locations
	oauthPaths := []string{
		"/home/agent/.config/gemini-cli/oauth_tokens.json",
		"/home/agent/.gemini/oauth_tokens.json",
	}

	for _, path := range oauthPaths {
		if _, err := os.Stat(path); err == nil {
			return protocol.DoctorCheck{
				Name:    "Gemini Credentials",
				Status:  "pass",
				Message: "Authenticated via OAuth",
			}
		}
	}

	return protocol.DoctorCheck{
		Name:       "Gemini Credentials",
		Status:     "warning",
		Message:    "Not authenticated (optional)",
		Suggestion: "Add API key in Config tab or run 'gemini' to authenticate via OAuth",
	}
}

func checkCodexCredentials() protocol.DoctorCheck {
	// Check for credentials in the agent-creds volume
	credPath := "/home/agent/.creds/codex/auth.json"
	if _, err := os.Stat(credPath); err == nil {
		return protocol.DoctorCheck{
			Name:    "Codex Credentials",
			Status:  "pass",
			Message: "Authenticated via API key",
		}
	}

	// Check for cached auth in standard locations
	authPaths := []string{
		"/home/agent/.codex/auth.json",
		"/home/agent/.config/codex/auth.json",
	}

	for _, path := range authPaths {
		if _, err := os.Stat(path); err == nil {
			return protocol.DoctorCheck{
				Name:    "Codex Credentials",
				Status:  "pass",
				Message: "Authenticated via cached auth",
			}
		}
	}

	return protocol.DoctorCheck{
		Name:       "Codex Credentials",
		Status:     "warning",
		Message:    "Not authenticated (optional)",
		Suggestion: "Add API key in Config tab or run 'codex' to authenticate",
	}
}

func (s *Server) checkDocker() protocol.DoctorCheck {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := s.docker.Ping(ctx); err != nil {
		return protocol.DoctorCheck{
			Name:       "Docker",
			Status:     "fail",
			Message:    fmt.Sprintf("Docker daemon unreachable: %v", err),
			Suggestion: "Ensure Docker is running and accessible",
		}
	}
	return protocol.DoctorCheck{
		Name:    "Docker",
		Status:  "pass",
		Message: "Docker daemon reachable",
	}
}

func (s *Server) checkGitIdentity() protocol.DoctorCheck {
	name := s.cfg.Git.Committer.Name
	email := s.cfg.Git.Committer.Email

	if name == "" || email == "" {
		missing := []string{}
		if name == "" {
			missing = append(missing, "name")
		}
		if email == "" {
			missing = append(missing, "email")
		}
		return protocol.DoctorCheck{
			Name:       "Git Identity",
			Status:     "warning",
			Message:    fmt.Sprintf("Git committer %s not configured", strings.Join(missing, " and ")),
			Suggestion: "Set git identity in Config tab or run: envoy config set git.committer.name/email",
		}
	}

	return protocol.DoctorCheck{
		Name:    "Git Identity",
		Status:  "pass",
		Message: fmt.Sprintf("Identity configured: %s <%s>", name, email),
	}
}

func (s *Server) checkRavenNetwork() protocol.DoctorCheck {
	networks, err := s.docker.ListNetworks()
	if err != nil {
		return protocol.DoctorCheck{
			Name:       "Raven Network",
			Status:     "fail",
			Message:    fmt.Sprintf("Failed to list networks: %v", err),
			Suggestion: "Check Docker connectivity",
		}
	}

	for _, n := range networks {
		if n.Name == "raven" {
			return protocol.DoctorCheck{
				Name:    "Raven Network",
				Status:  "pass",
				Message: "Network 'raven' exists",
			}
		}
	}

	return protocol.DoctorCheck{
		Name:       "Raven Network",
		Status:     "fail",
		Message:    "Network 'raven' not found",
		Suggestion: "Run: docker network create raven",
	}
}

func (s *Server) handleAgentDoctorStatus(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	workspace := r.URL.Query().Get("workspace")
	status, err := s.agentDoctor.GetStatus(workspace)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(status)
}

func (s *Server) handleAgentDoctorSync(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req protocol.AgentDoctorSyncRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		req = protocol.AgentDoctorSyncRequest{}
	}

	result, err := s.agentDoctor.Sync(req)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(result)
}

func (s *Server) handleAgentDoctorInit(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req protocol.AgentDoctorInitRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "invalid request body", http.StatusBadRequest)
		return
	}

	if req.Workspace == "" {
		http.Error(w, "workspace required", http.StatusBadRequest)
		return
	}

	result, err := s.agentDoctor.Init(req)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(result)
}

func (s *Server) handleAgentDoctorDiff(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	workspace := r.URL.Query().Get("workspace")
	if workspace == "" {
		http.Error(w, "workspace parameter required", http.StatusBadRequest)
		return
	}

	result, err := s.agentDoctor.Diff(workspace)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(result)
}
