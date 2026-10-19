package main

import (
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/hotschmoe/protectorate/internal/protocol"
	"github.com/urfave/cli/v2"
)

var workspacesCommand = &cli.Command{
	Name:  "workspaces",
	Usage: "List all workspaces",
	Action: func(c *cli.Context) error {
		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		var workspaces []*protocol.WorkspaceInfo
		if err := client.Get("/api/workspaces", &workspaces); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		return out.Write(workspaces, func() ([]string, [][]string) {
			return formatWorkspacesTable(workspaces)
		})
	},
}

var cloneCommand = &cli.Command{
	Name:      "clone",
	Usage:     "Clone a git repository into a workspace",
	ArgsUsage: "<url>",
	Flags: []cli.Flag{
		&cli.StringFlag{
			Name:    "name",
			Aliases: []string{"n"},
			Usage:   "Workspace name (derived from URL if not provided)",
		},
	},
	Action: func(c *cli.Context) error {
		if c.NArg() < 1 {
			return cli.Exit("repository URL required", 1)
		}

		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		req := protocol.CloneWorkspaceRequest{
			RepoURL: c.Args().First(),
			Name:    c.String("name"),
		}

		var job protocol.CloneJob
		if err := client.Post("/api/workspaces/clone", req, &job); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return pollCloneJob(client, job.ID, out)
		}

		out.WriteMessage("Cloning %s...", req.RepoURL)
		return pollCloneJob(client, job.ID, out)
	},
}

func pollCloneJob(client *EnvoyClient, jobID string, out *OutputWriter) error {
	for {
		var job protocol.CloneJob
		if err := client.Get("/api/workspaces/clone?id="+jobID, &job); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		switch job.Status {
		case "completed":
			if out.json {
				return out.Write(job, nil)
			}
			out.WriteMessage("Cloned to: %s", job.Workspace)
			return nil
		case "failed":
			if out.json {
				return out.Write(job, nil)
			}
			return cli.Exit("Clone failed: "+job.Error, 1)
		}

		time.Sleep(500 * time.Millisecond)
	}
}

var branchesCommand = &cli.Command{
	Name:      "branches",
	Usage:     "List branches for a workspace",
	ArgsUsage: "<workspace>",
	Action: func(c *cli.Context) error {
		if c.NArg() < 1 {
			return cli.Exit("workspace path required", 1)
		}

		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		workspace := c.Args().First()
		var branches protocol.BranchListResponse
		if err := client.Get("/api/workspaces/branches?workspace="+workspace, &branches); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return out.Write(branches, nil)
		}

		out.WriteMessage("Current: %s", branches.Current)
		if len(branches.Local) > 0 {
			out.WriteMessage("\nLocal branches:")
			for _, b := range branches.Local {
				marker := "  "
				if b == branches.Current {
					marker = "* "
				}
				out.WriteMessage("%s%s", marker, b)
			}
		}
		if len(branches.Remote) > 0 {
			out.WriteMessage("\nRemote branches:")
			for _, b := range branches.Remote {
				out.WriteMessage("  %s", b)
			}
		}
		return nil
	},
}

var checkoutCommand = &cli.Command{
	Name:      "checkout",
	Usage:     "Switch branch for a workspace",
	ArgsUsage: "<workspace> <branch>",
	Action: func(c *cli.Context) error {
		if c.NArg() < 2 {
			return cli.Exit("workspace and branch required", 1)
		}

		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		workspace := c.Args().Get(0)
		branch := c.Args().Get(1)

		req := protocol.SwitchBranchRequest{
			Workspace: workspace,
			Branch:    branch,
		}

		var result protocol.WorkspaceInfo
		if err := client.Post("/api/workspaces/branches?workspace="+workspace+"&action=switch", req, &result); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return out.Write(result, nil)
		}

		out.WriteMessage("Switched to branch: %s", branch)
		return nil
	},
}

var fetchCommand = &cli.Command{
	Name:      "fetch",
	Usage:     "Fetch from remote for a workspace (or all workspaces)",
	ArgsUsage: "[workspace]",
	Action: func(c *cli.Context) error {
		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		if c.NArg() < 1 {
			var results map[string]interface{}
			if err := client.Post("/api/workspaces/branches?action=fetch-all", nil, &results); err != nil {
				return cli.Exit(err.Error(), 1)
			}
			if c.Bool("json") {
				return out.Write(results, nil)
			}
			out.WriteMessage("Fetched all workspaces")
			return nil
		}

		workspace := c.Args().First()
		var result protocol.FetchResult
		if err := client.Post("/api/workspaces/branches?workspace="+workspace+"&action=fetch", nil, &result); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return out.Write(result, nil)
		}

		out.WriteMessage("Fetched: %s", workspace)
		return nil
	},
}

var pullCommand = &cli.Command{
	Name:      "pull",
	Usage:     "Pull from remote for a workspace",
	ArgsUsage: "<workspace>",
	Action: func(c *cli.Context) error {
		if c.NArg() < 1 {
			return cli.Exit("workspace path required", 1)
		}

		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		workspace := c.Args().First()
		var result protocol.FetchResult
		if err := client.Post("/api/workspaces/branches?workspace="+workspace+"&action=pull", nil, &result); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return out.Write(result, nil)
		}

		out.WriteMessage("Pulled: %s", workspace)
		if result.Message != "" {
			out.WriteMessage(result.Message)
		}
		return nil
	},
}

var pushCommand = &cli.Command{
	Name:      "push",
	Usage:     "Push to remote for a workspace",
	ArgsUsage: "<workspace>",
	Action: func(c *cli.Context) error {
		if c.NArg() < 1 {
			return cli.Exit("workspace path required", 1)
		}

		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		workspace := c.Args().First()
		var result protocol.FetchResult
		if err := client.Post("/api/workspaces/branches?workspace="+workspace+"&action=push", nil, &result); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return out.Write(result, nil)
		}

		out.WriteMessage("Pushed: %s", workspace)
		if result.Message != "" {
			out.WriteMessage(result.Message)
		}
		return nil
	},
}

func formatWorkspacesTable(workspaces []*protocol.WorkspaceInfo) ([]string, [][]string) {
	headers := []string{"NAME", "BRANCH", "STATUS", "CSTACK", "SLEEVE"}
	rows := make([][]string, 0, len(workspaces))

	for _, w := range workspaces {
		name := filepath.Base(w.Path)
		branch := "-"
		if w.Git != nil {
			branch = w.Git.Branch
			if w.Git.IsDirty {
				branch += "*"
			}
		}

		var status string
		if w.SizeCritical {
			status = "critical"
		} else if w.SizeWarning {
			status = "large"
		} else {
			status = "ok"
		}

		cstack := "-"
		if w.Cstack != nil && w.Cstack.Exists {
			cstack = fmt.Sprintf("%d/%d", w.Cstack.InProgress, w.Cstack.Total)
		}

		sleeve := "-"
		if w.InUse && w.SleeveName != "" {
			sleeve = w.SleeveName
		}

		rows = append(rows, []string{name, branch, status, cstack, sleeve})
	}

	return headers, rows
}
