package protocol

import "time"

// SleeveInfo represents a running sleeve container
type SleeveInfo struct {
	Name           string                  `json:"name"`
	ContainerID    string                  `json:"container_id"`
	ContainerName  string                  `json:"container_name"`
	Workspace      string                  `json:"workspace"`
	SpawnTime      time.Time               `json:"spawn_time"`
	Status         string                  `json:"status"`
	Constrained    bool                    `json:"constrained"`
	MemoryLimitMB  int64                   `json:"memory_limit_mb,omitempty"`
	CPULimit       int                     `json:"cpu_limit,omitempty"`
	Integrity      float64                 `json:"integrity"`
	SidecarHealthy bool                    `json:"sidecar_healthy"`
	Resources      *ContainerResourceStats `json:"resources,omitempty"`
	DHF            string                  `json:"dhf,omitempty"`
	DHFVersion     string                  `json:"dhf_version,omitempty"`
}

// ContainerResourceStats holds resource usage for a container
type ContainerResourceStats struct {
	MemoryUsedBytes  uint64  `json:"memory_used_bytes"`
	MemoryLimitBytes uint64  `json:"memory_limit_bytes"`
	MemoryPercent    float64 `json:"memory_percent"`
	CPUPercent       float64 `json:"cpu_percent"`
}

// HostStats holds system-level resource statistics
type HostStats struct {
	CPU    *CPUStats    `json:"cpu"`
	Memory *MemoryStats `json:"memory"`
	Disk   *DiskStats   `json:"disk"`
	Docker *DockerStats `json:"docker"`
}

// CPUStats holds CPU usage information
type CPUStats struct {
	UsagePercent float64 `json:"usage_percent"`
	Cores        int     `json:"cores"`
	Threads      int     `json:"threads"`
}

// MemoryStats holds memory usage information
type MemoryStats struct {
	UsedBytes  uint64  `json:"used_bytes"`
	TotalBytes uint64  `json:"total_bytes"`
	Percent    float64 `json:"percent"`
}

// DiskStats holds disk usage information
type DiskStats struct {
	UsedBytes  uint64  `json:"used_bytes"`
	TotalBytes uint64  `json:"total_bytes"`
	Percent    float64 `json:"percent"`
}

// DockerStats holds Docker container counts
type DockerStats struct {
	RunningContainers int `json:"running_containers"`
	TotalContainers   int `json:"total_containers"`
	MaxContainers     int `json:"max_containers"`
}

// SpawnSleeveRequest is the request body for spawning a new sleeve
type SpawnSleeveRequest struct {
	Workspace      string `json:"workspace"`
	Name           string `json:"name,omitempty"`
	MemoryLimitMB  int64  `json:"memory_limit_mb,omitempty"`
	CPULimit       int    `json:"cpu_limit,omitempty"`
}

// CloneWorkspaceRequest is the request body for cloning a git repo into a workspace
type CloneWorkspaceRequest struct {
	RepoURL string `json:"repo_url"`
	Name    string `json:"name,omitempty"`
}

// CloneJob represents an async clone operation
type CloneJob struct {
	ID        string    `json:"id"`
	RepoURL   string    `json:"repo_url"`
	Workspace string    `json:"workspace"`
	Status    string    `json:"status"` // pending, cloning, completed, failed
	Error     string    `json:"error,omitempty"`
	StartTime time.Time `json:"start_time"`
	EndTime   time.Time `json:"end_time,omitempty"`
}

// WorkspaceInfo represents a workspace directory
type WorkspaceInfo struct {
	Name         string            `json:"name"`
	Path         string            `json:"path"`
	InUse        bool              `json:"in_use"`
	SleeveName   string            `json:"sleeve_name,omitempty"`
	Git          *WorkspaceGitInfo `json:"git,omitempty"`
	Cstack       *CstackStats      `json:"cstack,omitempty"`
	SizeBytes    int64             `json:"size_bytes"`
	SizeWarning  bool              `json:"size_warning"`
	SizeCritical bool              `json:"size_critical"`
}

// CstackStats represents task statistics from cs stats --json
type CstackStats struct {
	Open       int  `json:"open"`
	Ready      int  `json:"ready"`
	InProgress int  `json:"in_progress"`
	Blocked    int  `json:"blocked"`
	Closed     int  `json:"closed"`
	Total      int  `json:"total"`
	Exists     bool `json:"exists"`
}

// CstackInitRequest is the request body for initializing cstack
type CstackInitRequest struct {
	Mode string `json:"mode"`
}

// CstackInitResult is the response from cstack init
type CstackInitResult struct {
	Success bool   `json:"success"`
	Message string `json:"message,omitempty"`
	Error   string `json:"error,omitempty"`
}

