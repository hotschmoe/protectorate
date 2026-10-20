package tailnet_test

import (
	"context"
	"io"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/hotschmoe/protectorate/internal/backup"
	"github.com/hotschmoe/protectorate/internal/tailnet"
)

// TestBackupRestoreWithLiveHeadscale backs up a home while headscale runs and
// has a node enrolled, restores it into a new home and starts headscale there:
// the enrolled node and the DERP certificate that nodes pin must be back.
func TestBackupRestoreWithLiveHeadscale(t *testing.T) {
	if os.Getenv("PROT_TAILNET_IT") != "1" {
		t.Skip("set PROT_TAILNET_IT=1 to run (downloads headscale, starts real processes)")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()

	home := t.TempDir()
	bin, err := tailnet.Install(ctx, home, tailnet.HeadscaleVersion, nil)
	if err != nil {
		t.Fatal(err)
	}
	cfg := testConfig(t)
	if err := cfg.Save(home); err != nil {
		t.Fatal(err)
	}
	svc, err := tailnet.Start(ctx, home, cfg, t.Logf)
	if err != nil {
		t.Fatalf("start: %v", err)
	}
	stopped := false
	defer func() {
		if !stopped {
			svc.Close()
		}
	}()
	enroll(ctx, t, svc.Admin, cfg.ServerURL, "alpha", []string{tailnet.TagNode, tailnet.TierTag("shared")}, false)

	file, err := backup.Create(ctx, backup.Options{Home: home, Dir: t.TempDir(), Version: "it"})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.Admin.WaitReady(ctx); err != nil {
		t.Fatalf("headscale did not resume after backup: %v", err)
	}
	cert, err := os.ReadFile(filepath.Join(home, "tailnet", "derp_cert.pem"))
	if err != nil {
		t.Fatal(err)
	}
	if nodes, err := svc.Admin.Nodes(ctx); err != nil || len(nodes) < 2 {
		t.Fatalf("live headscale lost its nodes during the backup: %v %v", nodes, err)
	}
	svc.Close()
	stopped = true

	restored := filepath.Join(t.TempDir(), "home")
	if _, err := backup.Restore(ctx, file, restored); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(restored, "bin"), 0o700); err != nil {
		t.Fatal(err)
	}
	in, err := os.Open(bin)
	if err != nil {
		t.Fatal(err)
	}
	defer in.Close()
	out, err := os.OpenFile(filepath.Join(restored, "bin", filepath.Base(bin)), os.O_CREATE|os.O_WRONLY|os.O_EXCL, 0o755)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := io.Copy(out, in); err != nil {
		t.Fatal(err)
	}
	if err := out.Close(); err != nil {
		t.Fatal(err)
	}

	svc2, err := tailnet.Start(ctx, restored, cfg, t.Logf)
	if err != nil {
		t.Fatalf("start from the restored home: %v", err)
	}
	defer svc2.Close()
	nodes, err := svc2.Admin.Nodes(ctx)
	if err != nil {
		t.Fatal(err)
	}
	found := false
	for _, n := range nodes {
		found = found || n.Name == "alpha" || n.GivenName == "alpha"
	}
	if !found {
		t.Fatalf("restored headscale does not know node alpha: %+v", nodes)
	}
	if got, _ := os.ReadFile(filepath.Join(restored, "tailnet", "derp_cert.pem")); string(got) != string(cert) {
		t.Fatal("the DERP certificate that nodes pin changed across the restore")
	}
}
