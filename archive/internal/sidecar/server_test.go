package sidecar

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func testConfig() *Config {
	return &Config{
		Port:          8080,
		SleeveName:    "test-sleeve",
		WorkspacePath: "/tmp",
	}
}

func TestHealthEndpoint(t *testing.T) {
	server := NewServer(testConfig())

	req := httptest.NewRequest(http.MethodGet, "/health", nil)
	w := httptest.NewRecorder()

	server.handleHealth(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("expected status 200, got %d", w.Code)
	}

	var resp map[string]string
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}

	if resp["status"] != "ok" {
		t.Errorf("expected status 'ok', got %q", resp["status"])
	}
}

func TestHealthEndpointMethodNotAllowed(t *testing.T) {
	server := NewServer(testConfig())

	req := httptest.NewRequest(http.MethodPost, "/health", nil)
	w := httptest.NewRecorder()

	server.handleHealth(w, req)

	if w.Code != http.StatusMethodNotAllowed {
		t.Errorf("expected status 405, got %d", w.Code)
	}
}

func TestStatusEndpoint(t *testing.T) {
	server := NewServer(testConfig())

	req := httptest.NewRequest(http.MethodGet, "/status", nil)
	w := httptest.NewRecorder()

	server.handleStatus(w, req)

	if w.Code != http.StatusOK {
		t.Errorf("expected status 200, got %d", w.Code)
	}

	var resp StatusResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}

	if resp.SleeveName != "test-sleeve" {
		t.Errorf("expected sleeve name 'test-sleeve', got %q", resp.SleeveName)
	}

	if resp.DHF == nil {
		t.Error("expected DHF to be non-nil")
	}

	if resp.Workspace == nil {
		t.Error("expected Workspace to be non-nil")
	}

	if resp.Workspace.Path != "/tmp" {
		t.Errorf("expected workspace path '/tmp', got %q", resp.Workspace.Path)
	}

	if resp.Process == nil {
		t.Error("expected Process to be non-nil")
	}

	if resp.Process.PID == 0 {
		t.Error("expected PID to be non-zero")
	}

	if resp.Auth == nil {
		t.Error("expected Auth to be non-nil")
	}
}
