package main

import (
	"log"
	"os"

	"github.com/urfave/cli/v2"
)

func main() {
	app := &cli.App{
		Name:    "envoy",
		Usage:   "Protectorate orchestration manager",
		Version: "0.1.0",
		Flags: []cli.Flag{
			&cli.StringFlag{
				Name:    "server",
				Aliases: []string{"s"},
				Value:   "http://localhost:7470",
				Usage:   "Envoy server URL",
				EnvVars: []string{"ENVOY_URL"},
			},
			&cli.BoolFlag{
				Name:    "json",
				Aliases: []string{"j"},
				Usage:   "Output in JSON format",
			},
		},
		Commands: []*cli.Command{
			serveCommand,
			statusCommand,
			spawnCommand,
			killCommand,
			infoCommand,
			doctorCommand,
			workspacesCommand,
			cloneCommand,
			branchesCommand,
			checkoutCommand,
			fetchCommand,
			pullCommand,
			pushCommand,
			statsCommand,
			authCommand,
			configCommand,
		},
		DefaultCommand: "serve",
	}

	if err := app.Run(os.Args); err != nil {
		log.Fatal(err)
	}
}
