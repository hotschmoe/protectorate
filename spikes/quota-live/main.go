// Throwaway runner: reads live quota once and prints a redacted table.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/hotschmoe/protectorate/internal/quota"
)

func piBaseURLs() []string {
	home, err := os.UserHomeDir()
	if err != nil {
		return nil
	}
	data, err := os.ReadFile(filepath.Join(home, ".pi", "agent", "models.json"))
	if err != nil {
		return nil
	}
	var cfg struct {
		Providers map[string]struct {
			BaseURL string `json:"baseUrl"`
		} `json:"providers"`
	}
	if json.Unmarshal(data, &cfg) != nil {
		return nil
	}
	var urls []string
	for _, p := range cfg.Providers {
		if p.BaseURL != "" {
			urls = append(urls, p.BaseURL)
		}
	}
	return urls
}

func main() {
	readers := []quota.Reader{quota.NewClaude(), quota.NewCodex(), quota.NewGrok(), quota.NewAntigravity()}
	urls := piBaseURLs()
	if u := os.Getenv("LOCAL_URL"); u != "" {
		urls = []string{u}
	}
	for _, u := range urls {
		readers = append(readers, quota.NewLocal(u))
	}
	now := time.Now()
	for _, s := range quota.ReadAll(context.Background(), readers, 45*time.Second) {
		status := "ok"
		if s.Err != "" {
			status = "ERR: " + s.Err
		}
		fmt.Printf("%-12s plan=%-8s source=%-18s %s\n", s.Provider, s.Plan, s.Source, status)
		for _, w := range s.Windows {
			fmt.Printf("    window %-12s used=%5.1f%% resets_in=%s\n", w.Name, w.UsedPercent, until(now, w.ResetsAt))
		}
		for _, r := range s.Resets {
			fmt.Printf("    reset  %-12s x%d expires_in=%s\n", r.Kind, r.Count, until(now, r.ExpiresAt))
		}
		if s.Provider == "local" && s.Err == "" {
			fmt.Printf("    models=%d latency=%s first=%s\n", len(s.Models), s.Latency.Round(time.Millisecond), strings.Join(s.Models[:min(1, len(s.Models))], ","))
		}
	}
}

func until(now, t time.Time) string {
	if t.IsZero() {
		return "-"
	}
	return t.Sub(now).Round(time.Minute).String()
}
