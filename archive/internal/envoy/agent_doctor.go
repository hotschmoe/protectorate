package envoy

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/hotschmoe/protectorate/internal/config"
	"github.com/hotschmoe/protectorate/internal/protocol"
	"gopkg.in/yaml.v3"
)

const (
	beginMarker = "<!-- BEGIN PROTECTORATE COMMON -->"
	endMarker   = "<!-- END PROTECTORATE COMMON -->"
)

type AgentDoctorManager struct {
	cfg          *config.EnvoyConfig
	configPath   string
	masterPath   string
	doctorConfig *protocol.AgentDoctorConfig
}

func NewAgentDoctorManager(cfg *config.EnvoyConfig) *AgentDoctorManager {
	workspaceRoot := cfg.Docker.WorkspaceRoot
	return &AgentDoctorManager{
		cfg:        cfg,
		configPath: filepath.Join(workspaceRoot, "protectorate", "agent-doctor", "config.yaml"),
		masterPath: filepath.Join(workspaceRoot, "protectorate", "agent-doctor", "master"),
	}
}

func (m *AgentDoctorManager) LoadConfig() error {
	data, err := os.ReadFile(m.configPath)
	if err != nil {
		return fmt.Errorf("failed to read config: %w", err)
	}

	var cfg protocol.AgentDoctorConfig
	if err := yaml.Unmarshal(data, &cfg); err != nil {
		return fmt.Errorf("failed to parse config: %w", err)
	}

	m.doctorConfig = &cfg
	return nil
}

func (m *AgentDoctorManager) GetStatus(workspace string) (*protocol.AgentDoctorStatus, error) {
	if err := m.LoadConfig(); err != nil {
		return nil, err
	}

	status := &protocol.AgentDoctorStatus{
		MasterPath: m.masterPath,
		Workspaces: []protocol.AgentDoctorWorkspaceStatus{},
	}

	workspaces := m.doctorConfig.Workspaces
	if workspace != "" {
		workspaces = []string{workspace}
	}

	for _, ws := range workspaces {
		wsStatus, err := m.getWorkspaceStatus(ws)
		if err != nil {
			wsStatus = &protocol.AgentDoctorWorkspaceStatus{
				Name: ws,
				Path: filepath.Join(m.cfg.Docker.WorkspaceRoot, ws),
			}
		}
		status.Workspaces = append(status.Workspaces, *wsStatus)
	}

	return status, nil
}

func (m *AgentDoctorManager) getWorkspaceStatus(workspace string) (*protocol.AgentDoctorWorkspaceStatus, error) {
	wsPath := filepath.Join(m.cfg.Docker.WorkspaceRoot, workspace)
	claudeMDPath := filepath.Join(wsPath, "CLAUDE.md")

	status := &protocol.AgentDoctorWorkspaceStatus{
		Name:    workspace,
		Path:    wsPath,
		Agents:  []protocol.AgentDoctorFileStatus{},
		Skills:  []protocol.AgentDoctorFileStatus{},
	}

	claudeContent, err := os.ReadFile(claudeMDPath)
	if err != nil {
		status.HasClaudeMD = false
		return status, nil
	}
	status.HasClaudeMD = true

	hasMarkers, lastSynced := m.parseMarkers(string(claudeContent))
	status.IsManaged = hasMarkers
	status.LastSynced = lastSynced

	if hasMarkers {
		masterContent, err := os.ReadFile(filepath.Join(m.masterPath, "CLAUDE.md.common"))
		if err == nil {
			status.ClaudeMDSync = m.isContentInSync(string(claudeContent), string(masterContent))
		}
	}

	status.Agents = m.getFileStatuses(wsPath, ".claude/agents", "agents")
	status.Skills = m.getFileStatuses(wsPath, ".claude/skills", "skills")

	return status, nil
}

func (m *AgentDoctorManager) parseMarkers(content string) (hasMarkers bool, lastSynced string) {
	if !strings.Contains(content, beginMarker) || !strings.Contains(content, endMarker) {
		return false, ""
	}

	re := regexp.MustCompile(`<!-- Last synced: ([^>]+) -->`)
	matches := re.FindStringSubmatch(content)
	if len(matches) > 1 {
		return true, matches[1]
	}

	return true, ""
}

