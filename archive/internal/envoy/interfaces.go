package envoy

import (
	"context"

	"github.com/docker/docker/api/types"
	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/network"
	"github.com/hotschmoe/protectorate/internal/protocol"
)

// DockerExecClient defines Docker exec session operations.
// Used by TerminalGateway and Server for terminal access.
type DockerExecClient interface {
	ExecAttach(ctx context.Context, opts ExecAttachOptions) (*ExecSession, error)
	ExecResize(ctx context.Context, execID string, cols, rows uint) error
}

// DockerContainerStatsClient defines Docker container stats operations.
// Used by SSEBroadcaster for resource monitoring.
type DockerContainerStatsClient interface {
	GetContainerStats(ctx context.Context, containerID string) (*protocol.ContainerResourceStats, error)
}

// DockerContainerCountsClient defines Docker container counting operations.
// Used by HostStatsCollector for dashboard metrics.
type DockerContainerCountsClient interface {
	GetContainerCounts(ctx context.Context) (*ContainerCounts, error)
}

// DockerContainerLifecycleClient defines Docker container lifecycle operations.
// Used by SleeveManager for spawning and killing sleeves.
type DockerContainerLifecycleClient interface {
	EnsureNetwork(name string) error
	CreateContainer(name, image string, config *container.Config, hostConfig *container.HostConfig, networkConfig *network.NetworkingConfig) (string, error)
	StartContainer(id string) error
	StopContainer(id string) error
	RemoveContainer(id string) error
	GetContainerByName(name string) (*types.Container, error)
	ListSleeveContainers() ([]types.Container, error)
}

// DockerListClient defines Docker listing operations.
// Used by Server for dashboard views.
type DockerListClient interface {
	ListContainers() ([]ContainerInfo, error)
	ListNetworks() ([]NetworkInfo, error)
}

// DockerPingClient defines Docker health check operations.
// Used by Server for health endpoints.
type DockerPingClient interface {
	Ping(ctx context.Context) error
}

// SidecarStatusClient defines sidecar status fetching operations.
// Used by Server and SSEBroadcaster for sleeve status enrichment.
type SidecarStatusClient interface {
	BatchGetStatus(containerNames []string) map[string]*SidecarStatus
}

// SleeveListProvider defines sleeve listing operations.
// Used by SSEBroadcaster and WorkspaceManager.
type SleeveListProvider interface {
	List() []*protocol.SleeveInfo
}

// HostStatsProvider defines host statistics operations.
// Used by SSEBroadcaster for dashboard metrics.
type HostStatsProvider interface {
	GetStats(ctx context.Context) *protocol.HostStats
}