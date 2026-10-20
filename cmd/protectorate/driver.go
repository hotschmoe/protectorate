package main

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"

	"github.com/hotschmoe/protectorate/internal/driver"
)

func driverCmd() *command {
	c := &command{Use: "driver", Short: "Dev: drive a harness through its headless protocol"}
	var harness, cwd, session string
	run := &command{
		Use:   "run [flags] <prompt>",
		Short: "Run one prompt, streaming events as JSON lines and asking approvals on the terminal",
		Args:  minimumArgs(1),
		RunE: func(cmd *command, args []string) error {
			return driverRun(cmd.Context(), driver.Harness(harness), cwd, session, strings.Join(args, " "))
		},
	}
	run.Flags().StringVar(&harness, "harness", "", "claude, codex, grok or pi")
	run.Flags().StringVar(&cwd, "cwd", "", "working directory of the session")
	run.Flags().StringVar(&session, "session", "", "native session id to resume (default: new)")
	_ = run.MarkFlagRequired("harness")
	_ = run.MarkFlagRequired("cwd")
	c.AddCommand(run)
	return c
}

func driverRun(ctx context.Context, h driver.Harness, cwd, session, prompt string) error {
	ctx, stop := signal.NotifyContext(ctx, syscall.SIGINT, syscall.SIGTERM)
	defer stop()

	owners := driver.NewOwners()
	s, err := owners.OpenDriver(ctx, h, cwd, session, "driver-run", driver.Options{})
	if err != nil {
		return err
	}
	defer s.Close()
	fmt.Fprintf(os.Stderr, "session %s/%s caps=%+v\n", h, s.SessionID(), s.Caps())

	go func() {
		<-ctx.Done()
		if s.Caps().Interrupt {
			ictx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			_ = s.Interrupt(ictx)
		}
		stop()
		sig := make(chan os.Signal, 1)
		signal.Notify(sig, syscall.SIGINT, syscall.SIGTERM)
		<-sig
		_ = s.Close()
	}()

	if err := s.Send(ctx, prompt); err != nil {
		return err
	}
	enc := json.NewEncoder(os.Stdout)
	tty := bufio.NewReader(os.Stdin)
	for ev := range s.Events() {
		_ = enc.Encode(ev)
		switch ev.Kind {
		case driver.KindApproval:
			allow, err := askApproval(tty, ev.Approval)
			if err != nil {
				return err
			}
			if err := s.Respond(context.Background(), ev.Approval.ID, allow, ""); err != nil {
				return err
			}
		case driver.KindSettled:
			fmt.Fprintf(os.Stderr, "settled (%s); session %s\n", ev.StopReason, s.SessionID())
			return s.Close()
		case driver.KindExited:
			return fmt.Errorf("harness exited (code %d): %s", ev.ExitCode, strings.TrimSpace(ev.Text))
		}
	}
	return nil
}

func askApproval(in *bufio.Reader, a *driver.Approval) (bool, error) {
	for {
		fmt.Fprintf(os.Stderr, "approve %s: %s [y/n] ", a.Kind, a.Summary)
		line, err := in.ReadString('\n')
		switch strings.ToLower(strings.TrimSpace(line)) {
		case "y", "yes":
			return true, nil
		case "n", "no":
			return false, nil
		}
		if err != nil {
			return false, fmt.Errorf("no approval answer: %w", err)
		}
	}
}
