package main

import (
	"fmt"
	"os"

	"github.com/hotschmoe/protectorate/internal/protocol"
	"github.com/urfave/cli/v2"
)

var statusCommand = &cli.Command{
	Name:  "status",
	Usage: "List all sleeves",
	Action: func(c *cli.Context) error {
		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		var sleeves []*protocol.SleeveInfo
		if err := client.Get("/api/sleeves", &sleeves); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		return out.Write(sleeves, func() ([]string, [][]string) {
			return formatSleevesTable(sleeves)
		})
	},
}

var spawnCommand = &cli.Command{
	Name:      "spawn",
	Usage:     "Spawn a new sleeve",
	ArgsUsage: "<workspace>",
	Flags: []cli.Flag{
		&cli.StringFlag{
			Name:    "name",
			Aliases: []string{"n"},
			Usage:   "Sleeve name (auto-generated if not provided)",
		},
		&cli.Int64Flag{
			Name:  "memory",
			Usage: "Memory limit in MB",
		},
		&cli.IntFlag{
			Name:  "cpu",
			Usage: "CPU limit (number of cores)",
		},
	},
	Action: func(c *cli.Context) error {
		if c.NArg() < 1 {
			return cli.Exit("workspace path required", 1)
		}

		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		req := protocol.SpawnSleeveRequest{
			Workspace:     c.Args().First(),
			Name:          c.String("name"),
			MemoryLimitMB: c.Int64("memory"),
			CPULimit:      c.Int("cpu"),
		}

		var sleeve protocol.SleeveInfo
		if err := client.Post("/api/sleeves", req, &sleeve); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return out.Write(sleeve, nil)
		}

		out.WriteMessage("Spawned sleeve: %s", sleeve.Name)
		out.WriteMessage("Container: %s", sleeve.ContainerName)
		out.WriteMessage("Workspace: %s", sleeve.Workspace)
		return nil
	},
}

var killCommand = &cli.Command{
	Name:      "kill",
	Usage:     "Kill a sleeve",
	ArgsUsage: "<name>",
	Action: func(c *cli.Context) error {
		if c.NArg() < 1 {
			return cli.Exit("sleeve name required", 1)
		}

		name := c.Args().First()
		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		if err := client.Delete("/api/sleeves/" + name); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return out.Write(map[string]string{"status": "killed", "name": name}, nil)
		}

		out.WriteMessage("Killed sleeve: %s", name)
		return nil
	},
}

var infoCommand = &cli.Command{
	Name:      "info",
	Usage:     "Get detailed sleeve information",
	ArgsUsage: "<name>",
	Action: func(c *cli.Context) error {
		if c.NArg() < 1 {
			return cli.Exit("sleeve name required", 1)
		}

		name := c.Args().First()
		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		var sleeve protocol.SleeveInfo
		if err := client.Get("/api/sleeves/"+name, &sleeve); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return out.Write(sleeve, nil)
		}

		out.WriteMessage("Name:       %s", sleeve.Name)
		out.WriteMessage("Container:  %s", sleeve.ContainerName)
		out.WriteMessage("Workspace:  %s", sleeve.Workspace)
		out.WriteMessage("Status:     %s", sleeve.Status)
		out.WriteMessage("Integrity:  %.0f%%", sleeve.Integrity*100)
		if sleeve.DHF != "" {
			out.WriteMessage("DHF:        %s %s", sleeve.DHF, sleeve.DHFVersion)
		}
		if sleeve.Constrained {
			out.WriteMessage("Memory:     %d MB", sleeve.MemoryLimitMB)
			out.WriteMessage("CPU:        %d cores", sleeve.CPULimit)
		}
		if sleeve.Resources != nil {
			out.WriteMessage("Mem Usage:  %.1f%%", sleeve.Resources.MemoryPercent)
			out.WriteMessage("CPU Usage:  %.1f%%", sleeve.Resources.CPUPercent)
		}
		return nil
	},
}

func formatSleevesTable(sleeves []*protocol.SleeveInfo) ([]string, [][]string) {
	headers := []string{"NAME", "STATUS", "WORKSPACE", "DHF", "INTEGRITY"}
	rows := make([][]string, 0, len(sleeves))

	for _, s := range sleeves {
		dhf := s.DHF
		if s.DHFVersion != "" {
			dhf = fmt.Sprintf("%s %s", s.DHF, s.DHFVersion)
		}
		integrity := fmt.Sprintf("%.0f%%", s.Integrity*100)
		rows = append(rows, []string{s.Name, s.Status, s.Workspace, dhf, integrity})
	}

	return headers, rows
}
