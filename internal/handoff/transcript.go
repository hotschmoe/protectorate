package handoff

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path"
	"regexp"
	"strings"
)

// Turn is one user or assistant message of a native transcript.
type Turn struct {
	Role string // "user" or "assistant"
	Text string
}

// Transcript is what the outgoing harness's own session store holds.
type Transcript struct {
	// Path is relative to the sleeve home the transcript was read from.
	Path  string
	Turns []Turn
}

// maxTranscriptRead bounds how much of a transcript is read: its tail, since
// the newest turns matter most.
const maxTranscriptRead = 16 << 20

var (
	sessionIDRE = regexp.MustCompile(`^[A-Za-z0-9._-]{1,128}$`)
	// a message that is wholly wrapped in tags is context the harness added
	// (environment, command echoes, skills), not something a person typed
	wrappedRE = regexp.MustCompile(`(?s)^\s*<([a-z][a-z0-9_-]*)>.*</[a-z][a-z0-9_-]*>\s*$`)
	grokQuery = regexp.MustCompile(`(?s)^\s*<user_query>\s*(.*?)\s*</user_query>\s*$`)
)

// ReadTranscript finds the native transcript of sessionID under home (the
// sleeve's home directory, where each harness keeps its sessions) and returns
// its user and assistant messages in order. Everything is read through an
// os.Root, so a symlink a sandboxed agent planted in its home cannot make the
// hub read a file outside it.
func ReadTranscript(home, harness, sessionID string) (Transcript, error) {
	if !sessionIDRE.MatchString(sessionID) {
		return Transcript{}, fmt.Errorf("transcript: invalid session id %q", sessionID)
	}
	root, err := os.OpenRoot(home)
	if err != nil {
		return Transcript{}, fmt.Errorf("transcript: open sleeve home: %w", err)
	}
	defer root.Close()
	p, err := locate(root.FS(), harness, sessionID)
	if err != nil {
		return Transcript{}, err
	}
	f, err := root.Open(p)
	if err != nil {
		return Transcript{}, fmt.Errorf("transcript: %w", err)
	}
	defer f.Close()
	fi, err := f.Stat()
	if err != nil {
		return Transcript{}, fmt.Errorf("transcript: %w", err)
	}
	if !fi.Mode().IsRegular() {
		return Transcript{}, fmt.Errorf("transcript: %s is not a regular file", p)
	}
	partial := false
	if fi.Size() > maxTranscriptRead {
		if _, err := f.Seek(fi.Size()-maxTranscriptRead, io.SeekStart); err != nil {
			return Transcript{}, fmt.Errorf("transcript: %w", err)
		}
		partial = true
	}
	turns, err := parse(harness, io.LimitReader(f, maxTranscriptRead), partial)
	if err != nil {
		return Transcript{}, fmt.Errorf("transcript %s: %w", p, err)
	}
	return Transcript{Path: p, Turns: turns}, nil
}

// locate returns the transcript path of a session, relative to the home.
//
//	claude  .claude/projects/<cwd>/<id>.jsonl
//	codex   .codex/sessions/YYYY/MM/DD/rollout-<time>-<id>.jsonl
//	grok    .grok/sessions/<cwd>/<id>/chat_history.jsonl
//	pi      .pi/sessions/<time>_<id>.jsonl
func locate(fsys fs.FS, harness, id string) (string, error) {
	var patterns []string
	switch harness {
	case "claude":
		patterns = []string{".claude/projects/*/" + id + ".jsonl"}
	case "codex":
		patterns = []string{".codex/sessions/*/*/*/rollout-*-" + id + ".jsonl"}
	case "grok":
		patterns = []string{".grok/sessions/*/" + id + "/chat_history.jsonl"}
	case "pi":
		patterns = []string{".pi/sessions/*_" + id + ".jsonl", ".pi/sessions/*/*_" + id + ".jsonl"}
	default:
		return "", fmt.Errorf("transcript: no native transcript for harness %q", harness)
	}
	for _, pat := range patterns {
		m, err := fs.Glob(fsys, pat)
		if err != nil {
			return "", fmt.Errorf("transcript: %w", err)
		}
		if len(m) > 0 {
			return m[len(m)-1], nil
		}
	}
	return "", fmt.Errorf("transcript: no %s session %s under %s", harness, id, path.Dir(patterns[0]))
}

