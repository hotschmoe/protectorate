package main

import (
	"os"

	"github.com/hotschmoe/protectorate/internal/protocol"
	"github.com/urfave/cli/v2"
)

var statsCommand = &cli.Command{
	Name:  "stats",
	Usage: "Show host resource statistics",
	Action: func(c *cli.Context) error {
		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		var stats protocol.HostStats
		if err := client.Get("/api/host/stats", &stats); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return out.Write(stats, nil)
		}

		if stats.CPU != nil {
			out.WriteMessage("CPU:    %.1f%% (%d cores, %d threads)",
				stats.CPU.UsagePercent, stats.CPU.Cores, stats.CPU.Threads)
		}
		if stats.Memory != nil {
			usedGB := float64(stats.Memory.UsedBytes) / (1024 * 1024 * 1024)
			totalGB := float64(stats.Memory.TotalBytes) / (1024 * 1024 * 1024)
			out.WriteMessage("Memory: %.1f%% (%.1f/%.1f GB)",
				stats.Memory.Percent, usedGB, totalGB)
		}
		if stats.Disk != nil {
			usedGB := float64(stats.Disk.UsedBytes) / (1024 * 1024 * 1024)
			totalGB := float64(stats.Disk.TotalBytes) / (1024 * 1024 * 1024)
			out.WriteMessage("Disk:   %.1f%% (%.1f/%.1f GB)",
				stats.Disk.Percent, usedGB, totalGB)
		}
		if stats.Docker != nil {
			out.WriteMessage("Docker: %d/%d containers running (max %d)",
				stats.Docker.RunningContainers, stats.Docker.TotalContainers, stats.Docker.MaxContainers)
		}

		return nil
	},
}
