package tailnet

import (
	"context"
	"fmt"
	"sync"
	"time"
)

// Service is the hub's whole networking stack: supervised headscale, the admin
// client and the hub's own tsnet member.
type Service struct {
	Net   *Net
	Admin *Admin

	stop context.CancelFunc
	wg   sync.WaitGroup
}

// Start launches headscale, waits for it, and joins the hub to its tailnet.
func Start(ctx context.Context, home string, cfg Config, logf func(string, ...any)) (*Service, error) {
	admin := NewAdmin(home, cfg)
	supCtx, stop := context.WithCancel(context.Background())
	s := &Service{Admin: admin, stop: stop}
	supErr := make(chan error, 1)
	s.wg.Go(func() {
		sup := &Supervisor{Home: home, Cfg: cfg, Logf: logf}
		supErr <- sup.Run(supCtx)
	})
	fail := func(err error) (*Service, error) {
		s.Close()
		return nil, err
	}

	readyCtx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()
	ready := make(chan error, 1)
	go func() { ready <- admin.WaitReady(readyCtx) }()
	select {
	case err := <-supErr:
		return fail(fmt.Errorf("headscale: %w", err))
	case err := <-ready:
		if err != nil {
			return fail(err)
		}
	}
	if err := admin.Ensure(ctx); err != nil {
		return fail(err)
	}
	n, err := UpHub(ctx, home, cfg, admin)
	if err != nil {
		return fail(err)
	}
	s.Net = n
	s.wg.Go(func() { s.enforceExpiry(supCtx, logf) })
	return s, nil
}

func (s *Service) enforceExpiry(ctx context.Context, logf func(string, ...any)) {
	t := time.NewTicker(time.Minute)
	defer t.Stop()
	for {
		if err := s.Admin.EnforceExpiry(ctx); err != nil && ctx.Err() == nil && logf != nil {
			logf("enforce key expiry: %v", err)
		}
		select {
		case <-ctx.Done():
			return
		case <-t.C:
		}
	}
}

// Close leaves the tailnet, then stops headscale and waits for it.
func (s *Service) Close() error {
	var err error
	if s.Net != nil {
		err = s.Net.Close()
	}
	s.stop()
	s.wg.Wait()
	return err
}
