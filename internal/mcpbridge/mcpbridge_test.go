package mcpbridge

import (
	"bytes"
	"context"
	"encoding/json"
	"flag"
	"net/http"
	"net/http/httptest"
	"os"
	"reflect"
	"strings"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/reflect/protoregistry"
	"google.golang.org/protobuf/types/descriptorpb"

	"github.com/hotschmoe/protectorate/internal/hub"
)

var update = flag.Bool("update", false, "rewrite testdata/tools.golden.json")

const testToken = "secret"

type bearer struct{}

func (bearer) RoundTrip(r *http.Request) (*http.Response, error) {
	r = r.Clone(r.Context())
	r.Header.Set("Authorization", "Bearer "+testToken)
	return http.DefaultTransport.RoundTrip(r)
}

func connectClient(t *testing.T, f Filter) *mcp.ClientSession {
	t.Helper()
	t.Setenv("PROTECTORATE_HOME", t.TempDir())
	hs := httptest.NewServer(hub.New("test", testToken).Handler())
	t.Cleanup(hs.Close)
	srv, err := NewServer(Config{HubURL: hs.URL, HTTPClient: &http.Client{Transport: bearer{}}, Version: "test", Filter: f})
	if err != nil {
		t.Fatal(err)
	}
	ct, st := mcp.NewInMemoryTransports()
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	if _, err := srv.Connect(ctx, st, nil); err != nil {
		t.Fatal(err)
	}
	cs, err := mcp.NewClient(&mcp.Implementation{Name: "test"}, nil).Connect(ctx, ct, nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { cs.Close() })
	return cs
}

func listTools(t *testing.T, cs *mcp.ClientSession) []*mcp.Tool {
	t.Helper()
	res, err := cs.ListTools(context.Background(), nil)
	if err != nil {
		t.Fatal(err)
	}
	return res.Tools
}

func call(t *testing.T, cs *mcp.ClientSession, name string, args any) (string, bool) {
	t.Helper()
	res, err := cs.CallTool(context.Background(), &mcp.CallToolParams{Name: name, Arguments: args})
	if err != nil {
		t.Fatal(err)
	}
	return res.Content[0].(*mcp.TextContent).Text, res.IsError
}

func TestEveryRPCHasDescription(t *testing.T) {
	defs, err := Catalog()
	if err != nil {
		t.Fatal(err)
	}
	if len(defs) == 0 {
		t.Fatal("empty catalog")
	}
	for _, d := range defs {
		if d.Description == "" {
			t.Errorf("%s (%s) has no proto comment; document the RPC", d.Name, d.Method.FullName())
		}
	}
}

func TestOwnerOnlyServicesNotExposed(t *testing.T) {
	defs, err := Catalog()
	if err != nil {
		t.Fatal(err)
	}
	for _, d := range defs {
		if strings.HasPrefix(d.Name, "token_") {
			t.Errorf("owner-only tool %s exposed", d.Name)
		}
	}
	for _, name := range []string{"session_create_session", "session_respawn_session", "node_add_node"} {
		for _, d := range defs {
			if d.Name == name {
				t.Errorf("owner-only rpc %s exposed", name)
			}
		}
	}
}

func TestImageMatchesGeneratedCode(t *testing.T) {
	var set descriptorpb.FileDescriptorSet
	if err := proto.Unmarshal(image, &set); err != nil {
		t.Fatal(err)
	}
	checked := 0
	for _, fdp := range set.File {
		if !strings.HasPrefix(fdp.GetPackage(), apiPackage) {
			continue
		}
		fd, err := protoregistry.GlobalFiles.FindFileByPath(fdp.GetName())
		if err != nil {
			t.Fatalf("%s in image but not compiled in: run make gen", fdp.GetName())
		}
		fdp = proto.Clone(fdp).(*descriptorpb.FileDescriptorProto)
		fdp.SourceCodeInfo = nil
		fdp.ProtoReflect().SetUnknown(nil)
		stripJSONNames(fdp.MessageType)
		want := protodesc.ToFileDescriptorProto(fd)
		stripJSONNames(want.MessageType)
		if !proto.Equal(fdp, want) {
			t.Errorf("%s: image differs from generated code; run make gen", fdp.GetName())
		}
		checked++
	}
	protoregistry.GlobalFiles.RangeFiles(func(fd protoreflect.FileDescriptor) bool {
		if string(fd.Package()) == apiPackage {
			checked--
		}
		return true
	})
	if checked != 0 {
		t.Errorf("image and compiled proto files differ in count by %d; run make gen", checked)
	}
}

