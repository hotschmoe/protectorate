package sleeve

import (
	"bytes"
	"context"
	"fmt"
	"os"
	"time"
)

// readEnvFile recovers the caller env a sleeve was spawned with from its
// bwrap --args file, so later launches of the same sleeve keep it.
func (c Config) readEnvFile(name string) (map[string]string, error) {
	raw, err := os.ReadFile(c.envFile(name))
	if os.IsNotExist(err) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("read sleeve env: %w", err)
	}
	parts := bytes.Split(bytes.TrimSuffix(raw, []byte{0}), []byte{0})
	if len(parts)%3 != 0 {
		return nil, fmt.Errorf("read sleeve env: malformed %s", c.envFile(name))
	}
	env := make(map[string]string, len(parts)/3)
	for i := 0; i < len(parts); i += 3 {
		if string(parts[i]) != "--setenv" {
			return nil, fmt.Errorf("read sleeve env: unexpected %q in %s", parts[i], c.envFile(name))
		}
		env[string(parts[i+1])] = string(parts[i+2])
	}
	return env, nil
}

// launch plans, and prepares the host side of, one run of info's harness
// with args, for a caller that supervises the process itself (a driver).
func (c Config) launch(info Info, args []string) ([]string, error) {
	bin, err := c.Locate(info.Harness)
	if err != nil {
		return nil, err
	}
	env, err := c.readEnvFile(info.Name)
	if err != nil {
		return nil, err
	}
	launch, err := c.Plan(Spec{
		Name: info.Name, Harness: info.Harness, Args: args, Workspace: info.Workspace, Env: env, Limits: info.Limits(),
	}, bin)
	if err != nil {
		return nil, err
	}
	// systemd-run refuses a unit name that is still being torn down.
	c.waitGone(context.Background(), info.Name, 5*time.Second)
	if err := launch.Layout.Apply(); err != nil {
		return nil, fmt.Errorf("prepare sleeve layout: %w", err)
	}
	if err := c.writeEnvFile(info.Name, launch); err != nil {
		return nil, err
	}
	return launch.Argv, nil
}

// Command returns the argv builder for a driver.Options.Command that runs
// the named sleeve's harness inside its sandbox and systemd scope, with the
// same mounts, credentials and limits the terminal path uses. The harness
// executable the driver names is replaced by the one Locate finds.
func (m *Manager) Command(name string) (func(bin string, args []string) ([]string, error), error) {
	info, err := m.Get(name)
	if err != nil {
		return nil, err
	}
	return func(_ string, args []string) ([]string, error) {
		return m.cfg.launch(info, args)
	}, nil
}
