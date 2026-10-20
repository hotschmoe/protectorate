package mcpbridge

import (
	"context"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"

	"connectrpc.com/connect"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/types/dynamicpb"
)

type Config struct {
	HubURL     string
	HTTPClient connect.HTTPClient
	Version    string
	Filter     Filter
}

type Tool map[string]any

type toolResult struct {
	Content []textContent `json:"content"`
	IsError bool          `json:"isError,omitempty"`
}

type textContent struct {
	Type string `json:"type"`
	Text string `json:"text"`
}

type Server struct {
	version  string
	tools    []Tool
	handlers map[string]func(context.Context, json.RawMessage) *toolResult
}

func NewServer(cfg Config) (*Server, error) {
	if err := cfg.Filter.Validate(); err != nil {
		return nil, err
	}
	defs, err := Catalog()
	if err != nil {
		return nil, err
	}
	srv := &Server{version: cfg.Version, tools: []Tool{}, handlers: make(map[string]func(context.Context, json.RawMessage) *toolResult)}
	for _, def := range defs {
		if !cfg.Filter.Permits(def) {
			continue
		}
		a := map[string]any{"readOnlyHint": def.ReadOnly, "idempotentHint": def.ReadOnly, "openWorldHint": false}
		if !def.ReadOnly {
			a["destructiveHint"] = def.Destructive
		}
		srv.tools = append(srv.tools, Tool{"name": def.Name, "description": def.Description, "inputSchema": InputSchema(def.Method.Input()), "annotations": a})
		srv.handlers[def.Name] = handler(def, cfg.HTTPClient, strings.TrimRight(cfg.HubURL, "/"))
	}
	return srv, nil
}

// Tools is the catalog served by tools/list.
func (s *Server) Tools() []Tool { return s.tools }

func HTTPHandler(srv *Server, token string) http.Handler {
	transport := srv.httpHandler()
	want := sha256.Sum256([]byte(token))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		got, ok := strings.CutPrefix(r.Header.Get("Authorization"), "Bearer ")
		sum := sha256.Sum256([]byte(got))
		if token == "" || !ok || subtle.ConstantTimeCompare(sum[:], want[:]) != 1 {
			w.Header().Set("WWW-Authenticate", "Bearer")
			http.Error(w, "missing or invalid bearer token", http.StatusUnauthorized)
			return
		}
		transport.ServeHTTP(w, r)
	})
}

func handler(def ToolDef, hc connect.HTTPClient, base string) func(context.Context, json.RawMessage) *toolResult {
	in, out := def.Method.Input(), def.Method.Output()
	client := connect.NewClient[dynamicpb.Message, dynamicpb.Message](hc, base+def.Path(), connect.WithProtoJSON(), connect.WithResponseInitializer(func(_ connect.Spec, msg any) error {
		*msg.(*dynamicpb.Message) = *dynamicpb.NewMessage(out)
		return nil
	}))
	return func(ctx context.Context, raw json.RawMessage) *toolResult {
		msg, err := decodeArgs(in, raw)
		if err != nil {
			return toolError(connect.CodeInvalidArgument, err.Error())
		}
		res, err := client.CallUnary(ctx, connect.NewRequest(msg))
		if err != nil {
			var ce *connect.Error
			if errors.As(err, &ce) {
				return toolError(ce.Code(), ce.Message())
			}
			return toolError(connect.CodeUnavailable, err.Error())
		}
		b, err := protojson.Marshal(res.Msg)
		if err != nil {
			return toolError(connect.CodeInternal, err.Error())
		}
		return &toolResult{Content: []textContent{{Type: "text", Text: string(b)}}}
	}
}

func decodeArgs(md protoreflect.MessageDescriptor, raw json.RawMessage) (*dynamicpb.Message, error) {
	if len(raw) == 0 || string(raw) == "null" {
		raw = json.RawMessage("{}")
	}
	msg := dynamicpb.NewMessage(md)
	if err := (protojson.UnmarshalOptions{}).Unmarshal(raw, msg); err != nil {
		return nil, fmt.Errorf("invalid arguments: %w", err)
	}
	return msg, nil
}

func toolError(code connect.Code, msg string) *toolResult {
	return &toolResult{IsError: true, Content: []textContent{{Type: "text", Text: fmt.Sprintf("%s: %s", code, msg)}}}
}
