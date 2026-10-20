//go:build linux

package hub

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/types/known/durationpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/handoff"
	"github.com/hotschmoe/protectorate/internal/sleeve"
)

// fakePi speaks enough of `pi --mode rpc` to answer a prompt with a summary
// from a local endpoint, and logs each prompt into the workspace.
const fakePi = `#!/bin/sh
while read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"get_state"'*)
      printf '%s\n' '{"type":"response","id":"'$id'","command":"get_state","success":true,"data":{"sessionId":"pi-s1","model":{"id":"dd","provider":"hotschmoe-local","baseUrl":"http://127.0.0.1:9/v1","contextWindow":1000}}}';;
    *'"prompt"'*)
      printf '%s\n' "$line" >> "$PWD/pi-prompts.log"
      printf '%s\n' '{"type":"response","id":"'$id'","command":"prompt","success":true}'
      printf '%s\n' '{"type":"message_start","message":{"role":"assistant"}}'
      printf '%s\n' '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"## Goal\n- answer ping\n## Next\n- keep going"}}'
      printf '%s\n' '{"type":"message_end","message":{"role":"assistant","provider":"hotschmoe-local","model":"dd","usage":{"input":5,"output":5,"totalTokens":10}}}'
      printf '%s\n' '{"type":"agent_settled"}';;
  esac
done
`

func newResleeveRig(t *testing.T) *chatRig {
	t.Helper()
	r := newChatRig(t)
	claude, err := exec.LookPath("claude")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(filepath.Dir(claude), "pi"), []byte(fakePi), 0o755); err != nil {
		t.Fatal(err)
	}
	for _, args := range [][]string{{"init", "-q"}, {"add", "."}, {"commit", "-qm", "first"}} {
		if args[0] == "add" {
			if err := os.WriteFile(filepath.Join(r.ws, "lib.go"), []byte("package lib\n"), 0o644); err != nil {
				t.Fatal(err)
			}
		}
		cmd := exec.Command("git", append([]string{"-C", r.ws}, args...)...)
		cmd.Env = append(os.Environ(), "GIT_CONFIG_GLOBAL=/dev/null", "GIT_AUTHOR_NAME=t", "GIT_AUTHOR_EMAIL=t@x", "GIT_COMMITTER_NAME=t", "GIT_COMMITTER_EMAIL=t@x")
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v %s", args, err, out)
		}
	}
	t.Cleanup(func() {
		exec.Command("systemctl", "--user", "kill", "--signal=SIGKILL", r.cfg.UnitName(summaryName(r.name))+".scope").Run()
	})
	return r
}

func (r *chatRig) resleeve(req *pb.ResleeveSleeveRequest) (*pb.Sleeve, error) {
	req.Name = r.name
	res, err := r.sl.ResleeveSleeve(r.ctx, connect.NewRequest(req))
	if err != nil {
		return nil, err
	}
	return res.Msg.Sleeve, nil
}

