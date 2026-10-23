package e2e

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"connectrpc.com/connect"
	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/gen/protectorate/v1/protectoratev1connect"
)

type deviceAuthPeer struct {
	c      *cluster
	client *http.Client
}

func devicePeer(c *cluster, ip string) deviceAuthPeer {
	dialer := &net.Dialer{LocalAddr: &net.TCPAddr{IP: net.ParseIP(ip)}, Timeout: 5 * time.Second}
	transport := &http.Transport{DialContext: dialer.DialContext}
	c.owner.Cleanup(transport.CloseIdleConnections)
	return deviceAuthPeer{c: c, client: &http.Client{Transport: transport, Timeout: 10 * time.Second}}
}

func (p deviceAuthPeer) rpc(method, token string, in map[string]any, want int) map[string]any {
	p.c.t.Helper()
	b, err := json.Marshal(in)
	if err != nil {
		p.c.t.Fatal(err)
	}
	req, err := http.NewRequestWithContext(p.c.ctx, http.MethodPost, p.c.url+"/protectorate.v1.DeviceService/"+method, bytes.NewReader(b))
	if err != nil {
		p.c.t.Fatal(err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Forwarded-For", "192.0.2.123")
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	res, err := p.client.Do(req)
	if err != nil {
		p.c.t.Fatalf("device %s: %v", method, err)
	}
	defer res.Body.Close()
	body, err := io.ReadAll(res.Body)
	if err != nil {
		p.c.t.Fatal(err)
	}
	if res.StatusCode != want {
		p.c.t.Fatalf("device %s: HTTP %d want %d: %s", method, res.StatusCode, want, body)
	}
	var out map[string]any
	if err := json.Unmarshal(body, &out); err != nil {
		p.c.t.Fatalf("decode device %s: %v", method, err)
	}
	return out
}

func deviceClock(t *testing.T, c *cluster) func(time.Duration) {
	t.Helper()
	if c.rust {
		return func(d time.Duration) {
			t.Helper()
			select {
			case <-time.After(d):
			case <-c.ctx.Done():
				t.Fatal(c.ctx.Err())
			}
		}
	}
	c.stop(c.hub)
	path := filepath.Join(c.root, "device-clock")
	now := time.Now().UTC().Truncate(time.Second)
	advance := func(d time.Duration) {
		t.Helper()
		now = now.Add(d)
		if err := os.WriteFile(path, []byte(fmt.Sprint(now.Unix())), 0600); err != nil {
			t.Fatal(err)
		}
	}
	advance(0)
	c.env = append(c.env, "PROT_E2E=1", "PROT_E2E_DEVICE_AUTH_CLOCK_FILE="+path)
	c.boot()
	return advance
}

func TestBinaryDeviceAuthorization(t *testing.T) {
	c := newCluster(t)
	advance := deviceClock(t, c)
	peer := devicePeer(c, "127.0.0.1")
	owner := c.token
	for _, name := range []string{"sleeve:spoof", "orchestrator-envoy"} {
		peer.rpc("StartDeviceAuth", "", map[string]any{"name": name, "platform": "linux-desktop"}, 400)
	}
	start := func(name string) map[string]any {
		t.Helper()
		out := peer.rpc("StartDeviceAuth", "", map[string]any{"name": name, "platform": "linux-desktop"}, 200)
		if len(fmt.Sprint(out["deviceCode"])) < 32 || len(fmt.Sprint(out["userCode"])) < 6 || out["interval"] != float64(5) || out["expiresAt"] == nil {
			t.Fatalf("incomplete start response: %v", out)
		}
		return out
	}
	poll := func(request map[string]any, want string) map[string]any {
		t.Helper()
		out := peer.rpc("PollDeviceAuth", "", map[string]any{"deviceCode": request["deviceCode"]}, 200)
		if out["status"] != want {
			t.Fatalf("poll status=%v want=%s", out, want)
		}
		if want != "approved" && out["secret"] != nil && out["secret"] != "" {
			t.Fatalf("secret outside approved response: %v", out)
		}
		return out
	}
	legacy := c.call("TokenService", "CreateToken", map[string]any{"name": "pair: existing phone", "scopes": []string{"read", "events", "terminal", "session_*", "sleeve_*", "chat_*", "backlog_*"}})
	manual := c.call("TokenService", "CreateToken", map[string]any{"name": "manually-scoped", "scopes": []string{"read", "events", "terminal", "session_*", "sleeve_*", "chat_*", "backlog_*"}})
	beforePair := fmt.Sprint(c.call("TokenService", "ListTokens", map[string]any{}))
	pairEnv := append(append([]string(nil), c.env...), "PROTECTORATE_HOME="+filepath.Join(c.root, "untrusted-pair"))
	pairCmd := exec.CommandContext(c.ctx, c.binary, "pair", "--hub", c.url, "--app")
	pairCmd.Env = pairEnv
	link, pairErr := c.commandOutput(pairCmd)
	if pairErr != nil || !strings.Contains(string(link), "protectorate://pair?url=") || strings.Contains(string(link), "token=") || strings.Contains(string(link), "ptk_") {
		t.Fatalf("pair did not emit a URL-only app link: %s", link)
	}
	for _, removed := range []string{"--ttl=1h", "--host-shell"} {
		cmd := exec.CommandContext(c.ctx, c.binary, "pair", "--hub", c.url, removed)
		cmd.Env = pairEnv
		out, err := c.commandOutput(cmd)
		if err == nil || (!c.rust && !bytes.Contains(out, []byte("unknown flag"))) {
			t.Fatalf("removed pairing flag %s accepted: %v\n%s", removed, err, out)
		}
	}
	if afterPair := fmt.Sprint(c.call("TokenService", "ListTokens", map[string]any{})); beforePair != afterPair {
		t.Fatal("pair unexpectedly minted or modified a token")
	}
	c.stop(c.hub)
	c.boot()
	if listed := peer.rpc("ListDevices", owner, map[string]any{}, 200); !c.rust && !strings.Contains(fmt.Sprint(listed), legacy["token"].(map[string]any)["id"].(string)) {
		t.Fatalf("existing paired token missing from devices: %v", listed)
	} else if strings.Contains(fmt.Sprint(listed), manual["token"].(map[string]any)["id"].(string)) {
		t.Fatalf("ordinary scoped token was promoted to a device: %v", listed)
	}
	ctx, cancel := context.WithTimeout(c.ctx, 15*time.Second)
	defer cancel()
	events := protectoratev1connect.NewEventServiceClient(c.client, c.url, connect.WithProtoJSON())
	eventRequest := connect.NewRequest(&pb.SubscribeRequest{})
	eventRequest.Header().Set("Authorization", "Bearer "+owner)
	stream, err := events.Subscribe(ctx, eventRequest)
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Close()
	first := start("officepc")
	observed := false
	for stream.Receive() {
		event := stream.Msg()
		encoded := fmt.Sprint(event)
		if strings.Contains(encoded, first["deviceCode"].(string)) {
			t.Fatal("event leaked device code")
		}
		if event.Kind == "device_auth.pending" {
			observed = true
			break
		}
	}
	if !observed {
		t.Fatalf("pending device not published live: %v", stream.Err())
	}
	stream.Close()
	pending := peer.rpc("ListDeviceAuth", owner, map[string]any{}, 200)
	if !strings.Contains(fmt.Sprint(pending), "127.0.0.1") || strings.Contains(fmt.Sprint(pending), "192.0.2.123") {
		t.Fatalf("request IP trusted spoofed forwarded header: %v", pending)
	}
	poll(first, "pending")
	slow := poll(first, "slow_down")
	if slow["interval"] != float64(10) {
		t.Fatalf("slow_down interval: %v", slow)
	}
	advance(10 * time.Second)
	poll(first, "pending")
	peer.rpc("ApproveDeviceAuth", owner, map[string]any{"userCode": strings.ToLower(first["userCode"].(string))}, 200)
	c.stop(c.hub)
	c.boot()
	if !c.rust {
		migrated := peer.rpc("ListDevices", legacy["secret"].(string), map[string]any{}, 200)
		if !strings.Contains(fmt.Sprint(migrated), "full") {
			t.Fatalf("existing paired token did not migrate to Full: %v", migrated)
		}
	}
	advance(10 * time.Second)
	approved := poll(first, "approved")
	firstSecret, ok := approved["secret"].(string)
	if !ok || !strings.HasPrefix(firstSecret, "ptk_") {
		t.Fatalf("missing approved token: %v", approved)
	}
	firstToken := approved["token"].(map[string]any)
	if firstToken["name"] != "officepc" || firstToken["platform"] != "linux-desktop" || firstToken["approvedBy"] != "owner" || firstToken["approvedAt"] == nil || firstToken["expiresAt"] != nil {
		t.Fatalf("device metadata or lifetime: %v", firstToken)
	}
	peer.rpc("RenameDevice", owner, map[string]any{"id": firstToken["id"], "name": "orchestrator-envoy"}, 400)
	if status := c.rpc("TokenService", "UpdateTokenScopes", map[string]any{"id": firstToken["id"], "add": []string{"read"}}, nil); status != 400 {
		t.Fatalf("device scope update status=%d want=400", status)
	}
	if !c.rust {
		advance(10 * time.Second)
	}
	poll(first, "consumed")
	peer.rpc("ApproveDeviceAuth", owner, map[string]any{"userCode": first["userCode"]}, 404)
	c.token = firstSecret
	if status := c.rpc("HubService", "GetStatus", map[string]any{}, nil); status != 200 {
		t.Fatalf("Full token cannot read hub: %d", status)
	}
	if status := c.rpc("TokenService", "ListTokens", map[string]any{}, nil); status != 403 {
		t.Fatalf("Full token admitted owner-only operation: %d", status)
	}
	mail := c.call("MessageService", "Send", map[string]any{"to": "owner", "body": "device owner-mailbox delivery", "ackRequired": true})["message"].(map[string]any)
	if mail["from"] != "owner" {
		t.Fatalf("device message used a separate mailbox: %v", mail)
	}
	c.token = owner
	if inbox := c.call("MessageService", "Inbox", map[string]any{"peek": true}); !strings.Contains(fmt.Sprint(inbox), mail["id"].(string)) {
		t.Fatalf("owner did not receive device message: %v", inbox)
	}
	c.token = firstSecret
	c.call("MessageService", "Ack", map[string]any{"id": mail["id"]})
	if status := c.rpc("MessageService", "Inbox", map[string]any{"as": "sibling"}, nil); status != 403 {
		t.Fatalf("device impersonated another mailbox: %d", status)
	}
	c.token = owner
	second := start("phone")
	c.stop(c.hub)
	c.boot()
	peer.rpc("ApproveDeviceAuth", firstSecret, map[string]any{"userCode": second["userCode"]}, 200)
	secondApproval := poll(second, "approved")
	secondSecret := secondApproval["secret"].(string)
	secondToken := secondApproval["token"].(map[string]any)
	if !strings.Contains(fmt.Sprint(secondToken["approvedBy"]), "officepc") {
		t.Fatalf("missing device approver: %v", secondToken)
	}
	reader := c.call("TokenService", "CreateToken", map[string]any{"name": "untrusted-reader", "scopes": []string{"read", "device_*"}})["secret"].(string)
	denied := start("denied-device")
	for _, method := range []string{"ApproveDeviceAuth", "DenyDeviceAuth"} {
		peer.rpc(method, reader, map[string]any{"userCode": denied["userCode"]}, 403)
		peer.rpc(method, "", map[string]any{"userCode": denied["userCode"]}, 401)
	}
	c.cli("devices", "deny", denied["userCode"].(string))
	poll(denied, "denied")
	expired := start("expired-device")
	if !c.rust {
		advance(10*time.Minute + time.Second)
		poll(expired, "expired")
		peer.rpc("ApproveDeviceAuth", owner, map[string]any{"userCode": expired["userCode"]}, 404)
		if pending := peer.rpc("ListDeviceAuth", owner, map[string]any{}, 200); strings.Contains(fmt.Sprint(pending), expired["userCode"].(string)) {
			t.Fatalf("expired request remained pending: %v", pending)
		}
	}
	c.cli("devices", "rename", secondToken["id"].(string), "renamed-phone")
	if listed := c.cli("devices", "ls", "--json"); !strings.Contains(listed, "renamed-phone") {
		t.Fatalf("CLI rename missing: %s", listed)
	}
	c.stop(c.hub)
	c.boot()
	listedDevices := peer.rpc("ListDevices", firstSecret, map[string]any{}, 200)
	used := false
	for _, entry := range listedDevices["devices"].([]any) {
		device := entry.(map[string]any)
		if device["id"] == firstToken["id"] && device["lastUsedAt"] != nil {
			used = true
		}
	}
	if !used {
		t.Fatalf("device last use missing after restart: %v", listedDevices)
	}
	deviceReq := connect.NewRequest(&pb.SubscribeRequest{})
	deviceReq.Header().Set("Authorization", "Bearer "+secondSecret)
	live, err := events.Subscribe(c.ctx, deviceReq)
	if err != nil {
		t.Fatal(err)
	}
	defer live.Close()
	streamSubject := ""
	if c.rust {
		peer.rpc("RenameDevice", owner, map[string]any{"id": firstToken["id"], "name": "officepc-live"}, 200)
		streamSubject = firstToken["id"].(string)
	} else {
		lease := c.call("LeaseService", "AcquireLease", map[string]any{"resource": "peer", "holder": "device-auth", "ttl": "30s"})
		streamSubject = lease["id"].(string)
	}
	for live.Receive() {
		if live.Msg().Subject == streamSubject {
			break
		}
	}
	if err := live.Err(); err != nil {
		t.Fatalf("live device stream: %v", err)
	}
	c.cli("devices", "revoke", secondToken["id"].(string))
	closed := make(chan error, 1)
	go func() {
		for live.Receive() {
		}
		closed <- live.Err()
	}()
	select {
	case err := <-closed:
		if connect.CodeOf(err) != connect.CodeUnauthenticated {
			t.Fatalf("revoked stream error: %v", err)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("revocation did not cut live event stream")
	}
	peer.rpc("ListDevices", secondSecret, map[string]any{}, 401)
	if !c.rust {
		c.call("LeaseService", "ReleaseLease", map[string]any{"id": streamSubject})
	}
	c.stop(c.hub)
	c.boot()
	peer.rpc("ListDevices", secondSecret, map[string]any{}, 401)
	peer.rpc("ListDevices", firstSecret, map[string]any{}, 200)
	deviceCLIApproval(t, c, peer)
	paths, err := filepath.Glob(filepath.Join(c.home, "audit", "*.jsonl"))
	if err != nil {
		t.Fatal(err)
	}
	audit := ""
	for _, path := range paths {
		b, err := os.ReadFile(path)
		if err != nil {
			t.Fatal(err)
		}
		audit += string(b)
	}
	for _, secret := range []string{firstSecret, secondSecret, first["deviceCode"].(string), second["deviceCode"].(string)} {
		if strings.Contains(audit, secret) {
			t.Fatal("audit leaked device or token secret")
		}
		identityFile := "tokens.json"
		if c.rust {
			identityFile = "identity.json"
		}
		state, err := os.ReadFile(filepath.Join(c.home, identityFile))
		if err != nil {
			t.Fatal(err)
		}
		if bytes.Contains(state, []byte(secret)) {
			t.Fatal("persisted state retained recoverable secret")
		}
		logs, err := filepath.Glob(filepath.Join(c.root, "hub-*.log"))
		if err != nil {
			t.Fatal(err)
		}
		for _, path := range logs {
			b, err := os.ReadFile(path)
			if err != nil {
				t.Fatal(err)
			}
			if bytes.Contains(b, []byte(secret)) {
				t.Fatal("hub log leaked device or token secret")
			}
		}
	}
	for _, operation := range []string{"StartDeviceAuth", "ApproveDeviceAuth", "DenyDeviceAuth", "RevokeDevice"} {
		if !strings.Contains(audit, "/"+operation) {
			t.Fatalf("missing %s audit", operation)
		}
	}
	if !strings.Contains(audit, "token:officepc") || !strings.Contains(audit, "unauthenticated") {
		t.Fatal("audit omitted device or unauthenticated actors")
	}
	c.stop(c.hub)
	c.boot("--trusted-proxy", "127.0.0.2")
	// Through a trusted proxy the forwarded address is the client, so the
	// non-LAN 192.0.2.123 is refused; the direct loopback peer is still allowed.
	if refused := devicePeer(c, "127.0.0.2").rpc("StartDeviceAuth", "", map[string]any{"name": "via-proxy", "platform": "linux-desktop"}, 403); !strings.Contains(fmt.Sprint(refused["message"]), "LAN only") {
		t.Fatalf("trusted proxy's forwarded client address not applied: %v", refused)
	}
	peer.rpc("StartDeviceAuth", "", map[string]any{"name": "direct", "platform": "linux-desktop"}, 200)
}

func deviceCLIApproval(t *testing.T, c *cluster, peer deviceAuthPeer) {
	t.Helper()
	home := filepath.Join(c.root, "remote-cli")
	cmd := exec.CommandContext(c.ctx, c.binary, "login", "--hub", c.url, "--name", "remote-cli", "--platform", "linux-cli")
	cmd.Env = append(append([]string(nil), c.env...), "PROTECTORATE_HOME="+home)
	log, err := os.CreateTemp(c.root, "device-login-*.log")
	if err != nil {
		t.Fatal(err)
	}
	defer log.Close()
	cmd.Stdout, cmd.Stderr = log, log
	if err := cmd.Start(); err != nil {
		t.Fatal(err)
	}
	done := make(chan error, 1)
	go func() { done <- cmd.Wait() }()
	finished := false
	t.Cleanup(func() {
		if !finished {
			_ = cmd.Process.Kill()
			<-done
		}
	})
	code := ""
	c.eventually(func() bool {
		pending := peer.rpc("ListDeviceAuth", c.token, map[string]any{}, 200)
		requests, _ := pending["requests"].([]any)
		for _, request := range requests {
			item := request.(map[string]any)
			if item["name"] == "remote-cli" {
				code, _ = item["userCode"].(string)
				return code != ""
			}
		}
		return false
	})
	b, err := os.ReadFile(log.Name())
	if err != nil || !strings.Contains(string(b), code) {
		t.Fatalf("CLI did not show approval code %s: %s (%v)", code, b, err)
	}
	c.cli("devices", "approve", code)
	select {
	case err := <-done:
		finished = true
		if err != nil {
			b, _ := os.ReadFile(log.Name())
			t.Fatalf("CLI login failed: %v\n%s", err, b)
		}
	case <-time.After(12 * time.Second):
		t.Fatal("CLI login did not finish after approval")
	}
	status := exec.CommandContext(c.ctx, c.binary, "--hub", c.url, "status", "--json")
	status.Env = cmd.Env
	if out, err := c.commandOutput(status); err != nil || !bytes.Contains(out, []byte("hostname")) {
		t.Fatalf("CLI login credential unusable: %v\n%s", err, out)
	}
}

func TestBinaryDeviceAuthorizationLimits(t *testing.T) {
	if os.Getenv("PROT_E2E_COMPANION_BINARY") != "" {
		testRustDeviceAuthorizationLimits(t)
		return
	}
	c := newCluster(t)
	advance := deviceClock(t, c)
	peer := devicePeer(c, "127.0.0.1")
	for i := 0; i < 10; i++ {
		peer.rpc("StartDeviceAuth", "", map[string]any{"name": "limited", "platform": "cli"}, 200)
	}
	peer.rpc("StartDeviceAuth", "", map[string]any{"name": "limited", "platform": "cli"}, 429)
	malformed := devicePeer(c, "127.0.0.99")
	for i := 0; i <= 10; i++ {
		req, err := http.NewRequestWithContext(c.ctx, http.MethodPost, c.url+"/protectorate.v1.DeviceService/StartDeviceAuth", strings.NewReader("{"))
		if err != nil {
			t.Fatal(err)
		}
		req.Header.Set("Content-Type", "application/json")
		res, err := malformed.client.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		_, err = io.Copy(io.Discard, res.Body)
		res.Body.Close()
		if err != nil {
			t.Fatal(err)
		}
		want := 400
		if i == 10 {
			want = 429
		}
		if res.StatusCode != want {
			t.Fatalf("malformed start %d: HTTP %d want %d", i+1, res.StatusCode, want)
		}
	}
	advance(time.Minute + time.Second)
	for i := 0; i < 60; i++ {
		source := devicePeer(c, fmt.Sprintf("127.0.0.%d", i/10+2))
		source.rpc("StartDeviceAuth", "", map[string]any{"name": "global-limit", "platform": "cli"}, 200)
	}
	devicePeer(c, "127.0.0.20").rpc("StartDeviceAuth", "", map[string]any{"name": "global-limit", "platform": "cli"}, 429)
	advance(time.Minute + time.Second)
	for i := 0; i < 120; i++ {
		peer.rpc("PollDeviceAuth", "", map[string]any{"deviceCode": "unknown-device-code"}, 200)
	}
	peer.rpc("PollDeviceAuth", "", map[string]any{"deviceCode": "unknown-device-code"}, 429)
	advance(time.Minute + time.Second)
	for i := 0; i < 1200; i++ {
		source := devicePeer(c, fmt.Sprintf("127.0.0.%d", i/120+30))
		source.rpc("PollDeviceAuth", "", map[string]any{"deviceCode": "unknown-device-code"}, 200)
	}
	devicePeer(c, "127.0.0.50").rpc("PollDeviceAuth", "", map[string]any{"deviceCode": "unknown-device-code"}, 429)
	advance(time.Minute + time.Second)
	for i := 0; i < 30; i++ {
		source := devicePeer(c, fmt.Sprintf("127.0.0.%d", i/10+60))
		source.rpc("StartDeviceAuth", "", map[string]any{"name": "pending-cap", "platform": "cli"}, 200)
	}
	devicePeer(c, "127.0.0.70").rpc("StartDeviceAuth", "", map[string]any{"name": "pending-cap", "platform": "cli"}, 429)
	advance(10*time.Minute + time.Second)
	peer.rpc("StartDeviceAuth", "", map[string]any{"name": "after-expiry", "platform": "cli"}, 200)
}

func testRustDeviceAuthorizationLimits(t *testing.T) {
	malformedStart := func(t *testing.T, peer deviceAuthPeer, want int) {
		t.Helper()
		req, err := http.NewRequestWithContext(peer.c.ctx, http.MethodPost, peer.c.url+"/protectorate.v1.DeviceService/StartDeviceAuth", strings.NewReader("{"))
		if err != nil {
			t.Fatal(err)
		}
		req.Header.Set("Content-Type", "application/json")
		res, err := peer.client.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		_, err = io.Copy(io.Discard, res.Body)
		res.Body.Close()
		if err != nil {
			t.Fatal(err)
		}
		if res.StatusCode != want {
			t.Fatalf("malformed start: HTTP %d want %d", res.StatusCode, want)
		}
	}
	t.Run("local", func(t *testing.T) {
		c := newCluster(t)
		peer := devicePeer(c, "127.0.0.1")
		for i := 0; i < 10; i++ {
			peer.rpc("StartDeviceAuth", "", map[string]any{"name": "limited", "platform": "cli"}, 200)
		}
		peer.rpc("StartDeviceAuth", "", map[string]any{"name": "limited", "platform": "cli"}, 429)
		malformed := devicePeer(c, "127.0.0.99")
		for i := 0; i < 10; i++ {
			malformedStart(t, malformed, 400)
		}
		malformedStart(t, malformed, 429)
		for i := 0; i < 120; i++ {
			peer.rpc("PollDeviceAuth", "", map[string]any{"deviceCode": "unknown-device-code"}, 200)
		}
		peer.rpc("PollDeviceAuth", "", map[string]any{"deviceCode": "unknown-device-code"}, 429)
	})
	t.Run("global_start", func(t *testing.T) {
		c := newCluster(t)
		peers := make([]deviceAuthPeer, 6)
		for i := range peers {
			peers[i] = devicePeer(c, fmt.Sprintf("127.0.0.%d", i+2))
		}
		for i := 0; i < 60; i++ {
			malformedStart(t, peers[i/10], 400)
		}
		malformedStart(t, devicePeer(c, "127.0.0.20"), 429)
	})
	t.Run("global_poll", func(t *testing.T) {
		c := newCluster(t)
		peers := make([]deviceAuthPeer, 10)
		for i := range peers {
			peers[i] = devicePeer(c, fmt.Sprintf("127.0.0.%d", i+30))
		}
		for i := 0; i < 1200; i++ {
			peers[i/120].rpc("PollDeviceAuth", "", map[string]any{"deviceCode": "unknown-device-code"}, 200)
		}
		devicePeer(c, "127.0.0.50").rpc("PollDeviceAuth", "", map[string]any{"deviceCode": "unknown-device-code"}, 429)
	})
	t.Run("pending_capacity", func(t *testing.T) {
		c := newCluster(t)
		peers := make([]deviceAuthPeer, 10)
		for i := range peers {
			peers[i] = devicePeer(c, fmt.Sprintf("127.0.0.%d", i+60))
		}
		for i := 0; i < 100; i++ {
			if i == 60 {
				c.stop(c.hub)
				c.boot()
			}
			peers[i/10].rpc("StartDeviceAuth", "", map[string]any{"name": "pending-cap", "platform": "cli"}, 200)
		}
		devicePeer(c, "127.0.0.70").rpc("StartDeviceAuth", "", map[string]any{"name": "pending-cap", "platform": "cli"}, 429)
		c.stop(c.hub)
		c.boot()
		devicePeer(c, "127.0.0.70").rpc("StartDeviceAuth", "", map[string]any{"name": "pending-cap", "platform": "cli"}, 429)
	})
	t.Run("window_reset", func(t *testing.T) {
		c := newCluster(t)
		peer := devicePeer(c, "127.0.0.1")
		for i := 0; i < 10; i++ {
			malformedStart(t, peer, 400)
		}
		malformedStart(t, peer, 429)
		select {
		case <-time.After(61 * time.Second):
		case <-c.ctx.Done():
			t.Fatal(c.ctx.Err())
		}
		malformedStart(t, peer, 400)
	})
}
