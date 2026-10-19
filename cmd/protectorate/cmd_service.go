package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"github.com/spf13/cobra"

	"github.com/hotschmoe/protectorate/internal/hub"
	"github.com/hotschmoe/protectorate/internal/svcgen"
)

// svcFlags are shared by every `service` generator.
type svcFlags struct {
	binary, home, writeDir string
	install                bool
}

func (f *svcFlags) bind(c *cobra.Command) {
	c.Flags().StringVar(&f.binary, "binary", "", "absolute path of the protectorate binary (default: this executable)")
	c.Flags().StringVar(&f.home, "home", "", "PROTECTORATE_HOME baked into the service (default: resolved state dir)")
	c.Flags().StringVar(&f.writeDir, "write", "", "write files into this directory instead of printing")
	c.Flags().BoolVar(&f.install, "install", false, "write files into their standard per-user location; never enables or starts anything")
}

func (f *svcFlags) spec(role string, args []string) (svcgen.Spec, error) {
	s := svcgen.Spec{Role: role, Binary: f.binary, Args: args, Home: f.home}
	if s.Binary == "" {
		exe, err := os.Executable()
		if err != nil {
			return s, fmt.Errorf("locate this executable (pass --binary): %w", err)
		}
		s.Binary = exe
	}
	if s.Home == "" {
		h, err := hub.Home()
		if err != nil {
			return s, err
		}
		s.Home = h
	}
	return s, nil
}

// emit prints content, or writes it to dir/name. Existing files are never
// overwritten.
func emit(w io.Writer, dir, name string, content []byte) (string, error) {
	if dir == "" {
		_, err := w.Write(content)
		return "", err
	}
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", fmt.Errorf("create %s: %w", dir, err)
	}
	p := filepath.Join(dir, name)
	f, err := os.OpenFile(p, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o644)
	if err != nil {
		if errors.Is(err, os.ErrExist) {
			return "", fmt.Errorf("%s exists; remove it yourself if you want it regenerated", p)
		}
		return "", err
	}
	if _, err := f.Write(content); err != nil {
		f.Close()
		return "", fmt.Errorf("write %s: %w", p, err)
	}
	if err := f.Close(); err != nil {
		return "", err
	}
	fmt.Fprintf(w, "wrote %s\n", p)
	return p, nil
}

func (f *svcFlags) target(def string) (string, error) {
	if f.writeDir != "" && f.install {
		return "", errors.New("--write and --install are mutually exclusive")
	}
	if f.writeDir != "" {
		return f.writeDir, nil
	}
	if f.install {
		return def, nil
	}
	return "", nil
}

func serviceCmd() *cobra.Command {
	c := &cobra.Command{
		Use:   "service",
		Short: "Generate service definitions for hub and node (prints by default; never starts anything)",
		Long: "Generate service definitions. Arguments after `--` are passed to `<role> serve`.\n" +
			"Output is printed unless --write DIR or --install is given. Nothing is enabled or started.",
	}
	c.AddCommand(serviceSystemdCmd(), serviceWindowsTaskCmd(), serviceWindowsCmd(), serviceTermuxCmd())
	return c
}

func roleArg(cmd *cobra.Command, args []string) error {
	if len(args) == 0 {
		return errors.New("role required: hub or node")
	}
	if args[0] != svcgen.RoleHub && args[0] != svcgen.RoleNode {
		return fmt.Errorf("role must be hub or node, got %q", args[0])
	}
	return nil
}

func serviceSystemdCmd() *cobra.Command {
	var f svcFlags
	var user, path, tmux string
	c := &cobra.Command{
		Use:   "systemd <hub|node> [flags] [-- serve-flags]",
		Short: "Generate a systemd unit (user unit, or system unit with --user-name)",
		Args:  roleArg,
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := f.spec(args[0], args[1:])
			if err != nil {
				return err
			}
			s.User, s.Path, s.TmuxSocket = user, path, tmux
			if s.TmuxSocket == "" {
				s.TmuxSocket = os.Getenv("PROTECTORATE_TMUX_SOCKET")
			}
			unit, err := svcgen.Systemd(s)
			if err != nil {
				return err
			}
			def := ""
			if f.install {
				if user != "" {
					def = "/etc/systemd/system"
				} else {
					u, err := os.UserHomeDir()
					if err != nil {
						return err
					}
					def = filepath.Join(u, ".config", "systemd", "user")
				}
			}
			dir, err := f.target(def)
			if err != nil {
				return err
			}
			w := cmd.OutOrStdout()
			p, err := emit(w, dir, s.Name()+".service", []byte(unit))
			if err != nil {
				return err
			}
			if p == "" {
				p = s.Name() + ".service"
			} else {
				p = filepath.Base(p)
				if f.install {
					p = filepath.Join(dir, p)
				}
			}
			fmt.Fprintf(cmd.ErrOrStderr(), "\n%s", svcgen.SystemdNotes(s, p))
			return nil
		},
	}
	f.bind(c)
	c.Flags().StringVar(&user, "user-name", "", "generate a system-level unit running as this account (default: user unit)")
	c.Flags().StringVar(&path, "path", "", "PATH for the service (default: common user bin dirs)")
	c.Flags().StringVar(&tmux, "tmux-socket", "", "PROTECTORATE_TMUX_SOCKET for the hub (default: current env)")
	return c
}