func (m *AgentDoctorManager) isContentInSync(wsContent, masterContent string) bool {
	beginIdx := strings.Index(wsContent, beginMarker)
	endIdx := strings.Index(wsContent, endMarker)
	if beginIdx == -1 || endIdx == -1 {
		return false
	}

	managed := wsContent[beginIdx+len(beginMarker) : endIdx]
	managed = strings.TrimSpace(managed)
	managed = regexp.MustCompile(`<!-- DO NOT EDIT - Managed by Agent Doctor -->\s*`).ReplaceAllString(managed, "")
	managed = regexp.MustCompile(`<!-- Last synced: [^>]+ -->\s*`).ReplaceAllString(managed, "")
	managed = strings.TrimSpace(managed)
	masterContent = strings.TrimSpace(masterContent)

	return managed == masterContent
}

func (m *AgentDoctorManager) getFileStatuses(wsPath, subdir, masterSubdir string) []protocol.AgentDoctorFileStatus {
	var statuses []protocol.AgentDoctorFileStatus

	masterDir := filepath.Join(m.masterPath, masterSubdir)
	files, err := os.ReadDir(masterDir)
	if err != nil {
		return statuses
	}

	for _, f := range files {
		if f.IsDir() || !strings.HasSuffix(f.Name(), ".md") {
			continue
		}

		wsFilePath := filepath.Join(wsPath, subdir, f.Name())
		masterFilePath := filepath.Join(masterDir, f.Name())

		status := protocol.AgentDoctorFileStatus{
			Path:     filepath.Join(subdir, f.Name()),
			MasterAt: masterFilePath,
		}

		wsContent, err := os.ReadFile(wsFilePath)
		if err != nil {
			status.Exists = false
			status.InSync = false
		} else {
			status.Exists = true
			masterContent, err := os.ReadFile(masterFilePath)
			if err != nil {
				status.InSync = false
			} else {
				status.InSync = string(wsContent) == string(masterContent)
			}
		}

		statuses = append(statuses, status)
	}

	return statuses
}

func (m *AgentDoctorManager) Sync(req protocol.AgentDoctorSyncRequest) (*protocol.AgentDoctorSyncResult, error) {
	if err := m.LoadConfig(); err != nil {
		return nil, err
	}

	workspaces := m.doctorConfig.Workspaces
	if req.Workspace != "" {
		workspaces = []string{req.Workspace}
	}

	var allChanges []protocol.AgentDoctorSyncChange

	for _, ws := range workspaces {
		changes, err := m.syncWorkspace(ws, req.DryRun)
		if err != nil {
			return &protocol.AgentDoctorSyncResult{
				DryRun:    req.DryRun,
				Workspace: ws,
				Error:     err.Error(),
			}, nil
		}
		allChanges = append(allChanges, changes...)
	}

	workspace := req.Workspace
	if workspace == "" {
		workspace = "all"
	}

	return &protocol.AgentDoctorSyncResult{
		DryRun:    req.DryRun,
		Workspace: workspace,
		Changes:   allChanges,
	}, nil
}

func (m *AgentDoctorManager) syncWorkspace(workspace string, dryRun bool) ([]protocol.AgentDoctorSyncChange, error) {
	var changes []protocol.AgentDoctorSyncChange
	wsPath := filepath.Join(m.cfg.Docker.WorkspaceRoot, workspace)

	claudeMDPath := filepath.Join(wsPath, "CLAUDE.md")
	claudeContent, err := os.ReadFile(claudeMDPath)
	if err != nil {
		changes = append(changes, protocol.AgentDoctorSyncChange{
			Action: "skip",
			File:   "CLAUDE.md",
			Reason: "file not found, use init first",
		})
	} else {
		hasMarkers, _ := m.parseMarkers(string(claudeContent))
		if !hasMarkers {
			changes = append(changes, protocol.AgentDoctorSyncChange{
				Action: "skip",
				File:   "CLAUDE.md",
				Reason: "no markers found, use init first",
			})
		} else {
			masterContent, err := os.ReadFile(filepath.Join(m.masterPath, "CLAUDE.md.common"))
			if err != nil {
				return nil, fmt.Errorf("failed to read master CLAUDE.md.common: %w", err)
			}

			newContent := m.injectContent(string(claudeContent), string(masterContent))
			if newContent != string(claudeContent) {
				if !dryRun {
					if err := os.WriteFile(claudeMDPath, []byte(newContent), 0644); err != nil {
						return nil, fmt.Errorf("failed to write CLAUDE.md: %w", err)
					}
				}
				changes = append(changes, protocol.AgentDoctorSyncChange{
					Action: "update",
					File:   "CLAUDE.md",
				})
			} else {
				changes = append(changes, protocol.AgentDoctorSyncChange{
					Action: "skip",
					File:   "CLAUDE.md",
					Reason: "already in sync",
				})
			}
		}
	}

	agentChanges := m.syncDirectory(wsPath, ".claude/agents", "agents", dryRun)
	skillChanges := m.syncDirectory(wsPath, ".claude/skills", "skills", dryRun)

	changes = append(changes, agentChanges...)
	changes = append(changes, skillChanges...)

	return changes, nil
}

