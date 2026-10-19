package mcpbridge

import (
	"google.golang.org/protobuf/reflect/protoreflect"
)

// InputSchema renders md as a JSON schema for its protojson form.
func InputSchema(md protoreflect.MessageDescriptor) map[string]any {
	return messageSchema(md, map[protoreflect.FullName]bool{})
}

func messageSchema(md protoreflect.MessageDescriptor, active map[protoreflect.FullName]bool) map[string]any {
	if s, ok := wellKnown(md); ok {
		return s
	}
	if active[md.FullName()] {
		return map[string]any{"type": "object"}
	}
	active[md.FullName()] = true
	defer delete(active, md.FullName())

	props := map[string]any{}
	for i := 0; i < md.Fields().Len(); i++ {
		f := md.Fields().Get(i)
		p := fieldSchema(f, active)
		if c := comment(f); c != "" {
			p["description"] = c
		}
		props[f.JSONName()] = p
	}
	return map[string]any{"type": "object", "properties": props, "additionalProperties": false}
}

func fieldSchema(f protoreflect.FieldDescriptor, active map[protoreflect.FullName]bool) map[string]any {
	if f.IsMap() {
		return map[string]any{"type": "object", "additionalProperties": singularSchema(f.MapValue(), active)}
	}
	s := singularSchema(f, active)
	if f.IsList() {
		return map[string]any{"type": "array", "items": s}
	}
	return s
}

func singularSchema(f protoreflect.FieldDescriptor, active map[protoreflect.FullName]bool) map[string]any {
	switch f.Kind() {
	case protoreflect.BoolKind:
		return map[string]any{"type": "boolean"}
	case protoreflect.StringKind:
		return map[string]any{"type": "string"}
	case protoreflect.BytesKind:
		return map[string]any{"type": "string", "contentEncoding": "base64"}
	case protoreflect.FloatKind, protoreflect.DoubleKind:
		return map[string]any{"type": "number"}
	case protoreflect.EnumKind:
		vals := f.Enum().Values()
		names := make([]string, vals.Len())
		for i := range names {
			names[i] = string(vals.Get(i).Name())
		}
		return map[string]any{"type": "string", "enum": names}
	case protoreflect.MessageKind, protoreflect.GroupKind:
		return messageSchema(f.Message(), active)
	default:
		return map[string]any{"type": "integer"}
	}
}

func wellKnown(md protoreflect.MessageDescriptor) (map[string]any, bool) {
	switch md.FullName() {
	case "google.protobuf.Timestamp":
		return map[string]any{"type": "string", "format": "date-time"}, true
	case "google.protobuf.Duration":
		return map[string]any{"type": "string", "pattern": `^-?[0-9]+(\.[0-9]+)?s$`}, true
	case "google.protobuf.FieldMask":
		return map[string]any{"type": "string"}, true
	case "google.protobuf.Struct", "google.protobuf.Any", "google.protobuf.Empty":
		return map[string]any{"type": "object"}, true
	case "google.protobuf.ListValue":
		return map[string]any{"type": "array"}, true
	case "google.protobuf.Value":
		return map[string]any{}, true
	case "google.protobuf.BoolValue":
		return map[string]any{"type": "boolean"}, true
	case "google.protobuf.StringValue", "google.protobuf.BytesValue":
		return map[string]any{"type": "string"}, true
	case "google.protobuf.FloatValue", "google.protobuf.DoubleValue":
		return map[string]any{"type": "number"}, true
	case "google.protobuf.Int32Value", "google.protobuf.UInt32Value", "google.protobuf.Int64Value", "google.protobuf.UInt64Value":
		return map[string]any{"type": "integer"}, true
	}
	return nil, false
}
