package main

import (
	_ "embed"
	"encoding/json"
	"net/http"
	"strings"

	"google.golang.org/protobuf/reflect/protoreflect"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protodesc"
	"google.golang.org/protobuf/types/descriptorpb"
)

//go:embed image.binpb
var image []byte

func mcpTools(w http.ResponseWriter, _ *http.Request) {
	var set descriptorpb.FileDescriptorSet
	if err := proto.Unmarshal(image, &set); err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	files, err := protodesc.NewFiles(&set)
	if err != nil {
		http.Error(w, err.Error(), 500)
		return
	}
	var tools []map[string]any
	files.RangeFiles(func(fd protoreflect.FileDescriptor) bool {
		if fd.Package() != "protectorate.v1" {
			return true
		}
		for i := 0; i < fd.Services().Len(); i++ {
			svc := fd.Services().Get(i)
			for j := 0; j < svc.Methods().Len(); j++ {
				m := svc.Methods().Get(j)
				if m.IsStreamingServer() || m.IsStreamingClient() {
					continue
				}
				tools = append(tools, map[string]any{
					"name":        toSnake(strings.TrimSuffix(string(svc.Name()), "Service")) + "_" + toSnake(string(m.Name())),
					"description": comment(m),
					"inputSchema": schema(m.Input()),
				})
			}
		}
		return true
	})
	w.Header().Set("Content-Type", "application/json")
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	enc.Encode(map[string]any{"tools": tools})
}

func comment(d protoreflect.Descriptor) string {
	return strings.TrimSpace(d.ParentFile().SourceLocations().ByDescriptor(d).LeadingComments)
}

func schema(md protoreflect.MessageDescriptor) map[string]any {
	props := map[string]any{}
	for i := 0; i < md.Fields().Len(); i++ {
		f := md.Fields().Get(i)
		p := map[string]any{"type": jsonType(f)}
		if c := comment(f); c != "" {
			p["description"] = c
		}
		if f.IsList() {
			p = map[string]any{"type": "array", "items": p}
		}
		props[f.JSONName()] = p
	}
	return map[string]any{"type": "object", "properties": props, "additionalProperties": false}
}

func jsonType(f protoreflect.FieldDescriptor) string {
	switch f.Kind() {
	case protoreflect.StringKind, protoreflect.BytesKind, protoreflect.Int64Kind, protoreflect.Uint64Kind:
		return "string"
	case protoreflect.BoolKind:
		return "boolean"
	case protoreflect.MessageKind, protoreflect.GroupKind:
		return "object"
	case protoreflect.FloatKind, protoreflect.DoubleKind:
		return "number"
	default:
		return "integer"
	}
}

func toSnake(s string) string {
	var b strings.Builder
	for i, r := range s {
		if r >= 'A' && r <= 'Z' {
			if i > 0 {
				b.WriteByte('_')
			}
			r += 'a' - 'A'
		}
		b.WriteRune(r)
	}
	return b.String()
}
