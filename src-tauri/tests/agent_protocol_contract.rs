use bloomery::agent::protocol::export;
use std::fs;
use std::path::PathBuf;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn generated_protocol_artifacts_are_fresh() {
    let root = repository_root();
    let schema_path = root.join("docs/protocol.schema.json");
    let typescript_path = root.join("frontend/src/bridge/generated/protocol.ts");

    let schema = fs::read_to_string(&schema_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", schema_path.display()));
    let typescript = fs::read_to_string(&typescript_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", typescript_path.display()));

    assert_eq!(
        schema,
        export::json_schema(),
        "protocol.schema.json is stale; run the protocol exporter"
    );
    assert_eq!(
        typescript,
        export::typescript(),
        "frontend protocol.ts is stale; run the protocol exporter"
    );
}

#[test]
fn protocol_export_is_deterministic() {
    assert_eq!(export::json_schema(), export::json_schema());
    assert_eq!(export::typescript(), export::typescript());
}

#[test]
fn envelope_schema_composes_common_and_event_fields() {
    let schema: serde_json::Value =
        serde_json::from_str(&export::json_schema()).expect("valid protocol schema");
    let envelope = &schema["$defs"]["envelope_base"];

    assert!(envelope.get("additionalProperties").is_none());
    assert_eq!(
        schema["unevaluatedProperties"],
        serde_json::Value::Bool(false)
    );
    assert_eq!(
        schema["$defs"]["agent_event_data"]["oneOf"]
            .as_array()
            .expect("event variants")
            .len(),
        15
    );
}