func (m *AgentDoctorManager) syncDirectory(wsPath, subdir, masterSubdir string, dryRun bool) []protocol.AgentDoctorSyncChange {
	var changes []protocol.AgentDoctorSyncChange

	masterDir := filepath.Join(m.masterPath, masterSubdir)
	files, err := os.ReadDir(masterDir)
	if err != nil {
		return changes
	}

	targetDir := filepath.Join(wsPath, subdir)

	for _, f := range files {
		if f.IsDir() || !strings.HasSuffix(f.Name(), ".md") {
			continue
		}

		masterFilePath := filepath.Join(masterDir, f.Name())
		targetFilePath := filepath.Join(targetDir, f.Name())
		relPath := filepath.Join(subdir, f.Name())

		masterContent, err := os.ReadFile(masterFilePath)
		if err != nil {
			continue
		}

		targetContent, err := os.ReadFile(targetFilePath)
		if err != nil {
			if !dryRun {
				if err := os.MkdirAll(targetDir, 0755); err != nil {
					continue
				}
				if err := os.WriteFile(targetFilePath, masterContent, 0644); err != nil {
					continue
				}
			}
			changes = append(changes, protocol.AgentDoctorSyncChange{
				Action: "create",
				File:   relPath,
			})
			continue
		}

		if string(targetContent) != string(masterContent) {
			if !dryRun {
				if err := os.WriteFile(targetFilePath, masterContent, 0644); err != nil {
					continue
				}
			}
			changes = append(changes, protocol.AgentDoctorSyncChange{
				Action: "update",
				File:   relPath,
			})
		} else {
			changes = append(changes, protocol.AgentDoctorSyncChange{
				Action: "skip",
				File:   relPath,
				Reason: "already in sync",
			})
		}
	}

	return changes
}

func (m *AgentDoctorManager) injectContent(original, masterContent string) string {
	beginIdx := strings.Index(original, beginMarker)
	endIdx := strings.Index(original, endMarker)

	if beginIdx == -1 || endIdx == -1 {
		return original
	}

	timestamp := time.Now().UTC().Format(time.RFC3339)
	injected := fmt.Sprintf("%s\n<!-- DO NOT EDIT - Managed by Agent Doctor -->\n<!-- Last synced: %s -->\n\n%s\n\n%s",
		beginMarker, timestamp, strings.TrimSpace(masterContent), endMarker)

	return original[:beginIdx] + injected + original[endIdx+len(endMarker):]
}

