//go:build linux

package hub

import (
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"connectrpc.com/connect"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
)

func TestSenderOf(t *testing.T) {
	tests := []struct {
		name string
		p    *auth.Principal
		want string
	}{
		{"no principal", nil, "owner"},
		{"owner", &auth.Principal{Owner: true, Name: "owner"}, "owner"},
		{"sleeve", &auth.Principal{Name: "sleeve:api"}, "api"},
		{"orchestrator", &auth.Principal{Name: "orchestrator-envoy"}, "envoy"},
		{"other token", &auth.Principal{Name: "pair: phone"}, "token:pair: phone"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := senderOf(tt.p); got != tt.want {
				t.Fatalf("senderOf = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestLocalURL(t *testing.T) {
	tests := []struct{ addr, want string }{
		{"127.0.0.1:7470", "http://127.0.0.1:7470"},
		{"0.0.0.0:9000", "http://127.0.0.1:9000"},
		{"[::]:9000", "http://127.0.0.1:9000"},
		{"10.0.0.5:80", "http://10.0.0.5:80"},
	}
	for _, tt := range tests {
		t.Run(tt.addr, func(t *testing.T) {
			a, err := net.ResolveTCPAddr("tcp", tt.addr)
			if err != nil {
				t.Fatal(err)
			}
			if got := LocalURL(a); got != tt.want {
				t.Fatalf("LocalURL = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestSleeveIdentity(t *testing.T) {
	r := newChatRig(t)
	r.h.SelfURL = "http://127.0.0.1:9999"
	r.spawn(pb.SleeveMode_SLEEVE_MODE_CHAT)

	tokenPath := filepath.Join(r.h.sleeves.HomeDir(r.name), sleeveTokenFile)
	fi, err := os.Stat(tokenPath)
	if err != nil {
		t.Fatal(err)
	}
	if fi.Mode().Perm() != 0o600 {
		t.Fatalf("token file mode %v", fi.Mode())
	}
	if _, err := os.Stat(filepath.Join(r.ws, sleeveTokenFile)); err == nil {
		t.Fatal("token written into the workspace")
	}
	raw, _ := os.ReadFile(tokenPath)
	secret := strings.TrimSpace(string(raw))
	p, err := r.h.authz.Authenticate(secret)
	if err != nil {
		t.Fatal(err)
	}
	if senderOf(p) != r.name {
		t.Fatalf("token maps to %q, want %q", senderOf(p), r.name)
	}
	for tool, want := range map[string]bool{
		"sleeve_kill_sleeve": false, "token_create_token": false, "chat_send": false,
		"sleeve_list_sleeves": true, "backlog_create_note": true, "lease_acquire_lease": true, "lease_list_leases": true,
	} {
		if got := auth.Grants(p.Scopes, tool); got != want {
			t.Errorf("Grants(%s) = %v, want %v", tool, got, want)
		}
	}

	env, err := os.ReadFile(filepath.Join(filepath.Dir(r.h.sleeves.HomeDir(r.name)), "env.args"))
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"PROTECTORATE_HUB\x00http://127.0.0.1:9999", "PROTECTORATE_TOKEN_FILE\x00" + filepath.Join(r.h.sleeves.UserHome(), sleeveTokenFile)} {
		if !strings.Contains(string(env), want) {
			t.Errorf("sleeve env lacks %q", strings.ReplaceAll(want, "\x00", "="))
		}
	}
	if strings.Contains(string(env), secret) {
		t.Error("the secret itself is in the sleeve env")
	}

	if _, err := r.sl.KillSleeve(r.ctx, connect.NewRequest(&pb.KillSleeveRequest{Name: r.name})); err != nil {
		t.Fatal(err)
	}
	if _, err := r.h.authz.Authenticate(secret); err == nil {
		t.Fatal("token still valid after kill")
	}

	// a respawn mints a new token and the old one stays dead
	r.spawn(pb.SleeveMode_SLEEVE_MODE_CHAT)
	raw2, _ := os.ReadFile(tokenPath)
	if strings.TrimSpace(string(raw2)) == secret {
		t.Fatal("respawn reused the token")
	}
	if _, err := r.h.authz.Authenticate(strings.TrimSpace(string(raw2))); err != nil {
		t.Fatal(err)
	}
	live := 0
	for _, tk := range r.h.authz.Store().List() {
		if tk.Name == sleeveTokenName(r.name) && tk.RevokedAt == nil {
			live++
		}
	}
	if live != 1 {
		t.Fatalf("%d live tokens for the sleeve, want 1", live)
	}
}
