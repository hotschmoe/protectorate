package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/spf13/cobra"

	"github.com/hotschmoe/protectorate/internal/backup"
	"github.com/hotschmoe/protectorate/internal/hub"
)

func hubBackupCmd() *cobra.Command {
	var to string
	c := &cobra.Command{
		Use:   "backup",
		Short: "Snapshot the hub's state directory into a timestamped tar.zst",
		Long: `Writes protectorate-hub-<time>.tar.zst and a .sha256 sidecar. The hub keeps
running. Its supervisor briefly stops headscale while copying the database and
WAL, then restarts it. Old backups are never deleted; prune them yourself.

Included: tokens, nodes, sleeves, leases, desk policies and counts, mailbox,
chat logs, job records and logs, request ids, audit log, directory, backlog and
the tailnet state (database, config, DERP certificate and keys). Not included:
bin/, auth/ and login/ (harness logins), node files, job artifacts and sleeve
workspaces.`,
		Args: cobra.NoArgs,
		RunE: func(cmd *cobra.Command, _ []string) error {
			home, err := hub.Home()
			if err != nil {
				return err
			}
			if to == "" {
				to = filepath.Join(home, "backups")
			}
			file, err := backup.Create(cmd.Context(), backup.Options{Home: home, Dir: to, Version: version})
			if err != nil {
				return err
			}
			sum, _ := os.ReadFile(file + ".sha256")
			fmt.Fprintf(cmd.OutOrStdout(), "%s\nsha256 %s\n", file, strings.Fields(string(sum) + " ")[0])
			return nil
		},
	}
	c.Flags().StringVar(&to, "to", "", "directory for the archive (default $PROTECTORATE_HOME/backups)")
	return c
}

func hubRestoreCmd() *cobra.Command {
	var home string
	c := &cobra.Command{
		Use:   "restore FILE --home DIR",
		Short: "Restore a hub backup into an empty state directory",
		Long: `Verifies the archive (its .sha256 sidecar if present, then the manifest: every
file's size and SHA-256) before writing anything, unpacks it into DIR, which must
be empty or missing, and checks the restored database's SQLite header and page
alignment. Headscale recovers its WAL and validates the database on start. Stop
the hub first, then start it with PROTECTORATE_HOME=DIR. Run
'hub tailnet install' and 'protectorate login' again on a new machine.`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			m, err := backup.Restore(cmd.Context(), args[0], home)
			if err != nil {
				return err
			}
			fmt.Fprintf(cmd.OutOrStdout(), "restored %d files into %s (backup of %s made %s by hub %s)\n",
				len(m.Files), home, m.Home, m.Created.Local().Format("2006-01-02 15:04:05"), m.Version)
			return nil
		},
	}
	c.Flags().StringVar(&home, "home", "", "state directory to restore into (must be empty or missing)")
	c.MarkFlagRequired("home")
	return c
}
