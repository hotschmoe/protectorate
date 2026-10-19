package main

import (
	"fmt"
	"os"

	"github.com/hotschmoe/protectorate/internal/protocol"
	"github.com/urfave/cli/v2"
)

const (
	exitCodeValid        = 0
	exitCodeExpired      = 1
	exitCodeExpiringSoon = 2
)

var authCommand = &cli.Command{
	Name:  "auth",
	Usage: "Manage authentication for AI providers",
	Subcommands: []*cli.Command{
		{
			Name:  "status",
			Usage: "Show authentication status for all providers",
			Action: func(c *cli.Context) error {
				return authStatusAction(c)
			},
		},
		{
			Name:      "login",
			Usage:     "Login to a provider",
			ArgsUsage: "<provider> <token>",
			Action: func(c *cli.Context) error {
				if c.NArg() < 2 {
					return cli.Exit("usage: envoy auth login <provider> <token>", 1)
				}

				provider := c.Args().Get(0)
				token := c.Args().Get(1)

				client := NewEnvoyClient(c.String("server"))
				out := NewOutputWriter(c.Bool("json"), os.Stdout)

				body := map[string]string{"token": token}

				var result protocol.AuthLoginResult
				if err := client.Post("/api/auth/"+provider+"/login", body, &result); err != nil {
					return cli.Exit(err.Error(), 1)
				}

				if c.Bool("json") {
					return out.Write(result, nil)
				}

				if result.Success {
					out.WriteMessage("%s: login successful (%s)", provider, result.Method)
				} else {
					out.WriteMessage("%s: login failed - %s", provider, result.Error)
				}
				return nil
			},
		},
		{
			Name:      "revoke",
			Usage:     "Revoke credentials for a provider",
			ArgsUsage: "<provider>",
			Action: func(c *cli.Context) error {
				if c.NArg() < 1 {
					return cli.Exit("usage: envoy auth revoke <provider>", 1)
				}

				client := NewEnvoyClient(c.String("server"))
				out := NewOutputWriter(c.Bool("json"), os.Stdout)

				provider := c.Args().First()

				var result protocol.AuthRevokeResult
				if err := client.Delete("/api/auth/" + provider); err != nil {
					return cli.Exit(err.Error(), 1)
				}

				if c.Bool("json") {
					return out.Write(result, nil)
				}

				out.WriteMessage("%s: credentials revoked", provider)
				return nil
			},
		},
		{
			Name:  "sync",
			Usage: "Sync credentials from CLI tool locations to shared volume",
			ArgsUsage: "[provider]",
			Action: func(c *cli.Context) error {
				client := NewEnvoyClient(c.String("server"))
				out := NewOutputWriter(c.Bool("json"), os.Stdout)

				provider := c.Args().First()
				if provider == "" {
					provider = "all"
				}

				var result map[string]interface{}
				if err := client.Post("/api/auth/sync", map[string]string{"provider": provider}, &result); err != nil {
					return cli.Exit(err.Error(), 1)
				}

				if c.Bool("json") {
					return out.Write(result, nil)
				}

				if msg, ok := result["message"].(string); ok {
					out.WriteMessage(msg)
				} else {
					out.WriteMessage("Sync completed")
				}
				return nil
			},
		},
		{
			Name:  "check",
			Usage: "Check auth status with exit codes (0=valid, 1=expired, 2=expiring soon)",
			Flags: []cli.Flag{
				&cli.BoolFlag{Name: "quiet", Aliases: []string{"q"}, Usage: "Only return exit code, no output"},
			},
			Action: authCheckAction,
		},
	},
	Action: func(c *cli.Context) error {
		return authStatusAction(c)
	},
}

func authStatusAction(c *cli.Context) error {
	client := NewEnvoyClient(c.String("server"))
	out := NewOutputWriter(c.Bool("json"), os.Stdout)

	var status protocol.AuthStatus
	if err := client.Get("/api/auth/status", &status); err != nil {
		return cli.Exit(err.Error(), 1)
	}

	if c.Bool("json") {
		return out.Write(status, nil)
	}

	providers := []protocol.AuthProvider{
		protocol.AuthProviderClaude,
		protocol.AuthProviderGemini,
		protocol.AuthProviderCodex,
		protocol.AuthProviderGit,
	}

	for _, provider := range providers {
		ps, ok := status.Providers[provider]
		if !ok {
			continue
		}

		if ps.Authenticated {
			method := ps.Method
			if method == "" {
				method = "unknown"
			}
			fmt.Printf("%-8s authenticated (%s)\n", provider+":", method)
		} else {
			fmt.Printf("%-8s not authenticated\n", provider+":")
		}
	}

	return nil
}

func authCheckAction(c *cli.Context) error {
	client := NewEnvoyClient(c.String("server"))
	out := NewOutputWriter(c.Bool("json"), os.Stdout)
	quiet := c.Bool("quiet")

	var result protocol.AuthCheckResult
	if err := client.Get("/api/auth/check", &result); err != nil {
		return cli.Exit(err.Error(), 1)
	}

	if c.Bool("json") {
		return out.Write(result, nil)
	}

	if !quiet {
		printAuthCheckResult(&result)
	}

	if result.Expired {
		return cli.Exit("", exitCodeExpired)
	}
	if result.ExpiringSoon {
		return cli.Exit("", exitCodeExpiringSoon)
	}
	return nil
}

func printAuthCheckResult(result *protocol.AuthCheckResult) {
	providers := []protocol.AuthProvider{
		protocol.AuthProviderClaude,
		protocol.AuthProviderGemini,
		protocol.AuthProviderCodex,
		protocol.AuthProviderGit,
	}

	for _, provider := range providers {
		info, ok := result.Providers[provider]
		if !ok {
			continue
		}

		icon := "[*]"
		switch info.Status {
		case "expired", "missing":
			icon = "[x]"
		case "expiring_soon":
			icon = "[!]"
		}

		message := info.Message
		if message == "" {
			message = info.Status
		}
		fmt.Printf("%s %-8s %s\n", icon, provider+":", message)
	}

	fmt.Println()
	switch {
	case result.Expired:
		fmt.Println("Status: EXPIRED - credentials need renewal")
	case result.ExpiringSoon:
		fmt.Println("Status: WARNING - credentials expiring soon")
	default:
		fmt.Println("Status: OK - all credentials valid")
	}
}
