package session

import (
	"context"
	"fmt"
	"testing"
)

// Commands that die within milliseconds of spawn can leave tmux without an
// exit status until Manager nudges its reaper; every one must still resolve.
func TestInstantExitsGetStatus(t *testing.T) {
	m := newTestManager(t)
	ctx := context.Background()
	if _, err := m.Create(ctx, CreateOptions{Name: "keep", Command: []string{"sleep", "60"}}); err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 40; i++ {
		name := fmt.Sprintf("x%d", i)
		if _, err := m.Create(ctx, CreateOptions{Name: name, Command: []string{"sh", "-c", "exit 3"}}); err != nil {
			t.Fatal(err)
		}
	}
	for i := 0; i < 40; i++ {
		if s := waitExited(t, m, fmt.Sprintf("x%d", i)); s.ExitCode != 3 {
			t.Errorf("%+v", s)
		}
	}
}
