package main

import (
	"os"

	"github.com/hotschmoe/protectorate/internal/protocol"
	"github.com/urfave/cli/v2"
)

var doctorCommand = &cli.Command{
	Name:  "doctor",
	Usage: "Run system health diagnostics",
	Action: func(c *cli.Context) error {
		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		var checks []protocol.DoctorCheck
		if err := client.Get("/api/doctor", &checks); err != nil {
			return cli.Exit(err.Error(), 1)
		}

		if c.Bool("json") {
			return out.Write(checks, nil)
		}

		hasIssues := false
		for _, check := range checks {
			icon := statusIcon(check.Status)
			out.WriteMessage("%s %s: %s", icon, check.Name, check.Message)
			if check.Suggestion != "" && check.Status != "pass" {
				out.WriteMessage("  -> %s", check.Suggestion)
				hasIssues = true
			}
		}

		if hasIssues {
			out.WriteMessage("\nSome checks have issues. See suggestions above.")
		} else {
			out.WriteMessage("\nAll checks passed.")
		}

		return nil
	},
}

func statusIcon(status string) string {
	switch status {
	case "pass":
		return "[OK]"
	case "warning":
		return "[!!]"
	case "fail":
		return "[XX]"
	default:
		return "[??]"
	}
}