func serviceWindowsTaskCmd() *cobra.Command {
	var f svcFlags
	var trigger string
	c := &cobra.Command{
		Use:   "windows-task [flags] [-- serve-flags]",
		Short: "Generate a Task Scheduler XML for `node serve` (UTF-16, for schtasks /Create /XML)",
		Args:  cobra.ArbitraryArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := f.spec(svcgen.RoleNode, args)
			if err != nil {
				return err
			}
			x, err := svcgen.TaskXML(s, trigger)
			if err != nil {
				return err
			}
			if f.install {
				return errors.New("--install is not supported for scheduled tasks; use --write and register with schtasks")
			}
			dir, err := f.target("")
			if err != nil {
				return err
			}
			w := cmd.OutOrStdout()
			name := s.Name() + ".xml"
			if dir == "" {
				fmt.Fprint(w, x)
				fmt.Fprintf(cmd.ErrOrStderr(), "\n%s", svcgen.TaskNotes(s, trigger, name))
				return nil
			}
			p, err := emit(w, dir, name, svcgen.EncodeUTF16LE(x))
			if err != nil {
				return err
			}
			fmt.Fprintf(cmd.ErrOrStderr(), "\n%s", svcgen.TaskNotes(s, trigger, p))
			return nil
		},
	}
	f.bind(c)
	c.Flags().StringVar(&trigger, "trigger", svcgen.TriggerLogon, "when to start: logon or boot")
	return c
}

func serviceWindowsCmd() *cobra.Command {
	var f svcFlags
	c := &cobra.Command{
		Use:   "install-windows [flags] [-- serve-flags]",
		Short: "Print the sc.exe commands that register `node serve` as a Windows service",
		Args:  cobra.ArbitraryArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := f.spec(svcgen.RoleNode, args)
			if err != nil {
				return err
			}
			out, err := svcgen.WindowsService(s, s.Home)
			if err != nil {
				return err
			}
			dir, err := f.target("")
			if err != nil {
				return err
			}
			if f.install {
				return errors.New("--install is not supported: run the printed commands in an elevated prompt")
			}
			_, err = emit(cmd.OutOrStdout(), dir, "protectorate-node-service.cmd", []byte(out))
			return err
		},
	}
	f.bind(c)
	return c
}

func serviceTermuxCmd() *cobra.Command {
	var f svcFlags
	c := &cobra.Command{
		Use:   "termux [flags] [-- serve-flags]",
		Short: "Generate a termux-services (runit) run script and a Termux:Boot script for `node serve`",
		Args:  cobra.ArbitraryArgs,
		RunE: func(cmd *cobra.Command, args []string) error {
			s, err := f.spec(svcgen.RoleNode, args)
			if err != nil {
				return err
			}
			run, err := svcgen.TermuxRun(s)
			if err != nil {
				return err
			}
			boot := svcgen.TermuxBoot()
			w := cmd.OutOrStdout()
			if f.writeDir != "" && f.install {
				return errors.New("--write and --install are mutually exclusive")
			}
			switch {
			case f.install:
				prefix := envOr("PREFIX", "/data/data/com.termux/files/usr")
				u, err := os.UserHomeDir()
				if err != nil {
					return err
				}
				if err := emitExec(w, filepath.Join(prefix, "var", "service", s.Name()), "run", run); err != nil {
					return err
				}
				if err := emitExec(w, filepath.Join(u, ".termux", "boot"), s.Name(), boot); err != nil {
					return err
				}
			case f.writeDir != "":
				if err := emitExec(w, filepath.Join(f.writeDir, s.Name()), "run", run); err != nil {
					return err
				}
				if err := emitExec(w, filepath.Join(f.writeDir, "boot"), s.Name(), boot); err != nil {
					return err
				}
			default:
				fmt.Fprintf(w, "# $PREFIX/var/service/%s/run\n%s\n# ~/.termux/boot/%s\n%s", s.Name(), run, s.Name(), boot)
			}
			fmt.Fprintf(cmd.ErrOrStderr(), "\n%s", svcgen.TermuxNotes(s))
			return nil
		},
	}
	f.bind(c)
	return c
}

func emitExec(w io.Writer, dir, name, content string) error {
	p, err := emit(w, dir, name, []byte(content))
	if err != nil {
		return err
	}
	return os.Chmod(p, 0o755)
}
