//go:build linux

package hub

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/hotschmoe/protectorate/internal/backlog"
	"github.com/hotschmoe/protectorate/internal/directory"
)

func commitIn(t *testing.T, dir, file, content, msg string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(filepath.Join(dir, file)), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, file), []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	gitIn(t, dir, "add", ".")
	gitIn(t, dir, "commit", "-q", "-m", msg)
}

func scanOf(sleeve, ws string) directory.Scan {
	return directory.Scan{Sleeve: sleeve, Workspace: directory.Inspect(context.Background(), ws)}
}

func TestDependencyNotices(t *testing.T) {
	r := newMailRig(t)
	c := r.h.coord

	lib := t.TempDir()
	gitIn(t, lib, "init", "-q")
	gitIn(t, lib, "remote", "add", "origin", "git@github.com:acme/lib.git")
	commitIn(t, lib, "go.mod", "module github.com/acme/lib\n", "init lib")
	gitIn(t, lib, "tag", "v0.1.0")
	api := t.TempDir()
	gitIn(t, api, "init", "-q")
	gitIn(t, api, "remote", "add", "origin", "git@github.com:acme/api.git")
	commitIn(t, api, "go.mod", "module github.com/acme/api\n\nrequire github.com/acme/lib v0.1.0\n", "init api")
	web := t.TempDir()
	gitIn(t, web, "init", "-q")
	gitIn(t, web, "remote", "add", "origin", "git@github.com:acme/web.git")
	commitIn(t, web, "package.json", `{"name":"web","dependencies":{"unrelated":"1"}}`, "init web")

	update := func(withAPI bool) {
		t.Helper()
		scans := []directory.Scan{scanOf("libdev", lib), scanOf("webdev", web)}
		if withAPI {
			scans = append(scans, scanOf("apidev", api))
		}
		if err := c.dir.Update(scans); err != nil {
			t.Fatal(err)
		}
	}
	update(true)

	// the first look only records a baseline
	c.checkReleases(r.ctx)
	if got := c.mail.Thread("t1"); len(got) != 0 {
		t.Fatalf("a baseline produced messages: %v", got)
	}
	v, _ := c.dir.Get("lib")
	if v.Refs == nil || v.Refs.LatestTag() != "v0.1.0" || v.Refs.Default != "main" && v.Refs.Default != "master" {
		t.Fatalf("baseline refs = %+v", v.Refs)
	}

	// apidev has asked lib for something and not been answered
	ask, err := c.send("apidev", sendArgs{To: "project:lib", Body: "please export Foo", Kind: "request"})
	if err != nil {
		t.Fatal(err)
	}

	commitIn(t, lib, "foo.go", "package lib\n", "add Foo")
	commitIn(t, lib, "bar.go", "package lib\n", "add Bar")
	gitIn(t, lib, "tag", "v0.2.0")
	c.checkReleases(r.ctx)

	box, err := c.mail.Inbox("apidev", false, true)
	if err != nil {
		t.Fatal(err)
	}
	if len(box) != 1 || box[0].From != "hub" || box[0].Kind != "notice" || box[0].AckRequired {
		t.Fatalf("apidev's mail = %+v", box)
	}
	body := box[0].Body
	for _, want := range []string{
		"Upstream release: acme/lib tag v0.2.0", "previous tag v0.1.0", "through: go:github.com/acme/lib",
		"Commits (2):", "add Bar", "add Foo", "A foo.go", "A bar.go", "2 files changed",
		"Your open requests to acme/lib:", "thread " + ask.Msg.Thread + ", " + ask.Msg.ID + ": please export Foo",
	} {
		if !strings.Contains(body, want) {
			t.Errorf("notice lacks %q:\n%s", want, body)
		}
	}
	if webBox, _ := c.mail.Inbox("webdev", false, true); len(webBox) != 0 {
		t.Fatalf("a project that does not depend on lib was notified: %v", webBox)
	}
	if libBox, _ := c.mail.Inbox("libdev", false, true); len(libBox) != 1 || libBox[0].Kind != "request" {
		t.Fatalf("libdev's mail = %v", libBox)
	}

	// a second pass has nothing new
	c.checkReleases(r.ctx)
	if box, _ := c.mail.Inbox("apidev", false, true); len(box) != 1 {
		t.Fatalf("the same release was announced twice: %d messages", len(box))
	}

	// the request is answered; a later plain commit on the default branch is a branch notice without it
	if _, _, err := c.mail.Ack(ask.Msg.ID, "libdev"); err != nil {
		t.Fatal(err)
	}
	commitIn(t, lib, "baz.go", "package lib\n", "add Baz")
	c.checkReleases(r.ctx)
	box, _ = c.mail.Inbox("apidev", false, true)
	if len(box) != 2 || !strings.Contains(box[1].Body, "Upstream change: acme/lib default branch") || strings.Contains(box[1].Body, "open requests") {
		t.Fatalf("branch notice = %+v", box)
	}

	// a dependent nobody works on gets a backlog item, once, and no message
	update(false)
	commitIn(t, lib, "qux.go", "package lib\n", "add Qux")
	gitIn(t, lib, "tag", "v0.3.0")
	c.checkReleases(r.ctx)
	c.checkReleases(r.ctx)
	items, err := r.h.backlog.List(backlog.Filter{Project: "acme/api"})
	if err != nil || len(items) != 1 || !strings.Contains(items[0].Title, "acme/lib released v0.3.0: review changes (uses go:github.com/acme/lib)") {
		t.Fatalf("backlog = %v %v", items, err)
	}
	if box, _ := c.mail.Inbox("apidev", false, true); len(box) != 2 {
		t.Fatalf("a sleeve-less dependent got a message: %d", len(box))
	}
	if undelivered := c.mail.Unresolved(); len(undelivered) != 0 {
		t.Fatalf("a backlog-only notice must not wait for a sleeve: %v", undelivered)
	}
}
