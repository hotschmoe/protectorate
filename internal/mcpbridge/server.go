package mcpbridge

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"

	"connectrpc.com/connect"
	"github.com/modelcontextprotocol/go-sdk/mcp"
	"google.golang.org/protobuf/encoding/protojson"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/types/dynamicpb"
)

// Config describes the hub to bridge to and which tools to serve.
type Config struct {
	HubURL     string
	HTTPClient connect.HTTPClient
	Version    string
	Filter     Filter
}

// NewServer builds an MCP server with one tool per permitted unary RPC.
func NewServer(cfg Config) (*mcp.Server, error) {
	if err := cfg.Filter.Validate(); err != nil {
		return nil, err
	}
	defs, err := Catalog()
	if err != nil {
		return nil, err
	}
	srv := mcp.NewServer(&mcp.Implementation{Name: "protectorate", Version: cfg.Version}, nil)
	base := strings.TrimRight(cfg.HubURL, "/")
	for _, def := range defs {
		if !cfg.Filter.Permits(def) {
			continue
		}
		srv.AddTool(&mcp.Tool{
			Name:        def.Name,
			Description: def.Description,
			InputSchema: InputSchema(def.Method.Input()),
			Annotations: annotations(def),
		}, handler(def, cfg.HTTPClient, base))
	}
	return srv, nil
}

// HTTPHandler serves the same tools over streamable HTTP.
func HTTPHandler(srv *mcp.Server) http.Handler {
	return mcp.NewStreamableHTTPHandler(func(*http.Request) *mcp.Server { return srv }, nil)
}

func annotations(def ToolDef) *mcp.ToolAnnotations {
	a := &mcp.ToolAnnotations{ReadOnlyHint: def.ReadOnly, IdempotentHint: def.ReadOnly}
	closed := false
	a.OpenWorldHint = &closed
	if !def.ReadOnly {
		d := def.Destructive
		a.DestructiveHint = &d
	}
	return a
}

func handler(def ToolDef, hc connect.HTTPClient, base string) mcp.ToolHandler {
	in, out := def.Method.Input(), def.Method.Output()
	client := connect.NewClient[dynamicpb.Message, dynamicpb.Message](hc, base+def.Path(),
		connect.WithProtoJSON(),
		connect.WithResponseInitializer(func(_ connect.Spec, msg any) error {
			*msg.(*dynamicpb.Message) = *dynamicpb.NewMessage(out)
			return nil
		}),
	)
	return func(ctx context.Context, req *mcp.CallToolRequest) (*mcp.CallToolResult, error) {
		msg, err := decodeArgs(in, req.Params.Arguments)
		if err != nil {
			return toolError(connect.CodeInvalidArgument, err.Error()), nil
		}
		res, err := client.CallUnary(ctx, connect.NewRequest(msg))
		if err != nil {
			var ce *connect.Error
			if errors.As(err, &ce) {
				return toolError(ce.Code(), ce.Message()), nil
			}
			return toolError(connect.CodeUnavailable, err.Error()), nil
		}
		b, err := protojson.Marshal(res.Msg)
		if err != nil {
			return toolError(connect.CodeInternal, err.Error()), nil
		}
		return &mcp.CallToolResult{Content: []mcp.Content{&mcp.TextContent{Text: string(b)}}}, nil
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

func toolError(code connect.Code, msg string) *mcp.CallToolResult {
	return &mcp.CallToolResult{
		IsError: true,
		Content: []mcp.Content{&mcp.TextContent{Text: fmt.Sprintf("%s: %s", code, msg)}},
	}
}
