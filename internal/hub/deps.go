package hub

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"log"
	"reflect"
	"strings"

	"google.golang.org/protobuf/types/known/structpb"

	"github.com/hotschmoe/protectorate/internal/backlog"
	"github.com/hotschmoe/protectorate/internal/directory"
	"github.com/hotschmoe/protectorate/internal/mailbox"
)

// checkReleases compares each project's tags and default branch with what was
// last seen and, on a new tag or an advance of the default branch, tells every
// dependent project the facts: a notice message where a sleeve works on it, a
// note where none does. It reads local git only.
func (c *coordinator) checkReleases(ctx context.Context) {
	for _, v := range c.dir.List() {
		if ctx.Err() != nil {
			return
		}
		ws, now, ok := c.readRefs(ctx, v)
		if !ok {
			continue
		}
		anc := directory.IsAncestor(ctx, ws)
		if v.Refs == nil {
			c.saveRefs(v.ID, now)
			continue
		}
		for _, ch := range directory.Changes(*v.Refs, now, anc) {
			c.announce(ctx, v, ws, ch)
		}
		if merged := directory.Merge(*v.Refs, now, anc); !reflect.DeepEqual(merged, *v.Refs) {
			c.saveRefs(v.ID, merged)
		}
	}
}

// readRefs reads the refs of the first owner workspace that is a git repo.
func (c *coordinator) readRefs(ctx context.Context, v directory.View) (string, directory.RefState, bool) {
	for _, o := range v.Owners {
		if r, err := directory.ReadRefs(ctx, o.Path); err == nil {
			return o.Path, r, true
		}
	}
	return "", directory.RefState{}, false
}

func (c *coordinator) saveRefs(id string, r directory.RefState) {
	if err := c.dir.SetRefs(id, r); err != nil {
		log.Printf("coordination: save refs of %s: %v", id, err)
	}
}

// announce tells the dependents of v about one change.
func (c *coordinator) announce(ctx context.Context, v directory.View, ws string, ch directory.Change) {
	facts, err := directory.Describe(ctx, ws, ch.From, ch.To)
	if err != nil {
		log.Printf("coordination: describe %s %s: %v", v.ID, ch.Name, err)
	}
	ref := ch.To
	if ch.Tag != "" {
		ref = ch.Tag
	}
	key := ch.Kind + ":" + ref + ":" + ch.To
	c.publishRelease(v, ch, len(v.Dependents))
	for _, e := range v.Dependents {
		d, err := c.dir.Get(e.Project)
		if err != nil {
			continue
		}
		requests := c.mail.OpenRequests(v.ID, d.ID)
		body := releaseBody(v, d, ch, e.Via, facts, requests)
		if len(liveOwners(d, "")) > 0 {
			_, err := c.send(hubName, sendArgs{
				To: projectPrefix + d.ID, Body: body, Kind: mailbox.Notice, RequestID: depRequestID(v.ID, key, d.ID),
				ReplyVia: c.projectAddress(v),
			})
			if err != nil {
				log.Printf("coordination: notify %s of %s: %v", d.ID, key, err)
			}
			continue
		}
		c.fileRelease(v, d, ch, e.Via, body)
	}
}

// projectAddress is the shortest address of a project: its display name
// when that resolves to it alone, else its id (a path:<dir> id is long).
func (c *coordinator) projectAddress(v directory.View) string {
	if got, err := c.dir.Get(v.Display); err == nil && got.ID == v.ID {
		return projectPrefix + v.Display
	}
	return projectPrefix + v.ID
}

func depRequestID(upstream, key, downstream string) string {
	sum := sha256.Sum256([]byte(upstream + "\x00" + key + "\x00" + downstream))
	return "dep-" + hex.EncodeToString(sum[:])[:32]
}

func (c *coordinator) publishRelease(v directory.View, ch directory.Change, dependents int) {
	payload, err := structpb.NewStruct(map[string]any{
		"kind": ch.Kind, "name": ch.Name, "from": ch.From, "to": ch.To, "dependents": float64(dependents),
	})
	if err == nil {
		c.h.emit("project.released", v.ID, payload)
	}
}

// fileRelease files a note for a dependent nobody works on, once.
func (c *coordinator) fileRelease(up, down directory.View, ch directory.Change, via []string, body string) {
	if c.h.backlog == nil {
		return
	}
	title := fmt.Sprintf("%s %s: review changes (uses %s)", up.Display, releaseHeadline(ch), strings.Join(via, ", "))
	existing, err := c.h.backlog.List(backlog.Filter{Project: down.Display})
	if err != nil {
		log.Printf("coordination: read notes: %v", err)
		return
	}
	for _, n := range existing {
		if n.Title == title {
			return
		}
	}
	if _, err := c.h.fileNote(backlog.NewNote{Title: title, Body: body, Project: down.Display, Source: hubName}); err != nil {
		log.Printf("coordination: file note for %s: %v", down.ID, err)
	}
}

func releaseHeadline(ch directory.Change) string {
	if ch.Kind == "tag" {
		return fmt.Sprintf("released %s", ch.Tag)
	}
	return fmt.Sprintf("default branch %s advanced to %s", ch.Name, directory.Short(ch.To))
}

// releaseBody states the facts of a change for one dependent. It makes no
// judgement about whether the change matters.
func releaseBody(up, down directory.View, ch directory.Change, via []string, f directory.Facts, requests []mailbox.Message) string {
	var b strings.Builder
	switch ch.Kind {
	case "tag":
		fmt.Fprintf(&b, "Upstream release: %s tag %s (%s)", up.Display, ch.Tag, directory.Short(ch.To))
		if ch.FromName != "" {
			fmt.Fprintf(&b, ", previous tag %s (%s)", ch.FromName, directory.Short(ch.From))
		} else if ch.From != "" {
			fmt.Fprintf(&b, ", previous default branch tip %s", directory.Short(ch.From))
		}
	default:
		fmt.Fprintf(&b, "Upstream change: %s default branch %s advanced %s -> %s", up.Display, ch.Name, directory.Short(ch.From), directory.Short(ch.To))
	}
	fmt.Fprintf(&b, ".\n%s depends on it through: %s.\n", down.Display, strings.Join(via, ", "))
	if s := f.String(); s != "" {
		b.WriteString("\n" + s)
	}
	if len(requests) > 0 {
		fmt.Fprintf(&b, "\nYour open requests to %s:\n", up.Display)
		for _, r := range requests {
			fmt.Fprintf(&b, "  thread %s, %s: %s\n", r.Thread, r.ID, firstLine(r.Body, 100))
		}
		b.WriteString("Close one with: protectorate msg ack <id>\n")
	}
	return strings.TrimRight(b.String(), "\n")
}
