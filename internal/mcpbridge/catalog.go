// Package mcpbridge exposes the hub API as MCP tools, generated at runtime
// from the proto service descriptors.
package mcpbridge

import (
	_ "embed"
	"fmt"
	"path"
	"sort"
	"strings"

	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/types/descriptorpb"

	pb "github.com/hotschmoe/protectorate/gen/protectorate/v1"
	"github.com/hotschmoe/protectorate/internal/auth"
)

// image is a buf image with source info, written by `make gen`. Generated Go
// descriptors carry no comments, so descriptions come from here.
//
//go:embed image.binpb
var image []byte

const apiPackage = "protectorate.v1"

// ToolDef is one unary RPC presented as a tool.
type ToolDef struct {
	Name        string
	Description string
	Method      protoreflect.MethodDescriptor
	ReadOnly    bool
	Destructive bool
}

// Path is the Connect procedure path, e.g. /protectorate.v1.HubService/GetStatus.
func (t ToolDef) Path() string {
	return "/" + string(t.Method.Parent().FullName()) + "/" + string(t.Method.Name())
}

// Catalog lists every unary hub RPC in the embedded image, sorted by tool
// name. node_local and owner_only services and methods and server-streaming RPCs are skipped.
func Catalog() ([]ToolDef, error) {
	var set descriptorpb.FileDescriptorSet
	if err := proto.Unmarshal(image, &set); err != nil {
		return nil, fmt.Errorf("decode embedded image: %w", err)
	}
	files, err := protodesc.NewFiles(&set)
	if err != nil {
		return nil, fmt.Errorf("build descriptors: %w", err)
	}
	var tools []ToolDef
	seen := map[string]string{}
	var walkErr error
	files.RangeFiles(func(fd protoreflect.FileDescriptor) bool {
		if string(fd.Package()) != apiPackage {
			return true
		}
		for i := 0; i < fd.Services().Len(); i++ {
			svc := fd.Services().Get(i)
			svcOpts, _ := svc.Options().(*descriptorpb.ServiceOptions)
			if proto.GetExtension(svcOpts, pb.E_NodeLocal).(bool) || proto.GetExtension(svcOpts, pb.E_OwnerOnly).(bool) {
				continue
			}
			for j := 0; j < svc.Methods().Len(); j++ {
				m := svc.Methods().Get(j)
				opts, _ := m.Options().(*descriptorpb.MethodOptions)
				if m.IsStreamingClient() || m.IsStreamingServer() || proto.GetExtension(opts, pb.E_OwnerOnlyRpc).(bool) {
					continue
				}
				name := auth.ToolName(svc, m)
				if prev, dup := seen[name]; dup {
					walkErr = fmt.Errorf("tool name %q is used by both %s and %s", name, prev, m.FullName())
					return false
				}
				seen[name] = string(m.FullName())
				tools = append(tools, ToolDef{
					Name:        name,
					Description: comment(m),
					Method:      m,
					ReadOnly:    opts.GetIdempotencyLevel() == descriptorpb.MethodOptions_NO_SIDE_EFFECTS,
					Destructive: proto.GetExtension(opts, pb.E_Destructive).(bool),
				})
			}
		}
		return true
	})
	if walkErr != nil {
		return nil, walkErr
	}
	sort.Slice(tools, func(i, j int) bool { return tools[i].Name < tools[j].Name })
	return tools, nil
}

// comment returns the leading proto comment of d as a single paragraph line.
func comment(d protoreflect.Descriptor) string {
	c := d.ParentFile().SourceLocations().ByDescriptor(d).LeadingComments
	return strings.Join(strings.Fields(c), " ")
}

// Filter selects which tools are served. With no Allow patterns only
// read-only tools pass; Allow replaces that default with glob patterns over
// tool names, and Deny always removes matches.
type Filter struct {
	Allow []string
	Deny  []string
}

func (f Filter) Validate() error {
	for _, p := range append(append([]string{}, f.Allow...), f.Deny...) {
		if _, err := path.Match(p, ""); err != nil {
			return fmt.Errorf("bad pattern %q: %w", p, err)
		}
	}
	return nil
}

func (f Filter) Permits(t ToolDef) bool {
	if matchAny(f.Deny, t.Name) {
		return false
	}
	if len(f.Allow) == 0 {
		return t.ReadOnly
	}
	return matchAny(f.Allow, t.Name)
}

func matchAny(patterns []string, name string) bool {
	for _, p := range patterns {
		if ok, _ := path.Match(p, name); ok {
			return true
		}
	}
	return false
}
