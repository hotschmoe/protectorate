package envoy

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"

	"github.com/docker/docker/api/types"
	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/filters"
	"github.com/docker/docker/api/types/network"
	"github.com/docker/docker/client"
	"github.com/hotschmoe/protectorate/internal/protocol"
)

type DockerClient struct {
	cli *client.Client
}

type ContainerInfo struct {
	ID      string   `json:"id"`
	Name    string   `json:"name"`
	Image   string   `json:"image"`
	State   string   `json:"state"`
	Status  string   `json:"status"`
	Ports   []string `json:"ports"`
	Created int64    `json:"created"`
}

type NetworkInfo struct {
	ID     string `json:"id"`
	Name   string `json:"name"`
	Driver string `json:"driver"`
	Scope  string `json:"scope"`
}

func NewDockerClient() (*DockerClient, error) {
	cli, err := client.NewClientWithOpts(client.FromEnv, client.WithAPIVersionNegotiation())
	if err != nil {
		return nil, err
	}

	return &DockerClient{cli: cli}, nil
}

func (d *DockerClient) ListContainers() ([]ContainerInfo, error) {
	ctx := context.Background()

	containers, err := d.cli.ContainerList(ctx, container.ListOptions{All: true})
	if err != nil {
		return nil, err
	}

	result := make([]ContainerInfo, 0, len(containers))
	for _, c := range containers {
		name := ""
		if len(c.Names) > 0 {
			name = c.Names[0][1:]
		}

		ports := make([]string, 0, len(c.Ports))
		for _, p := range c.Ports {
			if p.PublicPort > 0 {
				ports = append(ports, formatPort(p))
			}
		}

		result = append(result, ContainerInfo{
			ID:      c.ID[:12],
			Name:    name,
			Image:   c.Image,
			State:   c.State,
			Status:  c.Status,
			Ports:   ports,
			Created: c.Created,
		})
	}

	return result, nil
}

func formatPort(p types.Port) string {
	if p.IP != "" {
		return fmt.Sprintf("%s:%d->%d/%s", p.IP, p.PublicPort, p.PrivatePort, p.Type)
	}
	return fmt.Sprintf("%d->%d/%s", p.PublicPort, p.PrivatePort, p.Type)
}

func (d *DockerClient) ListNetworks() ([]NetworkInfo, error) {
	ctx := context.Background()

	networks, err := d.cli.NetworkList(ctx, network.ListOptions{})
	if err != nil {
		return nil, err
	}

	result := make([]NetworkInfo, 0, len(networks))
	for _, n := range networks {
		result = append(result, NetworkInfo{
			ID:     n.ID[:12],
			Name:   n.Name,
			Driver: n.Driver,
			Scope:  n.Scope,
		})
	}

	return result, nil
}

func (d *DockerClient) CreateContainer(name, image string, config *container.Config, hostConfig *container.HostConfig, networkConfig *network.NetworkingConfig) (string, error) {
	ctx := context.Background()

	resp, err := d.cli.ContainerCreate(ctx, config, hostConfig, networkConfig, nil, name)
	if err != nil {
		return "", err
	}

	return resp.ID, nil
}

func (d *DockerClient) StartContainer(id string) error {
	ctx := context.Background()
	return d.cli.ContainerStart(ctx, id, container.StartOptions{})
}

func (d *DockerClient) StopContainer(id string) error {
	ctx := context.Background()
	return d.cli.ContainerStop(ctx, id, container.StopOptions{})
}

func (d *DockerClient) RemoveContainer(id string) error {
	ctx := context.Background()
	return d.cli.ContainerRemove(ctx, id, container.RemoveOptions{Force: true})
}

func (d *DockerClient) GetContainerByName(name string) (*types.Container, error) {
	ctx := context.Background()

	f := filters.NewArgs()
	f.Add("name", name)

	containers, err := d.cli.ContainerList(ctx, container.ListOptions{All: true, Filters: f})
	if err != nil {
		return nil, err
	}

	for _, c := range containers {
		for _, n := range c.Names {
			if n == "/"+name {
				return &c, nil
			}
		}
	}

	return nil, nil
}

func (d *DockerClient) EnsureNetwork(name string) error {
	ctx := context.Background()

	networks, err := d.cli.NetworkList(ctx, network.ListOptions{})
	if err != nil {
		return err
	}

	for _, n := range networks {
		if n.Name == name {
			return nil
		}
	}

	_, err = d.cli.NetworkCreate(ctx, name, network.CreateOptions{Driver: "bridge"})
	return err
}

func (d *DockerClient) ListSleeveContainers() ([]types.Container, error) {
	ctx := context.Background()

	f := filters.NewArgs()
	f.Add("label", "protectorate.sleeve=true")

	return d.cli.ContainerList(ctx, container.ListOptions{All: true, Filters: f})
}

func (d *DockerClient) InspectContainer(id string) (types.ContainerJSON, error) {
	ctx := context.Background()
	return d.cli.ContainerInspect(ctx, id)
}

// ExecAttachOptions configures a Docker exec session
type ExecAttachOptions struct {
	Container string
	Cmd       []string
	Env       []string
	User      string
	Cols      uint
	Rows      uint
}

