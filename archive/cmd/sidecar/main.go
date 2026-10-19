package main

import (
	"context"
	"log"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"syscall"
	"time"

	"github.com/hotschmoe/protectorate/internal/sidecar"
)

func main() {
	cfg := &sidecar.Config{
		Port:          getEnvInt("SIDECAR_PORT", 8080),
		SleeveName:    getEnv("SLEEVE_NAME", "unknown"),
		WorkspacePath: getEnv("WORKSPACE_PATH", "/home/agent/workspace"),
	}

	server := sidecar.NewServer(cfg)

	// Signal handling for graceful shutdown
	stop := make(chan os.Signal, 1)
	signal.Notify(stop, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		log.Printf("sidecar starting on :%d (sleeve=%s, workspace=%s)",
			cfg.Port, cfg.SleeveName, cfg.WorkspacePath)
		if err := server.Start(); err != nil && err != http.ErrServerClosed {
			log.Fatalf("server error: %v", err)
		}
	}()

	<-stop
	log.Println("shutting down...")

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := server.Shutdown(ctx); err != nil {
		log.Printf("shutdown error: %v", err)
	}
}

func getEnv(key, defaultVal string) string {
	if val := os.Getenv(key); val != "" {
		return val
	}
	return defaultVal
}

func getEnvInt(key string, defaultVal int) int {
	if val := os.Getenv(key); val != "" {
		if i, err := strconv.Atoi(val); err == nil {
			return i
		}
	}
	return defaultVal
}