func (m *AgentDoctorManager) Init(req protocol.AgentDoctorInitRequest) (*protocol.AgentDoctorInitResult, error) {
	if err := m.LoadConfig(); err != nil {
		return nil, err
	}

	wsPath := filepath.Join(m.cfg.Docker.WorkspaceRoot, req.Workspace)
	claudeMDPath := filepath.Join(wsPath, "CLAUDE.md")

	content, err := os.ReadFile(claudeMDPath)
	if err != nil {
		masterContent, err := os.ReadFile(filepath.Join(m.masterPath, "CLAUDE.md.common"))
		if err != nil {
			return &protocol.AgentDoctorInitResult{
				Success:   false,
				Workspace: req.Workspace,
				Error:     "failed to read master CLAUDE.md.common",
			}, nil
		}

		timestamp := time.Now().UTC().Format(time.RFC3339)
		newContent := fmt.Sprintf("%s\n<!-- DO NOT EDIT - Managed by Agent Doctor -->\n<!-- Last synced: %s -->\n\n%s\n\n%s\n",
			beginMarker, timestamp, strings.TrimSpace(string(masterContent)), endMarker)

		if err := os.WriteFile(claudeMDPath, []byte(newContent), 0644); err != nil {
			return &protocol.AgentDoctorInitResult{
				Success:   false,
				Workspace: req.Workspace,
				Error:     fmt.Sprintf("failed to write CLAUDE.md: %v", err),
			}, nil
		}

		return &protocol.AgentDoctorInitResult{
			Success:   true,
			Workspace: req.Workspace,
			Message:   "created new CLAUDE.md with markers",
		}, nil
	}

	hasMarkers, _ := m.parseMarkers(string(content))
	if hasMarkers {
		return &protocol.AgentDoctorInitResult{
			Success:   true,
			Workspace: req.Workspace,
			Message:   "markers already present",
		}, nil
	}

	headerEnd := strings.Index(string(content), "---")
	if headerEnd == -1 {
		headerEnd = 0
	} else {
		for i := headerEnd; i < len(content); i++ {
			if content[i] == '\n' {
				headerEnd = i + 1
				break
			}
		}
	}

	titleLine := ""
	descLine := ""
	remaining := string(content)

	lines := strings.SplitN(string(content), "\n", 10)
	for i, line := range lines {
		if strings.HasPrefix(line, "# CLAUDE.md") || strings.HasPrefix(line, "# Claude") {
			titleLine = line + "\n\n"
			if i+1 < len(lines) && strings.HasPrefix(lines[i+1], "This file") {
				descLine = lines[i+1] + "\n\n"
				remaining = strings.Join(lines[i+2:], "\n")
			} else {
				remaining = strings.Join(lines[i+1:], "\n")
			}
			break
		}
	}

	masterContent, err := os.ReadFile(filepath.Join(m.masterPath, "CLAUDE.md.common"))
	if err != nil {
		return &protocol.AgentDoctorInitResult{
			Success:   false,
			Workspace: req.Workspace,
			Error:     "failed to read master CLAUDE.md.common",
		}, nil
	}

	timestamp := time.Now().UTC().Format(time.RFC3339)
	newContent := fmt.Sprintf("%s%s%s\n<!-- DO NOT EDIT - Managed by Agent Doctor -->\n<!-- Last synced: %s -->\n\n%s\n\n%s\n\n%s",
		titleLine, descLine, beginMarker, timestamp, strings.TrimSpace(string(masterContent)), endMarker, strings.TrimSpace(remaining))

	if err := os.WriteFile(claudeMDPath, []byte(newContent), 0644); err != nil {
		return &protocol.AgentDoctorInitResult{
			Success:   false,
			Workspace: req.Workspace,
			Error:     fmt.Sprintf("failed to write CLAUDE.md: %v", err),
		}, nil
	}

	return &protocol.AgentDoctorInitResult{
		Success:   true,
		Workspace: req.Workspace,
		Message:   "added markers to existing CLAUDE.md",
	}, nil
}

func (m *AgentDoctorManager) Diff(workspace string) (*protocol.AgentDoctorDiffResult, error) {
	if err := m.LoadConfig(); err != nil {
		return nil, err
	}

	result := &protocol.AgentDoctorDiffResult{
		Workspace: workspace,
		Entries:   []protocol.AgentDoctorDiffEntry{},
	}

	wsPath := filepath.Join(m.cfg.Docker.WorkspaceRoot, workspace)

	claudeMDPath := filepath.Join(wsPath, "CLAUDE.md")
	claudeContent, err := os.ReadFile(claudeMDPath)
	if err == nil {
		hasMarkers, _ := m.parseMarkers(string(claudeContent))
		if hasMarkers {
			masterContent, err := os.ReadFile(filepath.Join(m.masterPath, "CLAUDE.md.common"))
			if err == nil && !m.isContentInSync(string(claudeContent), string(masterContent)) {
				result.Entries = append(result.Entries, protocol.AgentDoctorDiffEntry{
					File:   "CLAUDE.md",
					Status: "modified",
					Diff:   "common section differs from master",
				})
			}
		}
	}

	m.addDiffEntries(result, wsPath, ".claude/agents", "agents")
	m.addDiffEntries(result, wsPath, ".claude/skills", "skills")

	return result, nil
}

func (m *AgentDoctorManager) addDiffEntries(result *protocol.AgentDoctorDiffResult, wsPath, subdir, masterSubdir string) {
	masterDir := filepath.Join(m.masterPath, masterSubdir)
	files, err := os.ReadDir(masterDir)
	if err != nil {
		return
	}

	for _, f := range files {
		if f.IsDir() || !strings.HasSuffix(f.Name(), ".md") {
			continue
		}

		masterFilePath := filepath.Join(masterDir, f.Name())
		targetFilePath := filepath.Join(wsPath, subdir, f.Name())
		relPath := filepath.Join(subdir, f.Name())

		masterContent, err := os.ReadFile(masterFilePath)
		if err != nil {
			continue
		}

		targetContent, err := os.ReadFile(targetFilePath)
		if err != nil {
			result.Entries = append(result.Entries, protocol.AgentDoctorDiffEntry{
				File:   relPath,
				Status: "added",
				Diff:   "file missing in workspace",
			})
			continue
		}

		if string(targetContent) != string(masterContent) {
			result.Entries = append(result.Entries, protocol.AgentDoctorDiffEntry{
				File:   relPath,
				Status: "modified",
				Diff:   "content differs from master",
			})
		}
	}
}