// ExecSession represents an active exec session
type ExecSession struct {
	ID   string
	Conn types.HijackedResponse
}

// ExecAttach creates and attaches to an exec session in a container
func (d *DockerClient) ExecAttach(ctx context.Context, opts ExecAttachOptions) (*ExecSession, error) {
	execConfig := container.ExecOptions{
		Cmd:          opts.Cmd,
		Env:          opts.Env,
		User:         opts.User,
		Tty:          true,
		AttachStdin:  true,
		AttachStdout: true,
		AttachStderr: true,
	}

	resp, err := d.cli.ContainerExecCreate(ctx, opts.Container, execConfig)
	if err != nil {
		return nil, fmt.Errorf("failed to create exec: %w", err)
	}

	attachResp, err := d.cli.ContainerExecAttach(ctx, resp.ID, container.ExecStartOptions{
		Tty: true,
	})
	if err != nil {
		return nil, fmt.Errorf("failed to attach exec: %w", err)
	}

	if opts.Cols > 0 && opts.Rows > 0 {
		_ = d.cli.ContainerExecResize(ctx, resp.ID, container.ResizeOptions{
			Width:  opts.Cols,
			Height: opts.Rows,
		})
	}

	return &ExecSession{
		ID:   resp.ID,
		Conn: attachResp,
	}, nil
}

// ExecResize resizes the TTY of an exec session
func (d *DockerClient) ExecResize(ctx context.Context, execID string, cols, rows uint) error {
	return d.cli.ContainerExecResize(ctx, execID, container.ResizeOptions{
		Width:  cols,
		Height: rows,
	})
}

// ExecInspect returns information about an exec session
func (d *DockerClient) ExecInspect(ctx context.Context, execID string) (container.ExecInspect, error) {
	return d.cli.ContainerExecInspect(ctx, execID)
}

// ContainerCounts holds running and total container counts
type ContainerCounts struct {
	Running int
	Total   int
}

// GetContainerCounts returns the number of running and total containers
func (d *DockerClient) GetContainerCounts(ctx context.Context) (*ContainerCounts, error) {
	containers, err := d.cli.ContainerList(ctx, container.ListOptions{All: true})
	if err != nil {
		return nil, err
	}

	counts := &ContainerCounts{Total: len(containers)}
	for _, c := range containers {
		if c.State == "running" {
			counts.Running++
		}
	}
	return counts, nil
}

// statsCache caches container stats to avoid excessive Docker API calls
type statsCache struct {
	mu    sync.RWMutex
	data  map[string]*cachedStats
	ttl   time.Duration
}

type cachedStats struct {
	stats     *protocol.ContainerResourceStats
	timestamp time.Time
}

var containerStatsCache = &statsCache{
	data: make(map[string]*cachedStats),
	ttl:  5 * time.Second,
}

// Ping checks if the Docker daemon is reachable
func (d *DockerClient) Ping(ctx context.Context) error {
	_, err := d.cli.Ping(ctx)
	return err
}

// GetContainerStats returns resource stats for a container (cached for 5s)
func (d *DockerClient) GetContainerStats(ctx context.Context, containerID string) (*protocol.ContainerResourceStats, error) {
	containerStatsCache.mu.RLock()
	if cached, ok := containerStatsCache.data[containerID]; ok {
		if time.Since(cached.timestamp) < containerStatsCache.ttl {
			containerStatsCache.mu.RUnlock()
			return cached.stats, nil
		}
	}
	containerStatsCache.mu.RUnlock()

	resp, err := d.cli.ContainerStats(ctx, containerID, false)
	if err != nil {
		return nil, fmt.Errorf("failed to get container stats: %w", err)
	}
	defer resp.Body.Close()

	var stats types.StatsJSON
	if err := json.NewDecoder(resp.Body).Decode(&stats); err != nil {
		return nil, fmt.Errorf("failed to decode stats: %w", err)
	}

	result := &protocol.ContainerResourceStats{
		MemoryUsedBytes:  stats.MemoryStats.Usage,
		MemoryLimitBytes: stats.MemoryStats.Limit,
	}

	if result.MemoryLimitBytes > 0 {
		result.MemoryPercent = float64(result.MemoryUsedBytes) / float64(result.MemoryLimitBytes) * 100
	}

	cpuDelta := float64(stats.CPUStats.CPUUsage.TotalUsage - stats.PreCPUStats.CPUUsage.TotalUsage)
	systemDelta := float64(stats.CPUStats.SystemUsage - stats.PreCPUStats.SystemUsage)
	if systemDelta > 0 && cpuDelta > 0 {
		numCPUs := float64(stats.CPUStats.OnlineCPUs)
		if numCPUs == 0 {
			numCPUs = float64(len(stats.CPUStats.CPUUsage.PercpuUsage))
		}
		if numCPUs > 0 {
			result.CPUPercent = (cpuDelta / systemDelta) * numCPUs * 100
		}
	}

	containerStatsCache.mu.Lock()
	containerStatsCache.data[containerID] = &cachedStats{
		stats:     result,
		timestamp: time.Now(),
	}
	containerStatsCache.mu.Unlock()

	return result, nil
}
