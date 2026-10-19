package envoy

import (
	"bytes"
	"strings"
	"testing"
	"time"
)

func TestWriteSSEData(t *testing.T) {
	tests := []struct {
		name string
		data string
		want string
	}{
		{
			name: "single line",
			data: "hello world",
			want: "data: hello world\n\n",
		},
		{
			name: "multi-line",
			data: "line1\nline2\nline3",
			want: "data: line1\ndata: line2\ndata: line3\n\n",
		},
		{
			name: "empty string",
			data: "",
			want: "data: \n\n",
		},
		{
			name: "HTML content",
			data: "<div class=\"test\">\n  <p>Hello</p>\n</div>",
			want: "data: <div class=\"test\">\ndata:   <p>Hello</p>\ndata: </div>\n\n",
		},
		{
			name: "JSON content",
			data: "{\"key\":\"value\",\n\"nested\":{\"a\":1}}",
			want: "data: {\"key\":\"value\",\ndata: \"nested\":{\"a\":1}}\n\n",
		},
		{
			name: "trailing newline",
			data: "line1\nline2\n",
			want: "data: line1\ndata: line2\ndata: \n\n",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var buf bytes.Buffer
			writeSSEData(&buf, tt.data)
			got := buf.String()
			if got != tt.want {
				t.Errorf("writeSSEData() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestSSEHub_ClientCount(t *testing.T) {
	hub := NewSSEHub()
	go hub.Run()

	if count := hub.ClientCount(); count != 0 {
		t.Errorf("ClientCount() = %d, want 0 for empty hub", count)
	}

	client1 := &SSEClient{send: make(chan *SSEMessage, 1)}
	client2 := &SSEClient{send: make(chan *SSEMessage, 1)}

	hub.register <- client1
	// Give the hub goroutine time to process
	waitForCount(hub, 1, 100)

	if count := hub.ClientCount(); count != 1 {
		t.Errorf("ClientCount() = %d after 1 registration, want 1", count)
	}

	hub.register <- client2
	waitForCount(hub, 2, 100)

	if count := hub.ClientCount(); count != 2 {
		t.Errorf("ClientCount() = %d after 2 registrations, want 2", count)
	}

	hub.unregister <- client1
	waitForCount(hub, 1, 100)

	if count := hub.ClientCount(); count != 1 {
		t.Errorf("ClientCount() = %d after unregister, want 1", count)
	}
}

func waitForCount(hub *SSEHub, expected int, maxAttempts int) {
	for i := 0; i < maxAttempts; i++ {
		if hub.ClientCount() == expected {
			return
		}
		time.Sleep(time.Millisecond)
	}
}

func TestSSEHub_Broadcast(t *testing.T) {
	hub := NewSSEHub()
	go hub.Run()

	client := &SSEClient{send: make(chan *SSEMessage, 10)}
	hub.register <- client

	// Wait for registration
	waitForCount(hub, 1, 100)

	hub.Broadcast("test-event", "test-data")

	// Give the broadcast goroutine time to process
	select {
	case msg := <-client.send:
		if msg.Event != "test-event" {
			t.Errorf("Event = %q, want 'test-event'", msg.Event)
		}
		if msg.Data != "test-data" {
			t.Errorf("Data = %q, want 'test-data'", msg.Data)
		}
	case <-time.After(100 * time.Millisecond):
		t.Error("expected message to be received within timeout")
	}
}

func TestHashString(t *testing.T) {
	h1 := hashString("hello")
	h2 := hashString("hello")
	h3 := hashString("world")

	if h1 != h2 {
		t.Error("same input should produce same hash")
	}
	if h1 == h3 {
		t.Error("different inputs should produce different hashes")
	}
	if len(h1) != 16 {
		t.Errorf("hash length = %d, want 16 (8 bytes hex-encoded)", len(h1))
	}
	if strings.ContainsAny(h1, "ABCDEF") {
		t.Error("hash should be lowercase hex")
	}
}
