//! JSON Schema for a request message, in the proto-JSON form the bridge
//! accepts: int64 as a string or number, enums as symbolic names, bytes as
//! base64 strings, maps as objects and unknown fields rejected.
use prost_reflect::{FieldDescriptor, Kind};
use protectorate_wire::MessageDescriptor;
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

pub fn input_schema(message: MessageDescriptor) -> Value {
    let mut active = BTreeSet::new();
    message_schema(&message, &mut active)
}

fn message_schema(message: &MessageDescriptor, active: &mut BTreeSet<String>) -> Value {
    if let Some(schema) = well_known(message.full_name()) {
        return schema;
    }
    if !active.insert(message.full_name().to_owned()) {
        // A recursive message: keep the schema finite and permissive.
        return json!({"type": "object"});
    }
    let mut properties = Map::new();
    for field in message.fields() {
        let mut schema = field_schema(&field, active);
        if let Some(description) = field_description(&field) {
            schema["description"] = Value::String(description);
        }
        properties.insert(field.json_name().to_owned(), schema);
    }
    active.remove(message.full_name());
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false,
    })
}

fn field_description(field: &FieldDescriptor) -> Option<String> {
    let description = field
        .parent_file()
        .file_descriptor_proto()
        .source_code_info
        .as_ref()?
        .location
        .iter()
        .find(|location| location.path.as_slice() == field.path())?
        .leading_comments
        .as_deref()?
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!description.is_empty()).then_some(description)
}

fn field_schema(field: &FieldDescriptor, active: &mut BTreeSet<String>) -> Value {
    if field.is_map() {
        let Kind::Message(entry) = field.kind() else {
            return json!({"type": "object"});
        };
        let value = entry.map_entry_value_field();
        return json!({
            "type": "object",
            "additionalProperties": singular_schema(&value.kind(), active),
        });
    }
    let singular = singular_schema(&field.kind(), active);
    if field.is_list() {
        return json!({"type": "array", "items": singular});
    }
    singular
}

fn singular_schema(kind: &Kind, active: &mut BTreeSet<String>) -> Value {
    match kind {
        Kind::Bool => json!({"type": "boolean"}),
        Kind::String => json!({"type": "string"}),
        Kind::Bytes => json!({"type": "string", "contentEncoding": "base64"}),
        Kind::Float | Kind::Double => json!({"type": "number"}),
        Kind::Enum(enum_type) => json!({
            "type": "string",
            "enum": enum_type.values().map(|v| v.name().to_owned()).collect::<Vec<String>>(),
        }),
        Kind::Message(message) => message_schema(message, active),
        _ => json!({"type": "integer"}),
    }
}

fn well_known(full_name: &str) -> Option<Value> {
    let schema = match full_name {
        "google.protobuf.Timestamp" => json!({"type": "string", "format": "date-time"}),
        "google.protobuf.Duration" => json!({
            "type": "string",
            "pattern": "^-?[0-9]+(\\.[0-9]+)?s$",
        }),
        "google.protobuf.FieldMask"
        | "google.protobuf.StringValue"
        | "google.protobuf.BytesValue" => {
            json!({"type": "string"})
        }
        "google.protobuf.Struct" | "google.protobuf.Any" | "google.protobuf.Empty" => {
            json!({"type": "object"})
        }
        "google.protobuf.ListValue" => json!({"type": "array"}),
        "google.protobuf.Value" => json!({}),
        "google.protobuf.BoolValue" => json!({"type": "boolean"}),
        "google.protobuf.FloatValue" | "google.protobuf.DoubleValue" => json!({"type": "number"}),
        "google.protobuf.Int32Value"
        | "google.protobuf.UInt32Value"
        | "google.protobuf.Int64Value"
        | "google.protobuf.UInt64Value" => json!({"type": "integer"}),
        _ => return None,
    };
    Some(schema)
}
