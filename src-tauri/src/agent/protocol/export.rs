use serde_json::{json, Map, Value};

const PROTOCOL_SCHEMA_ID: &str = "https://bloomery.dev/protocol/v1/event.schema.json";

pub fn json_schema() -> String {
    let mut definitions = Map::new();

    definitions.insert(
        "agent_error_category".to_string(),
        string_enum(&[
            "configuration",
            "authentication",
            "quota",
            "network",
            "parsing",
            "indexing",
            "model_capability",
            "tool_permission",
            "mcp",
            "database",
            "internal",
        ]),
    );
    definitions.insert(
        "agent_error".to_string(),
        object(
            json!({
                "code": string_schema(),
                "category": ref_schema("agent_error_category"),
                "message": string_schema(),
                "retryable": {"type": "boolean"},
                "details": optional(any_schema()),
            }),
            &["code", "category", "message", "retryable", "details"],
        ),
    );
    definitions.insert(
        "agent_message_role".to_string(),
        string_enum(&["user", "assistant", "tool", "system"]),
    );
    definitions.insert(
        "agent_run_state".to_string(),
        string_enum(&[
            "created",
            "preparing",
            "generating",
            "awaiting_permission",
            "executing_tools",
            "verifying",
            "completing",
            "completed",
            "cancelled",
            "failed",
            "interrupted",
        ]),
    );
    definitions.insert(
        "permission_decision".to_string(),
        string_enum(&["allow_once", "allow_session", "allow_always", "deny"]),
    );
    definitions.insert(
        "permission_risk".to_string(),
        string_enum(&["automatic", "confirmation_required", "dangerous"]),
    );
    definitions.insert(
        "run_outcome".to_string(),
        string_enum(&["completed", "cancelled", "failed", "interrupted"]),
    );
    definitions.insert(
        "task_progress_state".to_string(),
        string_enum(&[
            "queued",
            "running",
            "waiting_external",
            "paused",
            "completed",
            "failed",
            "cancelled",
            "interrupted",
        ]),
    );
    definitions.insert(
        "tool_outcome".to_string(),
        string_enum(&["succeeded", "failed", "cancelled"]),
    );

    definitions.insert(
        "run_created".to_string(),
        object(
            json!({
                "state": ref_schema("agent_run_state"),
                "user_message_id": uuid_schema(),
            }),
            &["state", "user_message_id"],
        ),
    );
    definitions.insert(
        "run_state_changed".to_string(),
        object(
            json!({
                "previous": ref_schema("agent_run_state"),
                "current": ref_schema("agent_run_state"),
                "reason": optional(string_schema()),
            }),
            &["previous", "current", "reason"],
        ),
    );
    definitions.insert(
        "message_delta".to_string(),
        object(
            json!({
                "message_id": uuid_schema(),
                "role": ref_schema("agent_message_role"),
                "delta": string_schema(),
            }),
            &["message_id", "role", "delta"],
        ),
    );
    definitions.insert(
        "message_completed".to_string(),
        object(
            json!({
                "message_id": uuid_schema(),
                "role": ref_schema("agent_message_role"),
                "content": string_schema(),
                "partial": {"type": "boolean"},
            }),
            &["message_id", "role", "content", "partial"],
        ),
    );
    definitions.insert(
        "tool_requested".to_string(),
        object(
            json!({
                "tool_call_id": uuid_schema(),
                "tool_id": string_schema(),
                "tool_name": string_schema(),
                "arguments": any_schema(),
            }),
            &["tool_call_id", "tool_id", "tool_name", "arguments"],
        ),
    );
    definitions.insert(
        "tool_started".to_string(),
        object(json!({"tool_call_id": uuid_schema()}), &["tool_call_id"]),
    );
    definitions.insert(
        "tool_progress".to_string(),
        object(
            json!({
                "tool_call_id": uuid_schema(),
                "progress": bounded_integer(0, 100),
                "message": optional(string_schema()),
            }),
            &["tool_call_id", "progress", "message"],
        ),
    );
    definitions.insert(
        "tool_completed".to_string(),
        object(
            json!({
                "tool_call_id": uuid_schema(),
                "outcome": ref_schema("tool_outcome"),
                "output": optional(any_schema()),
                "error": optional(ref_schema("agent_error")),
            }),
            &["tool_call_id", "outcome", "output", "error"],
        ),
    );
    definitions.insert(
        "permission_requested".to_string(),
        object(
            json!({
                "permission_id": uuid_schema(),
                "tool_call_id": uuid_schema(),
                "risk": ref_schema("permission_risk"),
                "reason": string_schema(),
                "summary": string_schema(),
            }),
            &["permission_id", "tool_call_id", "risk", "reason", "summary"],
        ),
    );
    definitions.insert(
        "permission_resolved".to_string(),
        object(
            json!({
                "permission_id": uuid_schema(),
                "decision": ref_schema("permission_decision"),
            }),
            &["permission_id", "decision"],
        ),
    );
    definitions.insert(
        "evidence_attached".to_string(),
        object(
            json!({
                "evidence_pack_id": uuid_schema(),
                "citation_numbers": array_schema(bounded_integer(0, u32::MAX as u64)),
            }),
            &["evidence_pack_id", "citation_numbers"],
        ),
    );
    definitions.insert(
        "usage_updated".to_string(),
        object(
            json!({
                "prompt_tokens": integer_schema(),
                "completion_tokens": integer_schema(),
                "total_tokens": integer_schema(),
            }),
            &["prompt_tokens", "completion_tokens", "total_tokens"],
        ),
    );
    definitions.insert(
        "task_progress".to_string(),
        object(
            json!({
                "task_id": uuid_schema(),
                "kind": string_schema(),
                "state": ref_schema("task_progress_state"),
                "progress": bounded_integer(0, 100),
            }),
            &["task_id", "kind", "state", "progress"],
        ),
    );
    definitions.insert(
        "run_completed".to_string(),
        object(
            json!({
                "outcome": ref_schema("run_outcome"),
                "assistant_message_id": optional(uuid_schema()),
            }),
            &["outcome", "assistant_message_id"],
        ),
    );
    definitions.insert(
        "error_raised".to_string(),
        object(
            json!({
                "error": ref_schema("agent_error"),
                "fatal": {"type": "boolean"},
            }),
            &["error", "fatal"],
        ),
    );

    let event_types = [
        ("run_created", "run_created"),
        ("run_state_changed", "run_state_changed"),
        ("message_delta", "message_delta"),
        ("message_completed", "message_completed"),
        ("tool_requested", "tool_requested"),
        ("tool_started", "tool_started"),
        ("tool_progress", "tool_progress"),
        ("tool_completed", "tool_completed"),
        ("permission_requested", "permission_requested"),
        ("permission_resolved", "permission_resolved"),
        ("evidence_attached", "evidence_attached"),
        ("usage_updated", "usage_updated"),
        ("task_progress", "task_progress"),
        ("run_completed", "run_completed"),
        ("error_raised", "error_raised"),
    ];
    let variants = event_types
        .iter()
        .map(|(event_type, definition)| event_variant(event_type, definition))
        .collect::<Vec<_>>();
    definitions.insert("agent_event_data".to_string(), json!({"oneOf": variants}));
    definitions.insert(
        "envelope_base".to_string(),
        open_object(
            json!({
                "protocol_version": {"const": 1},
                "event_id": uuid_schema(),
                "run_id": uuid_schema(),
                "conversation_id": uuid_schema(),
                "sequence": integer_schema(),
                "timestamp": {
                    "type": "string",
                    "format": "date-time",
                },
            }),
            &[
                "protocol_version",
                "event_id",
                "run_id",
                "conversation_id",
                "sequence",
                "timestamp",
            ],
        ),
    );

    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": PROTOCOL_SCHEMA_ID,
        "title": "Bloomery Agent Event Envelope",
        "description": "Versioned events emitted by the local-first Bloomery agent runtime.",
        "type": "object",
        "allOf": [
            {"$ref": "#/$defs/envelope_base"},
            {"$ref": "#/$defs/agent_event_data"},
        ],
        "unevaluatedProperties": false,
        "$defs": definitions,
    });

    format!(
        "{}\n",
        serde_json::to_string_pretty(&schema).expect("protocol schema is serializable")
    )
}

