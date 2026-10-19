package envoy

import (
	"context"
	"fmt"
	"net/http"
	"time"

	"github.com/docker/docker/api/types"
	"github.com/hotschmoe/protectorate/internal/config"
	"github.com/hotschmoe/protectorate/internal/protocol"
)

// ServerDockerClient defines Docker operations needed by Server.
// Composes multiple focused interfaces for different Server responsibilities.
type ServerDockerClient interface {
	DockerListClient
	DockerContainerStatsClient
	DockerPingClient
	DockerExecClient
	GetContainerByName(name string) (*types.Container, error)
}

// ServerSleeveManager defines sleeve operations needed by Server
type ServerSleeveManager interface {
	List() []*protocol.SleeveInfo
	Get(name string) (*protocol.SleeveInfo, error)
	Spawn(req protocol.SpawnSleeveRequest) (*protocol.SleeveInfo, error)
	Kill(name string) error
	RecoverSleeves() error
}

// ServerWorkspaceManager defines workspace operations needed by Server
type ServerWorkspaceManager interface {
	List() ([]protocol.WorkspaceInfo, error)
	Create(name string) (*protocol.WorkspaceInfo, error)
	Clone(req protocol.CloneWorkspaceRequest) (*protocol.CloneJob, error)
	GetJob(id string) (*protocol.CloneJob, error)
	InitCstack(wsPath, mode string) (*protocol.CstackInitResult, error)
	ListBranches(wsPath string) (*protocol.BranchListResponse, error)
	SwitchBranch(wsPath, branch string) error
	FetchRemote(wsPath string) (*protocol.FetchResult, error)
	PullRemote(wsPath string) (*protocol.FetchResult, error)
	CommitAll(wsPath, message string) (*protocol.FetchResult, error)
	PushToRemote(wsPath string) (*protocol.FetchResult, error)
	FetchAllRemotes() *protocol.FetchResult
	SetOnCloneProgress(fn func(jobID, status string, progress int, errMsg string))
}

// ServerHostStatsCollector defines host stats operations needed by Server
type ServerHostStatsCollector interface {
	GetStats(ctx context.Context) *protocol.HostStats
	GetMemoryStats() *protocol.MemoryStats
	GetCPUStats() *protocol.CPUStats
}

// ServerAuthManager defines auth operations needed by Server
type ServerAuthManager interface {
	GetStatus() *protocol.AuthStatus
	Check() *protocol.AuthCheckResult
	SyncFromCLI(provider string) (map[string]interface{}, error)
	Login(provider protocol.AuthProvider, token string) (*protocol.AuthLoginResult, error)
	Revoke(provider protocol.AuthProvider) (*protocol.AuthRevokeResult, error)
	LoadState() error
	StartupCheck()
}

type Server struct {
	cfg         *config.EnvoyConfig
	http        *http.Server
	docker      ServerDockerClient
	sidecar     SidecarStatusClient
	sleeves     ServerSleeveManager
	workspaces  ServerWorkspaceManager
	agentDoctor *AgentDoctorManager
	hostStats   ServerHostStatsCollector
	auth        ServerAuthManager
	sseHub      *SSEHub
	broadcaster *SSEBroadcaster
	shutdownCh  chan struct{} // Channel to signal shutdown for restart
}

