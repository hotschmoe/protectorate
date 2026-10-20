package e2e

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

func (c *cluster) rejectedCLI(want string, args ...string) {
	c.t.Helper()
	cmd := exec.CommandContext(c.ctx, c.binary, append([]string{"--hub", c.url}, args...)...)
	cmd.Env = c.env
	out, err := c.commandOutput(cmd)
	if err == nil || !strings.Contains(string(out), want) {
		c.t.Fatalf("expected CLI rejection containing %q: args=%v err=%v output=%s", want, args, err, out)
	}
}
func (c *cluster) rejectFileEscapes(input string) {
	c.t.Helper()
	outside := filepath.Join(c.root, "outside-transfer")
	if err := os.MkdirAll(outside, 0700); err != nil {
		c.t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(c.root, "node", "node-files", "escape")); err != nil {
		c.t.Fatalf("create confinement probe symlink: %v", err)
	}
	for _, path := range []string{"../outside-transfer/sentinel", filepath.Join(outside, "sentinel"), "escape/sentinel"} {
		c.rejectedCLI("permission_denied", "node", "put", "peer", input, path)
	}
	if _, err := os.Stat(filepath.Join(outside, "sentinel")); !os.IsNotExist(err) {
		c.t.Fatalf("rejected transfer wrote outside the node file root: %v", err)
	}
}
