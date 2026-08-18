# Bloomery Agent Event Protocol

This document defines the public event stream emitted by the Bloomery local-first
agent runtime. The wire contract is protocol version `1` and is shared by the
Rust runtime, the Tauri bridge, and any external replay client.

The machine-readable schema is `docs/protocol.schema.json`. TypeScript consumers
should import the generated definitions from
`frontend/src/bridge/generated/protocol.ts`. Both files are generated from the
Rust exporter:

```powershell
Set-Location src-tauri
cargo run --bin export_protocol
```

The exporter also supports a CI-friendly check:

```powershell
cargo run --bin export_protocol -- --check
```

## Compatibility

`protocol_version` is `1` for this contract. A version is increased when an
existing field is removed, renamed, changes type or meaning, or when envelope
semantics become incompatible. Consumers must reject a version they do not
understand instead of guessing.

Within version `1`:

- Existing event names and required fields are stable.
- New optional fields may be added to an existing payload.
- New event names are additive, but clients must ignore unknown event names at
  their boundary so that a newer producer can replay to an older UI.
- Producers must continue to emit valid events for the declared version.
- The generated TypeScript file and JSON Schema are artifacts; do not edit them
  directly. Update the Rust protocol and rerun the exporter.

## Envelope

Every event has this shape:

```json
{
  "protocol_version": 1,
  "event_id": "11111111-1111-4111-8111-111111111111",
  "run_id": "22222222-2222-4222-8222-222222222222",
  "conversation_id": "33333333-3333-4333-8333-333333333333",
  "sequence": 1,
  "timestamp": "2026-08-05T08:00:00Z",
  "type": "run_created",
  "data": {
    "state": "created",
    "user_message_id": "44444444-4444-4444-8444-444444444444"
  }
}
```

`event_id`, `run_id`, `conversation_id`, and all payload IDs are UUID strings.
`timestamp` is an RFC 3339 UTC timestamp. `sequence` is monotonically
increasing within a run and starts at `1`; it is not a global sequence.

The runtime persists an event and advances the run cursor in one SQLite
transaction before publishing it to the UI. A lost UI notification therefore
does not mean a lost event. An event that is visible in a replay is committed.

The Tauri live channel for `AgentEventEnvelope` values is `agent-event`. The
legacy `desktop-agent-delta` channel remains available for existing chat views;
it carries only assistant text fragments and is not a replacement for replay.

## Event Types

| Type | Meaning |
| --- | --- |
| `run_created` | The user message and its run were created atomically. |
| `run_state_changed` | The run moved between non-terminal states. |
| `message_delta` | A streamed text fragment for a user, assistant, tool, or system message. |
| `message_completed` | The complete message content currently available. `partial` is true when generation stopped early. |
| `tool_requested` | The model requested a registered tool with validated arguments. |
| `tool_started` | Tool execution began. |
| `tool_progress` | A tool reported bounded progress from `0` to `100`. |
| `tool_completed` | Tool execution ended with `succeeded`, `failed`, or `cancelled`. |
| `permission_requested` | A confirmation-required or dangerous tool is waiting for user approval. |
| `permission_resolved` | The user or policy resolved a permission request. |
| `evidence_attached` | The run attached an evidence pack and its citation numbers. |
| `usage_updated` | Provider token usage was updated. |
| `task_progress` | A durable background task reported its state and progress. |
| `run_completed` | The run reached `completed`, `cancelled`, `failed`, or `interrupted`. |
| `error_raised` | A structured error was surfaced. `fatal` means the current run cannot continue. |

The exact payload fields and enum values are defined by the generated artifacts;
the table is only a human-readable index.

## Ordering and Replay

Events must be applied in ascending `sequence` order for one `run_id`. The
runtime does not promise ordering across different runs or conversations.

The replay cursor is exclusive. A client that has applied sequence `12` asks
for events with `sequence > 12`:

```json
{
  "runId": "22222222-2222-4222-8222-222222222222",
  "afterSequence": 12
}
```

The desktop bridge exposes this through the `replay_agent_run` command. Passing
`0` replays the complete run. The response is ordered by sequence. A client
should keep its last applied sequence and request only the missing suffix after
reconnect or a dropped Tauri notification.

Replay reconstructs UI state; it does not re-execute the model or tools. A
terminal run is replay-only. Replaying the same suffix more than once must be
idempotent in the UI, keyed by `event_id` or `sequence`.

## Run Lifecycle

The normal state path is:

```text
created -> preparing -> generating -> executing_tools -> verifying -> completing -> completed
```

Permission-gated work pauses at `awaiting_permission`. A run may finish as
`cancelled`, `failed`, or `interrupted` from a non-terminal state. Terminal
runs never transition again and always have one terminal `run_completed` event.

`run_state_changed` records both `previous` and `current`. Consumers should
validate the transition for their local projection and fall back to replay if
they receive a gap or an invalid transition.

## Cancellation and Recovery

Cancellation is represented by a state change and a terminal completion event;
there is no separate cancellation event. When generation is cancelled, the
runtime may emit a partial `message_completed` before:

```json
{
  "type": "run_completed",
  "data": {
    "outcome": "cancelled",
    "assistant_message_id": "44444444-4444-4444-8444-444444444444"
  }
}
```

On application restart:

- A run waiting for permission remains waiting and its unresolved permission
  requests are replayed.
