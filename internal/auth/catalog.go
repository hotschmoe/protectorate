// Package auth implements scoped API tokens, request authorization and the
// audit log shared by every hub entry point.
//
// Scope grammar (the canonical form is the MCP tool name):
//
//	read               every NO_SIDE_EFFECTS RPC, derived from the descriptors
//	events             the EventService stream
//	terminal           the WebSocket terminal, read-write or read-only
//	terminal:readonly  the WebSocket terminal with readonly=1 only
//	node_admin:<glob>  care nodes whose name the glob matches, and admin jobs
//	                   there (granted per task, see the grant RPC)
//	<glob>             path.Match glob over tool names such as session_* or
//	                   backlog_create_note; the tool name is the snake_case
//	                   service name without "Service", an underscore, and the
//	                   snake_case method name
//
// Nothing is implied: a glob never grants the terminal, and no scope grants an
// owner_only service such as TokenService. Anything not granted is denied.
package auth

import (
	"fmt"
	"sort"
	"strings"
	"sync"
	"unicode"

	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/reflect/protoregistry"
	"google.golang.org/protobuf/types/descriptorpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
)

const apiPackage = "protectorate.v1"

// Method is one RPC the hub can serve.
type Method struct {
	Procedure string
	Tool      string
	Service   string
	ReadOnly  bool
	Streaming bool
	OwnerOnly bool
	// DeviceAllowed admits Full device tokens to an owner-only method.
	DeviceAllowed bool
}

// Mutating reports whether calls are audited: anything that may change state.
func (m Method) Mutating() bool { return !m.ReadOnly && !m.Streaming }

var (
	catalogOnce sync.Once
	catalog     map[string]Method
	catalogErr  error
)

// Methods returns every hub RPC keyed by Connect procedure path, e.g.
// /protectorate.v1.HubService/GetStatus. node_local services are excluded.
func Methods() (map[string]Method, error) {
	catalogOnce.Do(func() { catalog, catalogErr = buildCatalog() })
	return catalog, catalogErr
}

func buildCatalog() (map[string]Method, error) {
	out := map[string]Method{}
	var err error
	protoregistry.GlobalFiles.RangeFilesByPackage(apiPackage, func(fd protoreflect.FileDescriptor) bool {
		for i := 0; i < fd.Services().Len(); i++ {
			svc := fd.Services().Get(i)
			svcOpts, _ := svc.Options().(*descriptorpb.ServiceOptions)
			if proto.GetExtension(svcOpts, pb.E_NodeLocal).(bool) {
				continue
			}
			owner := proto.GetExtension(svcOpts, pb.E_OwnerOnly).(bool)
			for j := 0; j < svc.Methods().Len(); j++ {
				m := svc.Methods().Get(j)
				opts, _ := m.Options().(*descriptorpb.MethodOptions)
				proc := "/" + string(svc.FullName()) + "/" + string(m.Name())
				def := Method{
					Procedure: proc,
					Tool:      ToolName(svc, m),
					Service:   string(svc.Name()),
					ReadOnly:  opts.GetIdempotencyLevel() == descriptorpb.MethodOptions_NO_SIDE_EFFECTS,
					Streaming: m.IsStreamingClient() || m.IsStreamingServer(),
					OwnerOnly: owner || proto.GetExtension(opts, pb.E_OwnerOnlyRpc).(bool),

					DeviceAllowed: proto.GetExtension(opts, pb.E_DeviceAllowed).(bool),
				}
				for _, prev := range out {
					if prev.Tool == def.Tool {
						err = fmt.Errorf("tool name %q is used by both %s and %s", def.Tool, prev.Procedure, proc)
						return false
					}
				}
				out[proc] = def
			}
		}
		return true
	})
	return out, err
}

// ToolNames returns the tool names of all non-owner-only methods, sorted.
func ToolNames() ([]string, error) {
	ms, err := Methods()
	if err != nil {
		return nil, err
	}
	var names []string
	for _, m := range ms {
		if !m.OwnerOnly {
			names = append(names, m.Tool)
		}
	}
	sort.Strings(names)
	return names, nil
}

// ToolName is the canonical scope name of an RPC.
func ToolName(svc protoreflect.ServiceDescriptor, m protoreflect.MethodDescriptor) string {
	return snake(strings.TrimSuffix(string(svc.Name()), "Service")) + "_" + snake(string(m.Name()))
}

func snake(s string) string {
	r := []rune(s)
	var b strings.Builder
	for i, c := range r {
		if unicode.IsUpper(c) {
			if i > 0 && (unicode.IsLower(r[i-1]) || (unicode.IsUpper(r[i-1]) && i+1 < len(r) && unicode.IsLower(r[i+1]))) {
				b.WriteByte('_')
			}
			c = unicode.ToLower(c)
		}
		b.WriteRune(c)
	}
	return b.String()
}
