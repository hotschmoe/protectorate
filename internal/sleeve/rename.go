package sleeve

import (
	"context"
	"errors"
	"fmt"
	"log"
	"os"
	"path/filepath"

	"github.com/hotschmoe/protectorate/internal/session"
)

// previousOrchestratorName is what the orchestrator sleeve was called before
// Envoy. Open moves a record of that name to OrchestratorName once.
const previousOrchestratorName = "poe"

// renameRecord moves the record and state directory (chat log, native
// session, env file) of the sleeve from to the name to. A running harness is
// stopped first: nothing survives a hub restart anyway, and the next chat
// message resumes the native session, which lives in the moved home. Nothing
// is deleted. It does nothing when from is unknown, and refuses, leaving
// everything as it was, when to is taken.
func (m *Manager) renameRecord(ctx context.Context, from, to string) error {
	m.mu.Lock()
	e := m.sleeves[from]
	taken := m.sleeves[to] != nil
	m.mu.Unlock()
	switch {
	case e == nil:
		return nil
	case taken:
		return fmt.Errorf("sleeve %s exists as well as %s: leaving %s as it is", to, from, from)
	}
	oldDir, newDir := m.cfg.sleeveDir(from), m.cfg.sleeveDir(to)
	if _, err := os.Lstat(newDir); err == nil {
		return fmt.Errorf("%s already exists: leaving sleeve %s as it is", newDir, from)
	}
	if e.Session != "" && m.opts.Sessions != nil {
		if err := m.opts.Sessions.Kill(ctx, e.Session); err != nil && !errors.Is(err, session.ErrNotFound) {
			return fmt.Errorf("stop %s: kill session: %w", from, err)
		}
	}
	if err := m.cfg.killScope(ctx, from); err != nil {
		return fmt.Errorf("stop %s: %w", from, err)
	}
	moved := false
	if _, err := os.Lstat(oldDir); err == nil {
		if err := os.MkdirAll(filepath.Dir(newDir), 0o700); err != nil {
			return fmt.Errorf("create %s: %w", filepath.Dir(newDir), err)
		}
		if err := os.Rename(oldDir, newDir); err != nil {
			return fmt.Errorf("move %s to %s: %w", oldDir, newDir, err)
		}
		moved = true
	}
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.sleeves, from)
	e.Name = to
	if e.Session != "" {
		e.Session = SessionName(to)
	}
	m.sleeves[to] = e
	if err := m.saveLocked(); err != nil {
		delete(m.sleeves, to)
		m.sleeves[from] = e
		e.Name = from
		if moved {
			if rerr := os.Rename(newDir, oldDir); rerr != nil {
				log.Printf("sleeve %s: move %s back: %v", from, newDir, rerr)
			}
		}
		return fmt.Errorf("persist sleeves: %w", err)
	}
	log.Printf("sleeve %s renamed to %s", from, to)
	return nil
}