// WorkspaceGitInfo contains git repository information for a workspace
type WorkspaceGitInfo struct {
	Branch           string `json:"branch"`
	IsDetached       bool   `json:"is_detached,omitempty"`
	RemoteBranch     string `json:"remote_branch,omitempty"`
	IsDirty          bool   `json:"is_dirty"`
	UncommittedCount int    `json:"uncommitted_count"`
	AheadCount       int    `json:"ahead_count"`
	BehindCount      int    `json:"behind_count"`
	LastCommitHash   string `json:"last_commit_hash,omitempty"`
	LastCommitMsg    string `json:"last_commit_msg,omitempty"`
	LastCommitTime   string `json:"last_commit_time,omitempty"`
}

// BranchListResponse contains available branches for a workspace
type BranchListResponse struct {
	Current string   `json:"current"`
	Local   []string `json:"local"`
	Remote  []string `json:"remote"`
}

// SwitchBranchRequest is the request body for switching branches
type SwitchBranchRequest struct {
	Workspace string `json:"workspace"`
	Branch    string `json:"branch"`
}

// FetchResult contains the result of a git fetch operation
type FetchResult struct {
	Success bool   `json:"success"`
	Message string `json:"message,omitempty"`
}

// DoctorCheck represents a single diagnostic check result
type DoctorCheck struct {
	Name       string `json:"name"`
	Status     string `json:"status"` // "pass", "warning", "fail"
	Message    string `json:"message"`
	Suggestion string `json:"suggestion,omitempty"`
}

// AgentDoctorConfig represents the agent-doctor/config.yaml structure
type AgentDoctorConfig struct {
	Version    int      `yaml:"version" json:"version"`
	MasterPath string   `yaml:"master_path" json:"master_path"`
	Workspaces []string `yaml:"workspaces" json:"workspaces"`
}

// AgentDoctorFileStatus represents the sync status of a single file
type AgentDoctorFileStatus struct {
	Path     string `json:"path"`
	InSync   bool   `json:"in_sync"`
	Exists   bool   `json:"exists"`
	MasterAt string `json:"master_at,omitempty"`
}

// AgentDoctorWorkspaceStatus represents the sync status of a single workspace
type AgentDoctorWorkspaceStatus struct {
	Name         string                  `json:"name"`
	Path         string                  `json:"path"`
	HasClaudeMD  bool                    `json:"has_claude_md"`
	IsManaged    bool                    `json:"is_managed"`
	LastSynced   string                  `json:"last_synced,omitempty"`
	ClaudeMDSync bool                    `json:"claude_md_in_sync"`
	Agents       []AgentDoctorFileStatus `json:"agents"`
	Skills       []AgentDoctorFileStatus `json:"skills"`
}

// AgentDoctorStatus represents the overall sync status
type AgentDoctorStatus struct {
	MasterPath string                       `json:"master_path"`
	Workspaces []AgentDoctorWorkspaceStatus `json:"workspaces"`
}

// AgentDoctorSyncRequest is the request body for syncing workspaces
type AgentDoctorSyncRequest struct {
	Workspace string `json:"workspace,omitempty"`
	DryRun    bool   `json:"dry_run"`
}

// AgentDoctorSyncChange represents a single change in a sync operation
type AgentDoctorSyncChange struct {
	Action string `json:"action"` // "update", "create", "skip"
	File   string `json:"file"`
	Reason string `json:"reason,omitempty"`
}

// AgentDoctorSyncResult is the result of a sync operation
type AgentDoctorSyncResult struct {
	DryRun    bool                    `json:"dry_run"`
	Workspace string                  `json:"workspace"`
	Changes   []AgentDoctorSyncChange `json:"changes"`
	Error     string                  `json:"error,omitempty"`
}

// AgentDoctorInitRequest is the request body for initializing a workspace
type AgentDoctorInitRequest struct {
	Workspace string `json:"workspace"`
}

// AgentDoctorInitResult is the result of an init operation
type AgentDoctorInitResult struct {
	Success   bool   `json:"success"`
	Workspace string `json:"workspace"`
	Message   string `json:"message,omitempty"`
	Error     string `json:"error,omitempty"`
}

// AgentDoctorDiffEntry represents a single diff entry
type AgentDoctorDiffEntry struct {
	File   string `json:"file"`
	Status string `json:"status"` // "added", "modified", "removed"
	Diff   string `json:"diff,omitempty"`
}

// AgentDoctorDiffResult is the result of a diff operation
type AgentDoctorDiffResult struct {
	Workspace string                 `json:"workspace"`
	Entries   []AgentDoctorDiffEntry `json:"entries"`
}