func TestResleeveHandsOffToANewHarness(t *testing.T) {
	tests := []struct {
		name      string
		prompt    string
		interrupt bool
	}{
		{"idle chat", "ping", false},
		{"running turn is interrupted", "slow", true},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			r := newResleeveRig(t)
			events, stop := r.h.events.subscribe()
			defer stop()
			r.spawn(pb.SleeveMode_SLEEVE_MODE_CHAT)
			if _, err := r.send("ping", ""); err != nil {
				t.Fatal(err)
			}
			r.waitState(pb.ChatState_CHAT_STATE_IDLE)
			oldSID := r.get().Chat.HarnessSessionId
			if tc.prompt != "ping" {
				if _, err := r.send(tc.prompt, ""); err != nil {
					t.Fatal(err)
				}
				r.waitState(pb.ChatState_CHAT_STATE_WORKING)
			}
			before := r.get().Chat.LastSeq
			old := r.watch(0)

			s, err := r.resleeve(&pb.ResleeveSleeveRequest{To: pb.Harness_HARNESS_PI, Interrupt: tc.interrupt})
			if err != nil {
				t.Fatal(err)
			}
			if s.Harness != pb.Harness_HARNESS_PI || s.Workspace != r.ws || s.Limits.GetMemoryBytes() != 512<<20 {
				t.Fatalf("new sleeve = %v", s)
			}
			h := s.Handoff
			if h.GetFromHarness() != "claude" || h.ToHarness != "pi" || h.FromSessionId != oldSID ||
				h.SummarizerHarness != "pi" || !h.SummarizerLocal || h.SummarizerModel != "dd" || h.SummaryError != "" {
				t.Fatalf("handoff = %v", h)
			}
			if h.Path != filepath.Join(r.ws, handoff.File) {
				t.Fatalf("handoff path %s", h.Path)
			}
			doc, err := os.ReadFile(h.Path)
			if err != nil {
				t.Fatal(err)
			}
			for _, want := range []string{"Summarized by a local model", "### Goal", "- answer ping", "## Facts", "**Assistant, final message:**", "> pong", "please answer ping", "first", oldSID} {
				if !strings.Contains(string(doc), want) {
					t.Errorf("HANDOFF.md lacks %q:\n%s", want, doc)
				}
			}
			if archived, err := os.ReadFile(h.ArchivePath); err != nil || string(archived) != string(doc) || !strings.HasPrefix(h.ArchivePath, r.cfg.Home) {
				t.Fatalf("archive %s: %v", h.ArchivePath, err)
			}
			st, err := exec.Command("git", "-C", r.ws, "status", "--porcelain", "--untracked-files=all").CombinedOutput()
			if err != nil || strings.Contains(string(st), ".protectorate") {
				t.Fatalf("git sees the handoff: %s %v", st, err)
			}
			summaryPrompt, _ := os.ReadFile(filepath.Join(r.cfg.Home, "workspaces", summaryName(r.name), "pi-prompts.log"))
			if !strings.Contains(string(summaryPrompt), "pong") || !strings.Contains(string(summaryPrompt), "## Gotchas") {
				t.Fatalf("summary prompt = %s", summaryPrompt)
			}
			waitFor(t, "first prompt to the new harness", func() bool {
				p, _ := os.ReadFile(filepath.Join(r.ws, "pi-prompts.log"))
				return strings.Contains(string(p), "Read "+handoff.File+" first")
			})
			if err := <-old.done; err != nil {
				t.Fatalf("watch of the outgoing harness ended with %v", err)
			}
			live := r.watch(before)
			waitFor(t, "handoff and hub prompt in the continued log", func() bool {
				d := describe(live.all())
				return strings.Contains(d, ":other") && strings.Contains(d, ":user:hub:Continue the task")
			})
			for _, e := range live.all() {
				if e.GetHandoff() != nil && e.Seq <= before {
					t.Fatalf("handoff seq %d does not continue the log after %d", e.Seq, before)
				}
			}
			list, err := r.sl.ListSleeves(r.ctx, connect.NewRequest(&pb.ListSleevesRequest{}))
			if err != nil || len(list.Msg.Sleeves) != 1 {
				t.Fatalf("sleeves after resleeve: %v %v", list.Msg.GetSleeves(), err)
			}
			if _, err := r.h.sleeves.Get(summaryName(r.name)); err == nil {
				t.Fatal("the summary sleeve was left behind")
			}
			kinds := map[string]bool{}
			for drained := false; !drained; {
				select {
				case e := <-events:
					kinds[e.Kind] = true
				default:
					drained = true
				}
			}
			for _, k := range []string{"resleeve.started", "sleeve.resleeved", "resleeve.warning"} {
				if !kinds[k] {
					t.Errorf("no %s event: %v", k, kinds)
				}
			}
		})
	}
}