func stripJSONNames(msgs []*descriptorpb.DescriptorProto) {
	for _, m := range msgs {
		for _, f := range m.Field {
			f.JsonName = nil
		}
		stripJSONNames(m.NestedType)
	}
}

func TestToolsListGolden(t *testing.T) {
	cs := connectClient(t, Filter{Allow: []string{"*"}})
	got, err := json.MarshalIndent(listTools(t, cs), "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	got = append(got, '\n')
	const golden = "testdata/tools.golden.json"
	if *update {
		if err := os.WriteFile(golden, got, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	want, err := os.ReadFile(golden)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, want) {
		t.Errorf("tools/list differs from %s; run go test ./internal/mcpbridge -update and review the diff", golden)
	}
}

func TestSchemaShapes(t *testing.T) {
	cs := connectClient(t, Filter{Allow: []string{"*"}})
	byName := map[string]*mcp.Tool{}
	for _, tl := range listTools(t, cs) {
		byName[tl.Name] = tl
	}
	for _, n := range []string{"hub_get_status", "quota_get_quota", "session_list_sessions", "job_submit_job",
		"session_kill_session", "session_capture_pane", "backlog_list_items", "backlog_add_item", "backlog_update_item"} {
		if byName[n] == nil {
			t.Errorf("missing tool %s", n)
		}
	}
	if byName["event_subscribe"] != nil {
		t.Error("streaming RPC must not be a tool")
	}
	schema := func(n string) map[string]any {
		var m map[string]any
		b, _ := json.Marshal(byName[n].InputSchema)
		json.Unmarshal(b, &m)
		return m
	}
	get := func(m map[string]any, path ...string) any {
		var cur any = m
		for _, p := range path {
			cur = cur.(map[string]any)[p]
		}
		return cur
	}
	if got := get(schema("backlog_list_items"), "properties", "state", "enum"); !reflect.DeepEqual(got, []any{"ITEM_STATE_UNSPECIFIED", "ITEM_STATE_TODO", "ITEM_STATE_CLAIMED", "ITEM_STATE_DONE"}) {
		t.Errorf("state enum: %v", got)
	}
	if got := get(schema("job_submit_job"), "properties", "command", "type"); got != "array" {
		t.Errorf("command type: %v", got)
	}
	if got := get(schema("job_submit_job"), "properties", "env", "additionalProperties", "type"); got != "string" {
		t.Errorf("env value type: %v", got)
	}
	if got := get(schema("backlog_update_item"), "properties", "claim", "properties", "at", "format"); got != "date-time" {
		t.Errorf("timestamp format: %v", got)
	}
	if got := get(schema("backlog_add_item"), "additionalProperties"); got != false {
		t.Errorf("additionalProperties: %v", got)
	}
	if byName["session_kill_session"].Annotations.ReadOnlyHint || !*byName["session_kill_session"].Annotations.DestructiveHint {
		t.Error("kill_session must be a destructive mutation")
	}
	if a := byName["backlog_add_item"].Annotations; a.ReadOnlyHint || *a.DestructiveHint {
		t.Error("add_item must be a non-destructive mutation")
	}
	if !byName["quota_get_quota"].Annotations.ReadOnlyHint {
		t.Error("get_quota must be read-only")
	}
}

func TestCallTools(t *testing.T) {
	cs := connectClient(t, Filter{Allow: []string{"*"}})

	text, isErr := call(t, cs, "hub_get_status", nil)
	if isErr || !strings.Contains(text, `"version":"test"`) {
		t.Fatalf("hub_get_status: %q %v", text, isErr)
	}

	text, isErr = call(t, cs, "backlog_add_item", map[string]any{"project": "me/repo", "title": "Fix it", "priority": 2})
	if isErr || !strings.Contains(text, `"title":"Fix it"`) {
		t.Fatalf("add: %q %v", text, isErr)
	}
	text, isErr = call(t, cs, "backlog_list_items", map[string]any{"state": "ITEM_STATE_TODO"})
	if isErr || !strings.Contains(text, `"Fix it"`) {
		t.Fatalf("list: %q %v", text, isErr)
	}

	text, isErr = call(t, cs, "backlog_add_item", map[string]any{"project": "me/repo", "titel": "typo"})
	if !isErr || !strings.HasPrefix(text, "invalid_argument:") || !strings.Contains(text, "titel") {
		t.Errorf("unknown field: %q %v", text, isErr)
	}

	text, isErr = call(t, cs, "backlog_update_item", map[string]any{"id": "nope", "state": "ITEM_STATE_DONE"})
	if !isErr || !strings.HasPrefix(text, "not_found:") {
		t.Errorf("missing item: %q %v", text, isErr)
	}
}

func TestFilter(t *testing.T) {
	names := func(f Filter) []string {
		var out []string
		for _, tl := range listTools(t, connectClient(t, f)) {
			out = append(out, tl.Name)
		}
		return out
	}
	readOnly := []string{"backlog_list_items", "hub_get_status", "job_get_job", "job_list_jobs", "lease_list_leases", "node_get_node", "node_list_nodes", "quota_get_quota", "quota_suggest", "session_capture_pane", "session_list_sessions", "sleeve_get_sleeve", "sleeve_list_sleeves", "tailnet_get_status"}
	cases := []struct {
		name string
		f    Filter
		want []string
	}{
		{"default is read-only", Filter{}, readOnly},
		{"allow glob replaces default", Filter{Allow: []string{"backlog_*"}}, []string{"backlog_add_item", "backlog_list_items", "backlog_update_item"}},
		{"deny wins", Filter{Allow: []string{"backlog_*"}, Deny: []string{"*_update_item", "backlog_add_*"}}, []string{"backlog_list_items"}},
		{"deny on default", Filter{Deny: []string{"session_*", "node_*"}}, []string{"backlog_list_items", "hub_get_status", "job_get_job", "job_list_jobs", "lease_list_leases", "quota_get_quota", "quota_suggest", "sleeve_get_sleeve", "sleeve_list_sleeves", "tailnet_get_status"}},
	}
	for _, c := range cases {
		if got := names(c.f); !reflect.DeepEqual(got, c.want) {
			t.Errorf("%s: got %v want %v", c.name, got, c.want)
		}
	}
	if _, err := NewServer(Config{Filter: Filter{Allow: []string{"["}}}); err == nil {
		t.Error("bad glob accepted")
	}
}

func TestStreamableHTTP(t *testing.T) {
	t.Setenv("PROTECTORATE_HOME", t.TempDir())
	hs := httptest.NewServer(hub.New("test", testToken).Handler())
	t.Cleanup(hs.Close)
	srv, err := NewServer(Config{HubURL: hs.URL, HTTPClient: &http.Client{Transport: bearer{}}})
	if err != nil {
		t.Fatal(err)
	}
	const bridgeToken = "bridge-secret"
	ms := httptest.NewServer(HTTPHandler(srv, bridgeToken))
	t.Cleanup(ms.Close)

	denied := []struct {
		name   string
		header string
	}{
		{"no header", ""},
		{"wrong token", "Bearer nope"},
		{"wrong scheme", "Basic " + bridgeToken},
		{"bare token", bridgeToken},
		{"prefix of token", "Bearer bridge"},
		{"empty bearer", "Bearer "},
	}
	for _, tt := range denied {
		t.Run(tt.name, func(t *testing.T) {
			req, _ := http.NewRequest(http.MethodPost, ms.URL, strings.NewReader("{}"))
			if tt.header != "" {
				req.Header.Set("Authorization", tt.header)
			}
			res, err := http.DefaultClient.Do(req)
			if err != nil {
				t.Fatal(err)
			}
			res.Body.Close()
			if res.StatusCode != http.StatusUnauthorized {
				t.Fatalf("status = %d", res.StatusCode)
			}
		})
	}
	t.Run("empty configured token denies all", func(t *testing.T) {
		open := httptest.NewServer(HTTPHandler(srv, ""))
		defer open.Close()
		req, _ := http.NewRequest(http.MethodPost, open.URL, strings.NewReader("{}"))
		req.Header.Set("Authorization", "Bearer ")
		res, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		res.Body.Close()
		if res.StatusCode != http.StatusUnauthorized {
			t.Fatalf("status = %d", res.StatusCode)
		}
	})

	cs, err := mcp.NewClient(&mcp.Implementation{Name: "test"}, nil).
		Connect(context.Background(), &mcp.StreamableClientTransport{
			Endpoint:   ms.URL,
			HTTPClient: &http.Client{Transport: headerToken(bridgeToken)},
		}, nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { cs.Close() })
	if text, isErr := call(t, cs, "hub_get_status", nil); isErr || !strings.Contains(text, `"version":"test"`) {
		t.Fatalf("%q %v", text, isErr)
	}
}

type headerToken string

func (h headerToken) RoundTrip(r *http.Request) (*http.Response, error) {
	r = r.Clone(r.Context())
	r.Header.Set("Authorization", "Bearer "+string(h))
	return http.DefaultTransport.RoundTrip(r)
}
