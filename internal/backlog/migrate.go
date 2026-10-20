package backlog

import (
	"fmt"
	"regexp"
	"strings"
	"time"
)

// The pre-notes backlog was a markdown file of "## project" headings with
// "- [ ] title  #p2 @codex id:ab12" items. convertLegacy reads it once, when
// a hub first loads notes, and turns each item into a note that keeps its id.

var (
	legacyHeadingRe = regexp.MustCompile(`^##[ \t]+(\S.*?)[ \t]*$`)
	legacyItemRe    = regexp.MustCompile(`^[-*] \[([ xX~])\] +`)
	legacyTokenRe   = regexp.MustCompile(`\S+`)
	legacyPrioRe    = regexp.MustCompile(`^#p[1-9]$`)
	legacyHarnessRe = regexp.MustCompile(`^@[A-Za-z0-9_-]+$`)
	legacyIDRe      = regexp.MustCompile(`^id:[A-Za-z0-9_-]+$`)
	legacyClaimRe   = regexp.MustCompile(`^claim:[^\s@]+@\S+$`)
	legacyURLRe     = regexp.MustCompile(`^https?://\S+$`)
)

func legacyKind(tok string) string {
	switch {
	case legacyPrioRe.MatchString(tok):
		return "prio"
	case legacyHarnessRe.MatchString(tok):
		return "harness"
	case legacyIDRe.MatchString(tok):
		return "id"
	case legacyClaimRe.MatchString(tok):
		return "claim"
	case legacyURLRe.MatchString(tok):
		return "url"
	}
	return ""
}

func convertLegacy(text string, at time.Time) []Note {
	lines := strings.Split(strings.ReplaceAll(text, "\r\n", "\n"), "\n")
	var notes []Note
	used := map[string]bool{}
	project := ""
	for i := 0; i < len(lines); i++ {
		if m := legacyHeadingRe.FindStringSubmatch(lines[i]); m != nil {
			project = m[1]
			continue
		}
		m := legacyItemRe.FindStringSubmatchIndex(lines[i])
		if m == nil {
			continue
		}
		state := lines[i][m[2]]
		rest := lines[i][m[1]:]
		locs := legacyTokenRe.FindAllStringIndex(rest, -1)
		titleEnd := 0
		if len(locs) > 0 {
			titleEnd = locs[0][0]
		}
		meta := map[string]string{}
		for j := len(locs) - 1; j >= 0; j-- {
			tok := rest[locs[j][0]:locs[j][1]]
			kind := legacyKind(tok)
			if _, dup := meta[kind]; kind == "" || dup {
				titleEnd = locs[j][1]
				break
			}
			meta[kind] = tok
			titleEnd = locs[j][0]
		}
		var body []string
		for i+1 < len(lines) && strings.TrimSpace(lines[i+1]) != "" && (lines[i+1][0] == ' ' || lines[i+1][0] == '\t') {
			i++
			body = append(body, strings.TrimSpace(lines[i]))
		}
		var facts []string
		if u := meta["url"]; u != "" {
			facts = append(facts, "Issue: "+u)
		}
		if p := meta["prio"]; p != "" {
			facts = append(facts, "Priority: "+p[1:])
		}
		if h := meta["harness"]; h != "" {
			facts = append(facts, "Harness: "+h[1:])
		}
		switch {
		case state == 'x' || state == 'X':
			facts = append(facts, "Status: done")
		case state == '~':
			status := "Status: claimed"
			if c := strings.TrimPrefix(meta["claim"], "claim:"); c != "" {
				status += " by " + strings.Replace(c, "@", " at ", 1)
			}
			facts = append(facts, status)
		}
		if len(facts) > 0 && len(body) > 0 {
			body = append(body, "")
		}
		body = append(body, facts...)

		id := strings.TrimPrefix(meta["id"], "id:")
		if id == "" || used[id] {
			id, _ = newID(used)
		}
		used[id] = true
		title := strings.TrimSpace(rest[:titleEnd])
		if title == "" {
			title = meta["url"]
		}
		n := Note{ID: id, Title: title, Body: strings.Join(body, "\n"), Created: at, Updated: at, Project: project, Source: "owner"}
		if n.Title == "" && n.Body == "" {
			n.Title = fmt.Sprintf("untitled item %s", id)
		}
		notes = append(notes, n)
	}
	return notes
}
