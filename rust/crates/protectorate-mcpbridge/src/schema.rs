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

#[cfg(test)]
mod tests {
    use super::*;
    use protectorate_wire::Catalog;

    fn schema_for(message: &str) -> Value {
        input_schema(Catalog::embedded().unwrap().message(message).unwrap())
    }

    #[test]
    fn scalars_and_nested_messages() {
        let schema = schema_for("protectorate.v1.GetStatusResponse");
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["hostname"], json!({"type": "string"}));
        assert_eq!(
            schema["properties"]["uptimeSeconds"],
            json!({"type": "integer"})
        );
        assert_eq!(
            schema["properties"]["startedAt"],
            json!({"type": "string", "format": "date-time"})
        );
    }

    #[test]
    fn repeated_map_enum_duration_and_nested_messages() {
        let schema = schema_for("protectorate.v1.SpawnSleeveRequest");
        assert_eq!(
            schema["properties"]["args"],
            json!({"type": "array", "items": {"type": "string"}})
        );
        assert_eq!(
            schema["properties"]["env"],
            json!({"type": "object", "additionalProperties": {"type": "string"}})
        );
        assert_eq!(
            schema["properties"]["supervised"],
            json!({"type": "boolean"})
        );
        assert_eq!(
            schema["properties"]["exitAfterIdle"],
            json!({"type": "string", "pattern": "^-?[0-9]+(\\.[0-9]+)?s$"})
        );
        let harness = &schema["properties"]["harness"];
        assert_eq!(
            harness,
            &json!({"type": "string", "enum": [
                "HARNESS_UNSPECIFIED",
                "HARNESS_CLAUDE",
                "HARNESS_CODEX",
                "HARNESS_GROK",
                "HARNESS_PI",
                "HARNESS_SHELL",
            ]})
        );
        assert_eq!(schema["properties"]["limits"]["type"], "object");
        assert_eq!(
            schema["properties"]["readOnly"],
            json!({"type": "array", "items": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "target": {"type": "string"},
                },
                "additionalProperties": false,
            }})
        );
    }

    #[test]
    fn empty_messages_and_repeated_references_stay_complete() {
        assert_eq!(
            schema_for("protectorate.v1.GetStatusRequest"),
            json!({"type": "object", "properties": {}, "additionalProperties": false})
        );
        // ModelSettings uses ModelSelection three times; the recursion guard
        // must release it after each visit instead of flattening the later ones.
        let selection = json!({
            "type": "object",
            "properties": {
                "model": {"type": "string"},
                "effort": {"type": "string"},
            },
            "additionalProperties": false,
        });
        let schema = schema_for("protectorate.v1.ModelSettings");
        for key in ["requested", "effective", "nativeDefaults"] {
            assert_eq!(schema["properties"][key], selection, "{key}");
        }
    }
}
