package hub

import (
	"context"
	"fmt"
	"log"
	"path/filepath"
	"time"

	"github.com/hotschmoe/protectorate/internal/directory"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

const coordInterval = time.Minute

// coordinator is the hub's deterministic coordination layer: the directory of
// projects and, built on it, the mailbox. It is hub code, not a sleeve; no
// model is involved in anything it decides.
type coordinator struct {
	h        *Hub
	dir      *directory.Directory
	interval time.Duration
	kick     chan struct{}
}

// EnableCoordination turns on DirectoryService, keeping its state under home.
// Call after SetSleeves and before Handler or Serve.
func (h *Hub) EnableCoordination(home string) error {
	if h.sleeves == nil {
		return fmt.Errorf("coordination needs sleeves")
	}
	dir, err := directory.Open(filepath.Join(home, "directory.json"))
	if err != nil {
		return err
	}
	h.coord = &coordinator{h: h, dir: dir, interval: coordInterval, kick: make(chan struct{}, 1)}
	return nil
}

func (h *Hub) pokeCoord() {
	if h.coord != nil {
		h.coord.poke()
	}
}

// poke asks the run loop for a pass soon; passes already requested coalesce.
func (c *coordinator) poke() {
	select {
	case c.kick <- struct{}{}:
	default:
	}
}

// Run refreshes the directory on spawn and kill and on a slow interval until
// ctx is done.
func (c *coordinator) Run(ctx context.Context) {
	c.refresh(ctx)
	tick := time.NewTicker(c.interval)
	defer tick.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-tick.C:
		case <-c.kick:
		}
		c.refresh(ctx)
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