func NewServer(cfg *config.EnvoyConfig) (*Server, error) {
	docker, err := NewDockerClient()
	if err != nil {
		return nil, fmt.Errorf("failed to create docker client: %w", err)
	}

	sleeves := NewSleeveManager(docker, cfg)
	workspaces := NewWorkspaceManager(cfg, sleeves.List)
	agentDoctor := NewAgentDoctorManager(cfg)
	hostStats := NewHostStatsCollector(docker, 20)
	sidecar := NewSidecarClient(cfg.Docker.Network)
	auth := NewAuthManager()

	// Load auth state and perform startup check
	if err := auth.LoadState(); err != nil {
		// Log warning but don't fail startup
		fmt.Printf("Warning: failed to load auth state: %v\n", err)
	}
	auth.StartupCheck()

	if err := sleeves.RecoverSleeves(); err != nil {
		return nil, fmt.Errorf("failed to recover sleeves: %w", err)
	}

	sseHub := NewSSEHub()
	broadcaster := NewSSEBroadcaster(sseHub, sleeves, sidecar, docker, hostStats, workspaces)

	// Wire up clone progress callback
	workspaces.SetOnCloneProgress(broadcaster.BroadcastCloneProgress)

	s := &Server{
		cfg:         cfg,
		docker:      docker,
		sidecar:     sidecar,
		sleeves:     sleeves,
		workspaces:  workspaces,
		agentDoctor: agentDoctor,
		hostStats:   hostStats,
		auth:        auth,
		sseHub:      sseHub,
		broadcaster: broadcaster,
		shutdownCh:  make(chan struct{}),
	}

	mux := http.NewServeMux()
	s.registerRoutes(mux)

	s.http = &http.Server{
		Addr:         fmt.Sprintf(":%d", cfg.Server.Port),
		Handler:      mux,
		ReadTimeout:  30 * time.Second,
		WriteTimeout: 30 * time.Second,
	}

	return s, nil
}

func (s *Server) registerRoutes(mux *http.ServeMux) {
	mux.HandleFunc("/health", s.handleHealth)
	mux.HandleFunc("/api/events", s.handleSSE)
	mux.HandleFunc("/api/config", s.handleConfig)
	mux.HandleFunc("/api/config/", s.handleConfigKey)
	mux.HandleFunc("/api/auth/status", s.handleAuthStatus)
	mux.HandleFunc("/api/auth/check", s.handleAuthCheck)
	mux.HandleFunc("/api/auth/sync", s.handleAuthSync)
	mux.HandleFunc("/api/auth/", s.handleAuthProvider)
	mux.HandleFunc("/api/doctor", s.handleDoctor)
	mux.HandleFunc("/api/docker/containers", s.handleDockerContainers)
	mux.HandleFunc("/api/docker/networks", s.handleDockerNetworks)
	mux.HandleFunc("/api/workspaces", s.handleWorkspaces)
	mux.HandleFunc("/api/workspaces/clone", s.handleCloneWorkspace)
	mux.HandleFunc("/api/workspaces/branches", s.handleWorkspaceBranches)
	mux.HandleFunc("/api/workspaces/cstack", s.handleWorkspaceCstack)
	mux.HandleFunc("/api/sleeves", s.handleSleeves)
	mux.HandleFunc("/api/sleeves/", s.handleSleeveByName)
	mux.HandleFunc("/api/host/stats", s.handleHostStats)
	mux.HandleFunc("/api/host/limits", s.handleHostLimits)
	mux.HandleFunc("/sleeves/", s.handleSleeveTerminal)
	mux.HandleFunc("/envoy/terminal", s.handleEnvoyTerminal)
	mux.HandleFunc("/api/agent-doctor/status", s.handleAgentDoctorStatus)
	mux.HandleFunc("/api/agent-doctor/sync", s.handleAgentDoctorSync)
	mux.HandleFunc("/api/agent-doctor/init", s.handleAgentDoctorInit)
	mux.HandleFunc("/api/agent-doctor/diff", s.handleAgentDoctorDiff)
	mux.HandleFunc("/api/restart", s.handleRestart)
	mux.HandleFunc("/static/", s.handleStatic)
	mux.HandleFunc("/", s.handleIndex)
}

// ShutdownCh returns the shutdown channel for external signal handling.
func (s *Server) ShutdownCh() <-chan struct{} {
	return s.shutdownCh
}

func (s *Server) Start() error {
	// Start SSE hub and broadcaster
	go s.sseHub.Run()
	go s.broadcaster.Start(context.Background())

	return s.http.ListenAndServe()
}

func (s *Server) Shutdown() error {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	return s.http.Shutdown(ctx)
}