- A tool checkpoint is resumed only when the tool is explicitly declared
  idempotent by the runtime.
- An interrupted generation or unknown tool state is completed as
  `interrupted`; it is not silently replayed as a successful answer.
- `retry_agent_run` creates a new run from a terminal source run. The source
  events remain immutable.

The bridge commands are `replay_agent_run`, `cancel_agent_run`,
`retry_agent_run`, and `recover_agent_runs`. Commands are workspace-scoped by
the desktop application and never accept a Web login or cloud user ID.

## Permissions

Tools are classified as `automatic`, `confirmation_required`, or `dangerous`.
Only registered tools can produce a `tool_requested` event. Tool arguments are
validated before the event is persisted. A permission request contains a
stable `permission_id`, the related `tool_call_id`, the risk level, and a
human-readable reason and summary.

The UI must display the requested operation and wait for a decision. It must not
infer approval from a model message. `allow_once`, `allow_session`, and
`allow_always` are explicit decisions; `deny` prevents the call. Permission
intent supplied by a model is never trusted as approval.

## Assistant Response Metadata

Assistant messages may persist a `response_json` object alongside the message
body. This is not a replacement for the event stream; it is a compact replay
summary for the chat UI.

Current metadata can include:

- `memory.selected`: confirmed memories selected for this run, grouped with a
  `layer` such as `user_profile`, `domain_memory`, `task_memory`, or
  `reflection_memory`;
- `memory.selected_count`: the number of selected memories shown in the compact
  chat status row;
- `context_packet.memory_index`: intentionally empty in normal responses so the
  complete memory catalog is not injected into prompts;
- `skills.enabled_versions`: enabled Skill versions and content hashes;
- `skills.loaded`: Skill name, version, content hash, and trigger reason for
  Skills loaded into this run;
- `tool_calls`: compact tool audit entries with `id`, `tool_id`, `name`,
  `status`, and optional error metadata; tool outputs are not copied here;
- `evidence` and `follow_up_questions`: UI-facing summaries derived from the
  local run.

Clients must treat `response_json` as display metadata. It must not approve
permissions, execute tools, or write memory by itself.

## Errors

Errors use this shape:

```json
{
  "type": "error_raised",
  "data": {
    "error": {
      "code": "provider_quota_exceeded",
      "category": "quota",
      "message": "The configured provider quota is exhausted.",
      "retryable": false,
      "details": null
    },
    "fatal": true
  }
}
```

The category is one of `configuration`, `authentication`, `quota`, `network`,
`parsing`, `indexing`, `model_capability`, `tool_permission`, `mcp`,
`database`, or `internal`. `code` is stable enough for programmatic handling;
`message` is for display and diagnostics. `details` is untrusted structured
data and may be null. Secrets must never be placed in the message or details.

Retryable errors may be offered to the user as a retry. A fatal error ends the
current run and should be followed by a terminal `run_completed` event with a
`failed` outcome.

## TypeScript Usage

The generated contract is discriminated by `type`:

```ts
import type { AgentEventEnvelope } from "./generated/protocol";

export function applyAgentEvent(event: AgentEventEnvelope) {
  switch (event.type) {
    case "message_delta":
      return event.data.delta;
    case "tool_progress":
      return event.data.progress;
    case "run_completed":
      return event.data.outcome;
    default:
      return undefined;
  }
}
```

Unknown protocol versions or event types should be surfaced as a recoverable
projection error and followed by a full replay when possible. Never execute a
tool, write a file, or change permission state merely because an event was
received by the renderer.

## Local Compute Worker Protocol

The optional steel compute worker is a separate local process. It communicates
with the Rust host over stdin/stdout using UTF-8 JSON frames. It does not open a
network listener, own the Bloomery database, or receive provider credentials.

Each frame uses a byte-accurate `Content-Length` header followed by a blank line
and exactly that many UTF-8 JSON bytes:

```text
Content-Length: 146\r\n
\r\n
{"jsonrpc":"2.0","protocol_version":"1.0",...}
```

The current Worker protocol version is `1.0`. Requests contain a non-empty
string `id`, a method, and an object `params` value. Responses contain the same
`id` and exactly one of `result` or `error`; progress is an id-less
notification:

```json
{
  "jsonrpc": "2.0",
  "protocol_version": "1.0",
  "method": "progress",
  "params": {
    "task_id": "job-1",
    "progress": 42,
    "stage": "training"
  }
}
```

The bootstrap methods are:

| Method | Purpose |
| --- | --- |
| `hello` | Negotiate protocol and list supported operations. |
| `submit` | Submit a validated local task. |
| `cancel` | Request cancellation for a task ID. |
| `shutdown` | Stop the worker and close the stdio session. |

Unknown methods, unsupported operations, malformed parameters, protocol version
mismatches, oversized frames, and truncated input are errors. The host must
validate paths and task limits before sending them. Worker output is treated as
untrusted: artifact paths, hashes, model metadata, and results require schema
and integrity validation before persistence or activation.

The Rust implementation lives in `src-tauri/src/compute/`; the reference
Python package lives in `compute-worker/`. The protocol layer currently exposes
only an `echo` operation for contract testing. Scientific training, ONNX
inference, and constrained optimization are separate release tasks and must not
be inferred from the bootstrap capability list.
