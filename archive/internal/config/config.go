package config

import (
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"time"

	"gopkg.in/yaml.v3"
)

const DefaultConfigPath = "/home/agent/.config/envoy.yaml"

// Defaults holds all default configuration values in one place.
// Used by both config loading and config reset operations.
var Defaults = struct {
	ServerPort        int
	SleevesMax        int
	SleevesPollInt    string
	SleevesIdleThresh string
	SleevesImage      string
	DockerNetwork     string
	DockerWorkspaceRoot string
	GitCloneProtocol  string
}{
	ServerPort:        7470,
	SleevesMax:        10,
	SleevesPollInt:    "1h",
	SleevesIdleThresh: "0s",
	SleevesImage:      "ghcr.io/hotschmoe/protectorate-sleeve:latest",
	DockerNetwork:     "raven",
	DockerWorkspaceRoot: "/home/agent/workspaces",
	GitCloneProtocol:  "ssh",
}

// EnvoyConfig defines the configuration for the Envoy manager service.
type EnvoyConfig struct {
	mu         sync.RWMutex `yaml:"-"`
	configPath string       `yaml:"-"`

	Server  ServerConfig  `yaml:"server"`
	Sleeves SleevesConfig `yaml:"sleeves"`
	Docker  DockerConfig  `yaml:"docker"`
	Git     GitConfig     `yaml:"git"`
	Gitea   GiteaConfig   `yaml:"gitea,omitempty"`
	Mirror  MirrorConfig  `yaml:"mirror,omitempty"`
}

// ServerConfig defines HTTP server settings.
type ServerConfig struct {
	Port int `yaml:"port"`
}

// SleevesConfig defines sleeve management settings.
type SleevesConfig struct {
	Max           int    `yaml:"max"`
	PollInterval  string `yaml:"poll_interval"`
	IdleThreshold string `yaml:"idle_threshold"`
	Image         string `yaml:"image"`
}

// DockerConfig defines Docker-specific configuration.
type DockerConfig struct {
	Network         string `yaml:"network"`
	WorkspaceRoot   string `yaml:"workspace_root"`
	WorkspaceVolume string `yaml:"-"` // Internal only, not persisted
	CredsVolume     string `yaml:"-"` // Internal only, not persisted
}

// GitConfig defines git-related settings.
type GitConfig struct {
	CloneProtocol string          `yaml:"clone_protocol"`
	Committer     CommitterConfig `yaml:"committer"`
}

// CommitterConfig defines git committer identity.
type CommitterConfig struct {
	Name  string `yaml:"name"`
	Email string `yaml:"email"`
}

// GiteaConfig defines Gitea configuration.
type GiteaConfig struct {
	Enabled  bool   `yaml:"enabled"`
	URL      string `yaml:"url,omitempty"`
	User     string `yaml:"user,omitempty"`
	Password string `yaml:"-"` // Never persist passwords
	Token    string `yaml:"-"` // Never persist tokens in YAML
}

// MirrorConfig defines GitHub mirror configuration.
type MirrorConfig struct {
	Enabled   bool   `yaml:"enabled"`
	Frequency string `yaml:"frequency,omitempty"`
	GitHubOrg string `yaml:"github_org,omitempty"`
	Token     string `yaml:"-"` // Never persist tokens in YAML
}

// getEnv returns the environment variable value or a default.
func getEnv(key, defaultVal string) string {
	if val := os.Getenv(key); val != "" {
		return val
	}
	return defaultVal
}

// getEnvInt returns the environment variable as int or a default.
func getEnvInt(key string, defaultVal int) int {
	if val := os.Getenv(key); val != "" {
		if i, err := strconv.Atoi(val); err == nil {
			return i
		}
	}
	return defaultVal
}

// getEnvBool returns the environment variable as bool or a default.
func getEnvBool(key string, defaultVal bool) bool {
	if val := os.Getenv(key); val != "" {
		if b, err := strconv.ParseBool(val); err == nil {
			return b
		}
	}
	return defaultVal
}

// parseDuration parses a duration string, returning the default if empty or invalid.
func parseDuration(s string, defaultVal time.Duration) time.Duration {
	if s == "" {
		return defaultVal
	}
	d, err := time.ParseDuration(s)
	if err != nil {
		return defaultVal
	}
	return d
}

// LoadEnvoyConfig loads configuration with precedence: env vars > YAML file > defaults.
// The YAML file is stored in the agent-config volume at /home/agent/.config/envoy.yaml.
func LoadEnvoyConfig() *EnvoyConfig {
	cfg := &EnvoyConfig{configPath: DefaultConfigPath}
	cfg.applyDefaults()
	cfg.loadFromYAML()
	cfg.applyEnvOverrides()
	return cfg
}