func TestResleeveRefuses(t *testing.T) {
	r := newResleeveRig(t)
	r.spawn(pb.SleeveMode_SLEEVE_MODE_CHAT)
	if _, err := r.send("ping", ""); err != nil {
		t.Fatal(err)
	}
	r.waitState(pb.ChatState_CHAT_STATE_IDLE)
	sid := r.get().Chat.HarnessSessionId

	cs, err := r.h.chat.get(r.name, false)
	if err != nil {
		t.Fatal(err)
	}
	if !cs.takeOver() {
		t.Fatal("take over")
	}
	tests := []struct {
		name string
		call func() error
		want connect.Code
	}{
		{"send while the hub owns the chat", func() error { _, err := r.send("hi", ""); return err }, connect.CodeFailedPrecondition},
		{"compact while the hub owns the chat", func() error {
			_, err := r.chat.Compact(r.ctx, connect.NewRequest(&pb.CompactRequest{Sleeve: r.name}))
			return err
		}, connect.CodeFailedPrecondition},
		{"a second resleeve", func() error { _, err := r.resleeve(&pb.ResleeveSleeveRequest{To: pb.Harness_HARNESS_PI}); return err }, connect.CodeFailedPrecondition},
		{"onto shell", func() error {
			_, err := r.resleeve(&pb.ResleeveSleeveRequest{To: pb.Harness_HARNESS_SHELL})
			return err
		}, connect.CodeInvalidArgument},
		{"missing sleeve", func() error {
			_, err := r.sl.ResleeveSleeve(r.ctx, connect.NewRequest(&pb.ResleeveSleeveRequest{Name: "nosuch"}))
			return err
		}, connect.CodeNotFound},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if err := tc.call(); code(err) != tc.want {
				t.Fatalf("got %v, want %v", err, tc.want)
			}
		})
	}
	cs.giveBack()

	// a harness that cannot start: the old one comes back on its own session
	_, err = r.resleeve(&pb.ResleeveSleeveRequest{To: pb.Harness_HARNESS_GROK, Wait: durationpb.New(time.Second)})
	if code(err) != connect.CodeFailedPrecondition || !strings.Contains(err.Error(), "back on its own session") {
		t.Fatalf("resleeve onto a missing harness: %v", err)
	}
	s := r.get()
	if s.Harness != pb.Harness_HARNESS_CLAUDE || s.Handoff != nil {
		t.Fatalf("after a failed resleeve: %v", s)
	}
	waitFor(t, "claude resumed", func() bool {
		args, _ := os.ReadFile(filepath.Join(r.ws, "args.log"))
		lines := strings.Split(strings.TrimSpace(string(args)), "\n")
		return strings.Contains(lines[len(lines)-1], "--resume "+sid)
	})
}

func TestResleeveTarget(t *testing.T) {
	h := New("test", testToken)
	tests := []struct {
		name string
		from sleeve.Harness
		to   pb.Harness
		want sleeve.Harness
		code connect.Code
	}{
		{"explicit", sleeve.Claude, pb.Harness_HARNESS_CODEX, sleeve.Codex, 0},
		{"same harness, fresh session", sleeve.Claude, pb.Harness_HARNESS_CLAUDE, sleeve.Claude, 0},
		{"auto without quota is the local LLM", sleeve.Claude, pb.Harness_HARNESS_UNSPECIFIED, sleeve.Pi, 0},
		{"auto from pi without quota", sleeve.Pi, pb.Harness_HARNESS_UNSPECIFIED, "", connect.CodeFailedPrecondition},
		{"shell", sleeve.Claude, pb.Harness_HARNESS_SHELL, "", connect.CodeInvalidArgument},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got, err := h.resleeveTarget(tc.from, tc.to)
			if code(err) != tc.code || got != tc.want {
				t.Fatalf("got %q %v, want %q %v", got, err, tc.want, tc.code)
			}
		})
	}
	if got := h.summarizers(sleeve.Claude); len(got) != 1 || got[0] != sleeve.Pi {
		t.Fatalf("summarizers without quota = %v", got)
	}
}
