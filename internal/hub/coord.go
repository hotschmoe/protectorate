package hub

import (
	"context"
	"fmt"
	"log"
	"path/filepath"
	"sync"
	"time"

	"github.com/hotschmoe/protectorate/internal/directory"
	"github.com/hotschmoe/protectorate/internal/mailbox"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

const (
	coordInterval     = time.Minute
	deliverInterval   = time.Second
	defaultAckTimeout = 30 * time.Minute
)

// coordinator is the hub's deterministic coordination layer: the directory of
// projects and, built on it, the mailbox. It is hub code, not a sleeve; no
// model is involved in anything it decides.
type coordinator struct {
	h        *Hub
	dir      *directory.Directory
	mail     *mailbox.Store
	interval time.Duration
	every    time.Duration
	now      func() time.Time
	kick     chan struct{}
	wake     chan struct{}

	passMu sync.Mutex
}

// EnableCoordination turns on DirectoryService and MessageService, keeping
// their state under home. Call after SetSleeves and before Handler or Serve.
func (h *Hub) EnableCoordination(home string) error {
	if h.sleeves == nil {
		return fmt.Errorf("coordination needs sleeves")
	}
	dir, err := directory.Open(filepath.Join(home, "directory.json"))
	if err != nil {
		return err
	}
	c := &coordinator{
		h: h, dir: dir, interval: coordInterval, every: deliverInterval, now: time.Now,
		kick: make(chan struct{}, 1), wake: make(chan struct{}, 1),
	}
	if c.mail, err = mailbox.Open(filepath.Join(home, "mailbox.jsonl"), func() time.Time { return c.now() }); err != nil {
		return err
	}
	h.coord = c
	return nil
}

func (c *coordinator) ackTimeout() time.Duration {
	if t := c.h.AckTimeout; t > 0 {
		return t
	}
	return defaultAckTimeout
}

func (h *Hub) pokeCoord() {
	if h.coord != nil {
		h.coord.poke()
	}
}

// poke asks the run loop to rescan workspaces soon; requests coalesce.
func (c *coordinator) poke() {
	select {
	case c.kick <- struct{}{}:
	default:
	}
}

// nudge asks the run loop for a delivery pass soon.
func (c *coordinator) nudge() {
	select {
	case c.wake <- struct{}{}:
	default:
	}
}

// Run refreshes the directory on spawn and kill and on a slow interval, and
// delivers mail every second, until ctx is done.
func (c *coordinator) Run(ctx context.Context) {
	c.refresh(ctx)
	dirTick := time.NewTicker(c.interval)
	defer dirTick.Stop()
	passTick := time.NewTicker(c.every)
	defer passTick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-dirTick.C:
			c.refresh(ctx)
		case <-c.kick:
			c.refresh(ctx)
		case <-passTick.C:
		case <-c.wake:
		}
		c.pass(ctx)
	}
}

func (c *coordinator) close() {
	if err := c.mail.Close(); err != nil {
		log.Printf("coordination: close mailbox: %v", err)
	}
}

// refresh rescans the workspace of every running sleeve.
func (c *coordinator) refresh(ctx context.Context) {
	var scans []directory.Scan
	for _, i := range c.h.sleeves.List() {
		if i.State != sleeve.Running {
			continue
		}
		scans = append(scans, directory.Scan{Sleeve: i.Name, Workspace: directory.Inspect(ctx, i.Workspace)})
	}
	if ctx.Err() != nil {
		return
	}
	if err := c.dir.Update(scans); err != nil {
		log.Printf("coordination: update directory: %v", err)
	}
}