// applyDefaults sets all configuration to default values.
func (c *EnvoyConfig) applyDefaults() {
	c.Server = ServerConfig{
		Port: Defaults.ServerPort,
	}
	c.Sleeves = SleevesConfig{
		Max:           Defaults.SleevesMax,
		PollInterval:  Defaults.SleevesPollInt,
		IdleThreshold: Defaults.SleevesIdleThresh,
		Image:         Defaults.SleevesImage,
	}
	c.Docker = DockerConfig{
		Network:         Defaults.DockerNetwork,
		WorkspaceRoot:   Defaults.DockerWorkspaceRoot,
		WorkspaceVolume: "agent-workspaces",
		CredsVolume:     "agent-creds",
	}
	c.Git = GitConfig{
		CloneProtocol: Defaults.GitCloneProtocol,
		Committer: CommitterConfig{
			Name:  "",
			Email: "",
		},
	}
	c.Gitea = GiteaConfig{
		Enabled: false,
		URL:     "http://gitea:3000",
	}
	c.Mirror = MirrorConfig{
		Enabled:   false,
		Frequency: "daily",
	}
}

// loadFromYAML reads configuration from the YAML file if it exists.
func (c *EnvoyConfig) loadFromYAML() {
	data, err := os.ReadFile(c.configPath)
	if err != nil {
		return // File doesn't exist or unreadable - use defaults
	}
	if err := yaml.Unmarshal(data, c); err != nil {
		log.Printf("Warning: failed to parse config file %s: %v (using defaults)", c.configPath, err)
		return
	}
}

// applyEnvOverrides applies environment variable overrides (highest precedence).
func (c *EnvoyConfig) applyEnvOverrides() {
	if val := os.Getenv("ENVOY_PORT"); val != "" {
		c.Server.Port = getEnvInt("ENVOY_PORT", c.Server.Port)
	}
	if val := os.Getenv("ENVOY_MAX_SLEEVES"); val != "" {
		c.Sleeves.Max = getEnvInt("ENVOY_MAX_SLEEVES", c.Sleeves.Max)
	}
	if val := os.Getenv("ENVOY_POLL_INTERVAL"); val != "" {
		c.Sleeves.PollInterval = val
	}
	if val := os.Getenv("ENVOY_IDLE_THRESHOLD"); val != "" {
		c.Sleeves.IdleThreshold = val
	}
	if val := os.Getenv("SLEEVE_IMAGE"); val != "" {
		c.Sleeves.Image = val
	}
	if val := os.Getenv("DOCKER_NETWORK"); val != "" {
		c.Docker.Network = val
	}
	if val := os.Getenv("WORKSPACE_ROOT"); val != "" {
		c.Docker.WorkspaceRoot = val
	}
	if val := os.Getenv("WORKSPACE_VOLUME"); val != "" {
		c.Docker.WorkspaceVolume = val
	}
	if val := os.Getenv("CREDS_VOLUME"); val != "" {
		c.Docker.CredsVolume = val
	}
	if val := os.Getenv("GIT_CLONE_PROTOCOL"); val != "" {
		c.Git.CloneProtocol = val
	}
	// Gitea/Mirror env overrides
	if val := os.Getenv("GITEA_URL"); val != "" {
		c.Gitea.URL = val
	}
	if val := os.Getenv("GITEA_USER"); val != "" {
		c.Gitea.User = val
	}
	if val := os.Getenv("GITEA_PASSWORD"); val != "" {
		c.Gitea.Password = val
	}
	if val := os.Getenv("GITEA_TOKEN"); val != "" {
		c.Gitea.Token = val
	}
	c.Gitea.Enabled = getEnvBool("GITEA_ENABLED", c.Gitea.Enabled)
	c.Mirror.Enabled = getEnvBool("MIRROR_ENABLED", c.Mirror.Enabled)
	if val := os.Getenv("MIRROR_FREQUENCY"); val != "" {
		c.Mirror.Frequency = val
	}
	if val := os.Getenv("MIRROR_GITHUB_ORG"); val != "" {
		c.Mirror.GitHubOrg = val
	}
	if val := os.Getenv("MIRROR_GITHUB_TOKEN"); val != "" {
		c.Mirror.Token = val
	}
}

// Save persists the current configuration to the YAML file.
func (c *EnvoyConfig) Save() error {
	c.mu.Lock()
	defer c.mu.Unlock()

	// Ensure directory exists
	dir := filepath.Dir(c.configPath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create config directory: %w", err)
	}

	data, err := yaml.Marshal(c)
	if err != nil {
		return fmt.Errorf("failed to marshal config: %w", err)
	}

	// Write with header comment
	header := "# Protectorate Envoy Configuration\n# Modify via WebUI or CLI: envoy config set <key> <value>\n# Changes require envoy restart to take effect.\n\n"
	content := []byte(header + string(data))

	if err := os.WriteFile(c.configPath, content, 0644); err != nil {
		return fmt.Errorf("failed to write config: %w", err)
	}

	return nil
}

// GetPollInterval returns the poll interval as a duration.
func (c *EnvoyConfig) GetPollInterval() time.Duration {
	return parseDuration(c.Sleeves.PollInterval, time.Hour)
}

// GetIdleThreshold returns the idle threshold as a duration.
func (c *EnvoyConfig) GetIdleThreshold() time.Duration {
	return parseDuration(c.Sleeves.IdleThreshold, 0)
}
