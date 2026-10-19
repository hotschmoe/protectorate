package envoy

import (
	"fmt"
	"log"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/mount"
	"github.com/docker/docker/api/types/network"
	"github.com/hotschmoe/protectorate/internal/config"
	"github.com/hotschmoe/protectorate/internal/protocol"
)


var namePool = []string{
	"quell", "virginia", "rei", "mickey", "trepp",
	"tanaka", "athena", "apollo", "hermes", "iris", "prometheus",
}

type SleeveManager struct {
	mu                sync.RWMutex
	docker            DockerContainerLifecycleClient
	cfg               *config.EnvoyConfig
	sleeves           map[string]*protocol.SleeveInfo
	usedNames         map[string]bool
	pendingWorkspaces map[string]bool
}

func NewSleeveManager(docker DockerContainerLifecycleClient, cfg *config.EnvoyConfig) *SleeveManager {
	return &SleeveManager{
		docker:            docker,
		cfg:               cfg,
		sleeves:           make(map[string]*protocol.SleeveInfo),
		usedNames:         make(map[string]bool),
		pendingWorkspaces: make(map[string]bool),
	}
}

func (m *SleeveManager) releaseName(name string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.usedNames, name)
}

// extractWorkspaceName extracts the workspace name from a full path
// e.g., /home/agent/workspaces/my-repo -> my-repo
func (m *SleeveManager) extractWorkspaceName(workspacePath string) string {
	wsRoot := m.cfg.Docker.WorkspaceRoot
	if strings.HasPrefix(workspacePath, wsRoot+"/") {
		return strings.TrimPrefix(workspacePath, wsRoot+"/")
	}
	// Fallback: use basename
	parts := strings.Split(workspacePath, "/")
	if len(parts) > 0 {
		return parts[len(parts)-1]
	}
	return workspacePath
}

// reserveWorkspaceAndName atomically checks workspace availability and reserves
// both the workspace (marking it pending) and the sleeve name. Returns the
// reserved name or an error. Caller must release pendingWorkspaces on completion.
func (m *SleeveManager) reserveWorkspaceAndName(workspace, requestedName string) (string, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if m.pendingWorkspaces[workspace] {
		return "", fmt.Errorf("workspace %q already has a spawn in progress", workspace)
	}
	for _, s := range m.sleeves {
		if s.Workspace == workspace {
			return "", fmt.Errorf("workspace %q is already in use by sleeve %q", workspace, s.Name)
		}
	}

	var name string
	if requestedName == "" {
		for _, n := range namePool {
			if !m.usedNames[n] {
				name = n
				break
			}
		}
		if name == "" {
			name = fmt.Sprintf("sleeve-%d", time.Now().UnixNano())
		}
	} else {
		if m.usedNames[requestedName] {
			return "", fmt.Errorf("sleeve name %q already in use", requestedName)
		}
		name = requestedName
	}

	m.pendingWorkspaces[workspace] = true
	m.usedNames[name] = true
	return name, nil
}

func (m *SleeveManager) Spawn(req protocol.SpawnSleeveRequest) (*protocol.SleeveInfo, error) {
	workspace := req.Workspace

	if workspace == "" {
		return nil, fmt.Errorf("workspace path required")
	}

	// Ensure workspace directory exists in the volume.
	// Docker's volume subpath feature requires the directory to exist before
	// container creation. We create it here to handle both explicit workspace
	// creation and direct spawn with a new workspace name.
	if err := os.MkdirAll(workspace, 0755); err != nil {
		return nil, fmt.Errorf("failed to create workspace directory %q: %w", workspace, err)
	}

	name, err := m.reserveWorkspaceAndName(workspace, req.Name)
	if err != nil {
		return nil, err
	}

	defer func() {
		m.mu.Lock()
		delete(m.pendingWorkspaces, workspace)
		m.mu.Unlock()
	}()

	containerName := "sleeve-" + name

	if err := m.docker.EnsureNetwork(m.cfg.Docker.Network); err != nil {
		m.releaseName(name)
		return nil, fmt.Errorf("failed to ensure network: %w", err)
	}

	constrained := req.MemoryLimitMB > 0 || req.CPULimit > 0

	labels := map[string]string{
		"protectorate.sleeve":    "true",
		"protectorate.name":      name,
		"protectorate.workspace": workspace,
	}
	if req.MemoryLimitMB > 0 {
		labels["protectorate.constrained"] = "true"
		labels["protectorate.memory_limit_mb"] = strconv.FormatInt(req.MemoryLimitMB, 10)
	}
	if req.CPULimit > 0 {
		labels["protectorate.constrained"] = "true"
		labels["protectorate.cpu_limit"] = strconv.Itoa(req.CPULimit)
	}

	cfg := &container.Config{
		Image:  m.cfg.Sleeves.Image,
		Labels: labels,
	}

	workspaceName := m.extractWorkspaceName(workspace)

	mounts := []mount.Mount{
		{
			Type:   mount.TypeVolume,
			Source: m.cfg.Docker.WorkspaceVolume,
			Target: "/home/agent/workspace",
			VolumeOptions: &mount.VolumeOptions{
				Subpath: workspaceName,
			},
		},
		{
			Type:     mount.TypeVolume,
			Source:   m.cfg.Docker.CredsVolume,
			Target:   "/home/agent/.creds",
			ReadOnly: true,
		},
	}

	hostCfg := &container.HostConfig{
		Mounts: mounts,
	}

	if req.MemoryLimitMB > 0 || req.CPULimit > 0 {
		hostCfg.Resources = container.Resources{
			Memory:   req.MemoryLimitMB * 1024 * 1024,
			NanoCPUs: int64(req.CPULimit) * 1e9,
		}
	}

	netCfg := &network.NetworkingConfig{
		EndpointsConfig: map[string]*network.EndpointSettings{
			m.cfg.Docker.Network: {},
		},
	}

	containerID, err := m.docker.CreateContainer(containerName, m.cfg.Sleeves.Image, cfg, hostCfg, netCfg)
	if err != nil {
		m.releaseName(name)
		return nil, fmt.Errorf("failed to create container: %w", err)
	}

	if err := m.docker.StartContainer(containerID); err != nil {
		m.docker.RemoveContainer(containerID)
		m.releaseName(name)
		return nil, fmt.Errorf("failed to start container: %w", err)
	}

	sleeve := &protocol.SleeveInfo{
		Name:          name,
		ContainerID:   containerID[:12],
		ContainerName: containerName,
		Workspace:     workspace,
		SpawnTime:     time.Now(),
		Status:        "running",
		Constrained:   constrained,
		MemoryLimitMB: req.MemoryLimitMB,
		CPULimit:      req.CPULimit,
	}

	m.mu.Lock()
	m.sleeves[name] = sleeve
	m.mu.Unlock()

	return sleeve, nil
}

