package hub

import (
	"context"
	"errors"
	"fmt"
	"log"
	"os/exec"
	"strings"
	"time"

	"github.com/hotschmoe/protectorate/internal/directory"
	"github.com/hotschmoe/protectorate/internal/mailbox"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

type baseChange struct {
	tip         string
	first, last time.Time
}

func (c *coordinator) checkBases(ctx context.Context) {
	if c.bases == nil {
		c.bases = map[string]baseChange{}
	}
	live := map[string]bool{}
	for _, info := range c.h.sleeves.List() {
		if ctx.Err() != nil {
			return
		}
		if info.State != sleeve.Running || info.BaseRepo == "" {
			continue
		}
		live[info.Name] = true
		tip, err := sleeve.BranchTip(info.BaseRepo, info.BaseBranch)
		if err != nil || tip == info.BaseTip {
			delete(c.bases, info.Name)
			continue
		}
		now := c.now()
		ch := c.bases[info.Name]
		if ch.tip != tip {
			if ch.first.IsZero() {
				ch.first = now
			}
			ch.tip, ch.last = tip, now
			c.bases[info.Name] = ch
		}
		if now.Sub(ch.last) < 2*time.Second && now.Sub(ch.first) < 10*time.Second {
			continue
		}
		// Ref polling is file-only; history is inspected in the existing read-only jail.
		if _, err := c.h.sleeves.Git(ctx, info.Name, info.BaseRepo, "merge-base", "--is-ancestor", info.BaseTip, tip); err != nil {
			var exit *exec.ExitError
			if errors.As(err, &exit) && exit.ExitCode() == 1 {
				if err := c.h.sleeves.SetBaseTip(info.Name, info.BaseTip, tip); err != nil {
					log.Printf("base notice %s: remember changed history: %v", info.Name, err)
				} else {
					delete(c.bases, info.Name)
				}
			}
			continue
		}
		_, own := c.h.sleeves.Git(ctx, info.Name, info.Workspace, "merge-base", "--is-ancestor", tip, "HEAD")
		if own != nil {
			out, err := c.h.sleeves.Git(ctx, info.Name, info.BaseRepo, "-c", "log.showSignature=false", "log", "--format=%h %s", "-10", info.BaseTip+".."+tip, "--")
			if err != nil {
				log.Printf("base notice %s: describe commits: %v", info.Name, err)
				continue
			}
			lines := strings.Split(strings.TrimSpace(out), "\n")
			for n, line := range lines {
				lines[n] = firstLine(line, 160)
			}
			ref := "base/" + info.BaseBranch
			if strings.Trim(ref, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_./-") != "" {
				ref = shQuote(ref)
			}
			body := fmt.Sprintf("Base branch %s advanced %s..%s.\n%s\nUpdate with: git fetch base && git rebase %s\nNo reply needed.", info.BaseBranch, directory.Short(info.BaseTip), directory.Short(tip), strings.Join(lines, "\n"), ref)
			if _, err := c.send(hubName, sendArgs{To: info.Name, Body: body, Kind: mailbox.Notice, RequestID: depRequestID(info.BaseRepo, info.BaseTip+":"+tip, info.Name)}); err != nil {
				log.Printf("base notice %s: %v", info.Name, err)
				continue
			}
		}
		if err := c.h.sleeves.SetBaseTip(info.Name, info.BaseTip, tip); err != nil {
			log.Printf("base notice %s: %v", info.Name, err)
			continue
		}
		delete(c.bases, info.Name)
	}
	for name := range c.bases {
		if !live[name] {
			delete(c.bases, name)
		}
	}
}
