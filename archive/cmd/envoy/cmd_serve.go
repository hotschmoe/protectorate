package main

import (
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/hotschmoe/protectorate/internal/config"
	"github.com/hotschmoe/protectorate/internal/envoy"
	"github.com/urfave/cli/v2"
)

var serveCommand = &cli.Command{
	Name:  "serve",
	Usage: "Start the envoy HTTP server",
	Action: func(c *cli.Context) error {
		cfg := config.LoadEnvoyConfig()

		srv, err := envoy.NewServer(cfg)
		if err != nil {
			return cli.Exit("failed to create server: "+err.Error(), 1)
		}

		go func() {
			log.Printf("envoy starting on port %d", cfg.Server.Port)
			if err := srv.Start(); err != nil {
				log.Fatalf("server error: %v", err)
			}
		}()

		// Wait for OS signal or restart request
		sigCh := make(chan os.Signal, 1)
		signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

		select {
		case <-sigCh:
			log.Println("received shutdown signal")
		case <-srv.ShutdownCh():
			log.Println("restart requested via API")
		}

		log.Println("shutting down...")
		if err := srv.Shutdown(); err != nil {
			log.Printf("shutdown error: %v", err)
		}

		return nil
	},
}