func (m *SleeveManager) Kill(name string) error {
	m.mu.RLock()
	sleeve, ok := m.sleeves[name]
	m.mu.RUnlock()

	if !ok {
		return fmt.Errorf("sleeve %q not found", name)
	}

	containerName := "sleeve-" + name

	c, err := m.docker.GetContainerByName(containerName)
	if err != nil {
		return fmt.Errorf("failed to find container: %w", err)
	}

	if c != nil {
		if err := m.docker.StopContainer(c.ID); err != nil {
			// Continue to remove even if stop fails
		}
		if err := m.docker.RemoveContainer(c.ID); err != nil {
			return fmt.Errorf("failed to remove container: %w", err)
		}
	}

	m.mu.Lock()
	delete(m.sleeves, name)
	m.mu.Unlock()

	m.releaseName(sleeve.Name)

	return nil
}

func (m *SleeveManager) List() []*protocol.SleeveInfo {
	m.mu.RLock()
	defer m.mu.RUnlock()

	result := make([]*protocol.SleeveInfo, 0, len(m.sleeves))
	for _, s := range m.sleeves {
		result = append(result, s)
	}
	return result
}

func (m *SleeveManager) Get(name string) (*protocol.SleeveInfo, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	sleeve, ok := m.sleeves[name]
	if !ok {
		return nil, fmt.Errorf("sleeve %q not found", name)
	}
	return sleeve, nil
}

func (m *SleeveManager) RecoverSleeves() error {
	containers, err := m.docker.ListSleeveContainers()
	if err != nil {
		return fmt.Errorf("failed to list sleeve containers: %w", err)
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	recovered := 0
	for _, c := range containers {
		name := c.Labels["protectorate.name"]
		workspace := c.Labels["protectorate.workspace"]

		if name == "" {
			continue
		}

		if _, exists := m.sleeves[name]; exists {
			continue
		}

		containerName := "sleeve-" + name

		var status string
		switch c.State {
		case "running":
			status = "running"
		case "exited":
			status = "stopped"
		default:
			status = c.State
		}

		constrained := c.Labels["protectorate.constrained"] == "true"
		var memoryLimitMB int64
		var cpuLimit int
		if memStr := c.Labels["protectorate.memory_limit_mb"]; memStr != "" {
			memoryLimitMB, _ = strconv.ParseInt(memStr, 10, 64)
		}
		if cpuStr := c.Labels["protectorate.cpu_limit"]; cpuStr != "" {
			cpu, _ := strconv.Atoi(cpuStr)
			cpuLimit = cpu
		}

		sleeve := &protocol.SleeveInfo{
			Name:          name,
			ContainerID:   c.ID[:12],
			ContainerName: containerName,
			Workspace:     workspace,
			SpawnTime:     time.Unix(c.Created, 0),
			Status:        status,
			Constrained:   constrained,
			MemoryLimitMB: memoryLimitMB,
			CPULimit:      cpuLimit,
		}

		m.sleeves[name] = sleeve
		m.usedNames[name] = true
		recovered++
	}

	if recovered > 0 {
		log.Printf("recovered %d existing sleeve(s) from Docker", recovered)
	}

	return nil
}
