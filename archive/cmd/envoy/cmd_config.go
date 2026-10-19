package main

import (
	"fmt"
	"os"
	"strings"

	"github.com/urfave/cli/v2"
)

var configCommand = &cli.Command{
	Name:  "config",
	Usage: "Manage envoy configuration",
	Subcommands: []*cli.Command{
		{
			Name:      "get",
			Usage:     "Get configuration value(s)",
			ArgsUsage: "[key]",
			Action: func(c *cli.Context) error {
				client := NewEnvoyClient(c.String("server"))
				out := NewOutputWriter(c.Bool("json"), os.Stdout)

				key := c.Args().First()
				if key == "" {
					var config map[string]interface{}
					if err := client.Get("/api/config", &config); err != nil {
						return cli.Exit(err.Error(), 1)
					}
					return out.Write(config, nil)
				}

				var value interface{}
				if err := client.Get("/api/config/"+key, &value); err != nil {
					return cli.Exit(err.Error(), 1)
				}

				if c.Bool("json") {
					return out.Write(value, nil)
				}

				fmt.Printf("%s = %v\n", key, value)
				return nil
			},
		},
		{
			Name:      "set",
			Usage:     "Set configuration value",
			ArgsUsage: "<key> <value>",
			Action: func(c *cli.Context) error {
				if c.NArg() < 2 {
					return cli.Exit("usage: envoy config set <key> <value>", 1)
				}

				client := NewEnvoyClient(c.String("server"))
				out := NewOutputWriter(c.Bool("json"), os.Stdout)

				key := c.Args().Get(0)
				value := strings.Join(c.Args().Slice()[1:], " ")

				body := map[string]string{"value": value}
				var result map[string]interface{}
				if err := client.Put("/api/config/"+key, body, &result); err != nil {
					return cli.Exit(err.Error(), 1)
				}

				if c.Bool("json") {
					return out.Write(result, nil)
				}

				fmt.Printf("%s = %v\n", key, result["value"])
				return nil
			},
		},
	},
	Action: func(c *cli.Context) error {
		client := NewEnvoyClient(c.String("server"))
		out := NewOutputWriter(c.Bool("json"), os.Stdout)

		var config map[string]interface{}
		if err := client.Get("/api/config", &config); err != nil {
			return cli.Exit(err.Error(), 1)
		}
		return out.Write(config, nil)
	},
}