type line struct {
	Type    string `json:"type"`
	Meta    bool   `json:"isMeta"`
	Summary bool   `json:"isCompactSummary"`
	Side    bool   `json:"isSidechain"`
	// claude and pi
	Message *struct {
		ID      string          `json:"id"`
		Role    string          `json:"role"`
		Content json.RawMessage `json:"content"`
	} `json:"message"`
	// codex
	Payload *struct {
		Type    string          `json:"type"`
		Role    string          `json:"role"`
		Content json.RawMessage `json:"content"`
	} `json:"payload"`
	// grok
	Content   json.RawMessage `json:"content"`
	Prompt    *int            `json:"prompt_index"`
	Synthetic string          `json:"synthetic_reason"`
}

func parse(harness string, r io.Reader, partial bool) ([]Turn, error) {
	br := bufio.NewReaderSize(r, 64<<10)
	var turns []Turn
	lastID := ""
	first := true
	for {
		raw, err := br.ReadBytes('\n')
		if len(raw) > 0 && !(first && partial) {
			if t, id, ok := parseLine(harness, bytes.TrimSpace(raw)); ok {
				if n := len(turns); n > 0 && t.Role == "assistant" && id != "" && id == lastID && turns[n-1].Role == "assistant" {
					turns[n-1].Text += "\n" + t.Text
				} else {
					turns = append(turns, t)
				}
				lastID = id
			}
		}
		first = false
		if errors.Is(err, io.EOF) {
			return turns, nil
		}
		if err != nil {
			return nil, err
		}
	}
}

// parseLine returns the message a transcript line holds, if it is a user or
// assistant message a person would see, and the harness's message id.
func parseLine(harness string, raw []byte) (Turn, string, bool) {
	var l line
	if len(raw) == 0 || json.Unmarshal(raw, &l) != nil {
		return Turn{}, "", false
	}
	var role, id string
	var content json.RawMessage
	switch harness {
	case "claude":
		if (l.Type != "user" && l.Type != "assistant") || l.Message == nil || l.Meta || l.Summary || l.Side {
			return Turn{}, "", false
		}
		role, id, content = l.Message.Role, l.Message.ID, l.Message.Content
	case "pi":
		if l.Type != "message" || l.Message == nil {
			return Turn{}, "", false
		}
		role, content = l.Message.Role, l.Message.Content
	case "codex":
		if l.Type != "response_item" || l.Payload == nil || l.Payload.Type != "message" {
			return Turn{}, "", false
		}
		role, content = l.Payload.Role, l.Payload.Content
	case "grok":
		if (l.Type == "user" && (l.Prompt == nil || l.Synthetic != "")) || (l.Type != "user" && l.Type != "assistant") {
			return Turn{}, "", false
		}
		role, content = l.Type, l.Content
	}
	if role != "user" && role != "assistant" {
		return Turn{}, "", false
	}
	text, tool := contentText(content)
	text = strings.TrimSpace(text)
	if harness == "grok" && role == "user" {
		if m := grokQuery.FindStringSubmatch(text); m != nil {
			text = m[1]
		}
	}
	if text == "" || (role == "user" && (tool || wrappedRE.MatchString(text))) {
		return Turn{}, "", false
	}
	return Turn{Role: role, Text: text}, id, true
}

// contentText joins the text parts of a message's content, which is a plain
// string or a list of typed parts. tool reports a tool result among them.
func contentText(raw json.RawMessage) (text string, tool bool) {
	var s string
	if json.Unmarshal(raw, &s) == nil {
		return s, false
	}
	var parts []struct {
		Type string `json:"type"`
		Text string `json:"text"`
	}
	if json.Unmarshal(raw, &parts) != nil {
		return "", false
	}
	var out []string
	for _, p := range parts {
		switch p.Type {
		case "text", "input_text", "output_text":
			if p.Text != "" {
				out = append(out, p.Text)
			}
		case "tool_result", "toolResult", "function_call_output":
			tool = true
		}
	}
	return strings.Join(out, "\n"), tool
}
