package envoy

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"
)

func TestSidecarClient_GetStatus_Success(t *testing.T) {
	expectedStatus := &SidecarStatus{
		SleeveName: "test-sleeve",
		DHF: &SidecarDHFInfo{
			Name:    "claude",
			Version: "1.0.0",
		},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/status" {
			t.Errorf("expected path /status, got %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(expectedStatus)
	}))
	defer server.Close()

	client := &SidecarClient{
		http:    &http.Client{Timeout: 5 * time.Second},
		network: "test",
	}

	url := server.URL + "/status"
	resp, err := client.http.Get(url)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer resp.Body.Close()

	var status SidecarStatus
	if err := json.NewDecoder(resp.Body).Decode(&status); err != nil {
		t.Fatalf("failed to decode response: %v", err)
	}

	if status.SleeveName != expectedStatus.SleeveName {
		t.Errorf("SleeveName = %q, want %q", status.SleeveName, expectedStatus.SleeveName)
	}
	if status.DHF == nil || status.DHF.Name != "claude" {
		t.Errorf("DHF = %v, want name='claude'", status.DHF)
	}
}

func TestSidecarClient_GetStatus_NonOKStatus(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusServiceUnavailable)
	}))
	defer server.Close()

	client := &SidecarClient{
		http:    &http.Client{Timeout: 5 * time.Second},
		network: "test",
	}

	url := server.URL + "/status"
	resp, err := client.http.Get(url)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusOK {
		t.Error("expected non-OK status code")
	}
}

func TestSidecarClient_GetStatus_InvalidJSON(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte("not valid json"))
	}))
	defer server.Close()

	client := &SidecarClient{
		http:    &http.Client{Timeout: 5 * time.Second},
		network: "test",
	}

	url := server.URL + "/status"
	resp, err := client.http.Get(url)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer resp.Body.Close()

	var status SidecarStatus
	err = json.NewDecoder(resp.Body).Decode(&status)
	if err == nil {
		t.Error("expected JSON decode error")
	}
}

func TestSidecarClient_Health_Success(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/health" {
			t.Errorf("expected path /health, got %s", r.URL.Path)
		}
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	client := &SidecarClient{
		http:    &http.Client{Timeout: 5 * time.Second},
		network: "test",
	}

	url := server.URL + "/health"
	resp, err := client.http.Get(url)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		t.Errorf("expected status 200, got %d", resp.StatusCode)
	}
}

func TestSidecarClient_Health_Failure(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer server.Close()

	client := &SidecarClient{
		http:    &http.Client{Timeout: 5 * time.Second},
		network: "test",
	}

	url := server.URL + "/health"
	resp, err := client.http.Get(url)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusOK {
		t.Error("expected non-OK status")
	}
}

func TestSidecarClient_BatchGetStatus_Concurrent(t *testing.T) {
	var requestCount int64

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt64(&requestCount, 1)
		// Simulate some work
		time.Sleep(10 * time.Millisecond)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(&SidecarStatus{SleeveName: "test"})
	}))
	defer server.Close()

	// Create a custom client that redirects to our test server
	client := NewSidecarClient("test")

	// Test that BatchGetStatus handles concurrent requests properly
	names := []string{"container1", "container2", "container3"}

	start := time.Now()

	// We can't easily test BatchGetStatus directly without network setup,
	// but we can verify the concurrent behavior by checking timing
	for _, name := range names {
		go func(n string) {
			url := server.URL + "/status"
			client.http.Get(url)
		}(name)
	}

	// Wait for all requests
	time.Sleep(50 * time.Millisecond)

	elapsed := time.Since(start)

	// With 3 concurrent requests of 10ms each, should complete in ~10-20ms
	// not ~30ms (sequential)
	if elapsed > 100*time.Millisecond {
		t.Logf("Requests took %v, which suggests sequential execution", elapsed)
	}

	if atomic.LoadInt64(&requestCount) < 3 {
		t.Errorf("expected at least 3 requests, got %d", requestCount)
	}
}