pub fn typescript() -> String {
    TYPESCRIPT.to_string()
}

fn any_schema() -> Value {
    json!({})
}

fn array_schema(items: Value) -> Value {
    json!({"type": "array", "items": items})
}

fn bounded_integer(minimum: u64, maximum: u64) -> Value {
    json!({
        "type": "integer",
        "minimum": minimum,
        "maximum": maximum,
    })
}

fn event_variant(event_type: &str, definition: &str) -> Value {
    json!({
        "type": "object",
        "required": ["type", "data"],
        "properties": {
            "type": {"const": event_type},
            "data": ref_schema(definition),
        },
    })
}

fn integer_schema() -> Value {
    json!({"type": "integer", "minimum": 0})
}

fn object(properties: Value, required: &[&str]) -> Value {
    let mut schema = open_object(properties, required);
    schema["additionalProperties"] = json!(false);
    schema
}

fn open_object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

fn optional(schema: Value) -> Value {
    json!({"anyOf": [schema, {"type": "null"}]})
}

fn ref_schema(name: &str) -> Value {
    json!({"$ref": format!("#/$defs/{name}")})
}

fn string_enum(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn string_schema() -> Value {
    json!({"type": "string"})
}

fn uuid_schema() -> Value {
    json!({"type": "string", "format": "uuid"})
}

const TYPESCRIPT: &str = r#"/* AUTO-GENERATED by `cargo run --bin export_protocol`. DO NOT EDIT. */

export const AGENT_PROTOCOL_VERSION = 1 as const;

export type UUID = string;
export type IsoTimestamp = string;
export type JsonValue = unknown;

export type AgentErrorCategory =
  | "configuration"
  | "authentication"
  | "quota"
  | "network"
  | "parsing"
  | "indexing"
  | "model_capability"
  | "tool_permission"
  | "mcp"
  | "database"
  | "internal";

export interface AgentError {
  code: string;
  category: AgentErrorCategory;
  message: string;
  retryable: boolean;
  details: JsonValue | null;
}

export type AgentRunState =
  | "created"
  | "preparing"
  | "generating"
  | "awaiting_permission"
  | "executing_tools"
  | "verifying"
  | "completing"
  | "completed"
  | "cancelled"
  | "failed"
  | "interrupted";

export type AgentMessageRole = "user" | "assistant" | "tool" | "system";
export type ToolOutcome = "succeeded" | "failed" | "cancelled";
export type PermissionRisk = "automatic" | "confirmation_required" | "dangerous";
export type PermissionDecision = "allow_once" | "allow_session" | "allow_always" | "deny";
export type TaskProgressState =
  | "queued"
  | "running"
  | "waiting_external"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted";
export type RunOutcome = "completed" | "cancelled" | "failed" | "interrupted";

export interface RunCreated {
  state: AgentRunState;
  user_message_id: UUID;
}

export interface RunStateChanged {
  previous: AgentRunState;
  current: AgentRunState;
  reason: string | null;
}

export interface MessageDelta {
  message_id: UUID;
  role: AgentMessageRole;
  delta: string;
}

export interface MessageCompleted {
  message_id: UUID;
  role: AgentMessageRole;
  content: string;
  partial: boolean;
}

export interface ToolRequested {
  tool_call_id: UUID;
  tool_id: string;
  tool_name: string;
  arguments: JsonValue;
}

export interface ToolStarted {
  tool_call_id: UUID;
}

export interface ToolProgress {
  tool_call_id: UUID;
  progress: number;
  message: string | null;
}

export interface ToolCompleted {
  tool_call_id: UUID;
  outcome: ToolOutcome;
  output: JsonValue | null;
  error: AgentError | null;
}

export interface PermissionRequested {
  permission_id: UUID;
  tool_call_id: UUID;
  risk: PermissionRisk;
  reason: string;
  summary: string;
}

export interface PermissionResolved {
  permission_id: UUID;
  decision: PermissionDecision;
}

export interface EvidenceAttached {
  evidence_pack_id: UUID;
  citation_numbers: number[];
}

export interface UsageUpdated {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

export interface TaskProgress {
  task_id: UUID;
  kind: string;
  state: TaskProgressState;
  progress: number;
}

export interface RunCompleted {
  outcome: RunOutcome;
  assistant_message_id: UUID | null;
}

export interface ErrorRaised {
  error: AgentError;
  fatal: boolean;
}

export type AgentEventType =
  | "run_created"
  | "run_state_changed"
  | "message_delta"
  | "message_completed"
  | "tool_requested"
  | "tool_started"
  | "tool_progress"
  | "tool_completed"
  | "permission_requested"
  | "permission_resolved"
  | "evidence_attached"
  | "usage_updated"
  | "task_progress"
  | "run_completed"
  | "error_raised";

export type AgentEventData =
  | { type: "run_created"; data: RunCreated }
  | { type: "run_state_changed"; data: RunStateChanged }
  | { type: "message_delta"; data: MessageDelta }
  | { type: "message_completed"; data: MessageCompleted }
  | { type: "tool_requested"; data: ToolRequested }
  | { type: "tool_started"; data: ToolStarted }
  | { type: "tool_progress"; data: ToolProgress }
  | { type: "tool_completed"; data: ToolCompleted }
  | { type: "permission_requested"; data: PermissionRequested }
  | { type: "permission_resolved"; data: PermissionResolved }
  | { type: "evidence_attached"; data: EvidenceAttached }
  | { type: "usage_updated"; data: UsageUpdated }
  | { type: "task_progress"; data: TaskProgress }
  | { type: "run_completed"; data: RunCompleted }
  | { type: "error_raised"; data: ErrorRaised };

export interface AgentEventEnvelopeBase {
  protocol_version: typeof AGENT_PROTOCOL_VERSION;
  event_id: UUID;
  run_id: UUID;
  conversation_id: UUID;
  sequence: number;
  timestamp: IsoTimestamp;
}

export type AgentEventEnvelope = AgentEventEnvelopeBase & AgentEventData;
export type AgentEvent = AgentEventEnvelope;
"#;
