# Compute Worker Protocol

Bloomery's optional local compute worker runs as a child process and communicates
over framed JSON-RPC on stdin/stdout. It does not listen on a port and receives
no provider credentials.

## Envelope

Every request uses `jsonrpc: "2.0"`, `protocol_version: "1.0"`, a non-empty
string `id`, a method, and an object `params`. Frames use an ASCII
`Content-Length` header followed by a UTF-8 JSON body. Individual frames are
limited to 8 MiB.

## Operations

`hello` reports the worker version and supported operations. `shutdown` exits
after returning `{ "state": "stopped" }`. `cancel` acknowledges a task ID;
long-running cancellation is part of the supervised task integration.

`submit` accepts a `task_id`, an `operation`, and an optional `payload`.

### `train_linear_regression`

The payload contains `features` (a numeric matrix, with `null` for missing
values), `targets`, optional `feature_names`, `field_mapping`, `data_version`,
and a `split_policy`. Split policies are `random`, `group`, or `time`. Missing
feature values are imputed from training-set means, and standardization is fit
on training rows only. The result contains a versioned model artifact,
train/validation metrics, feature importance, applicability ranges, and the
exact split indices used for auditability.

### `predict_linear_regression`

The payload contains a returned `artifact` and a `features` matrix. The result
returns the artifact ID, feature names, and predictions after applying the
artifact's recorded preprocessing parameters.

## Progress and errors

Long-running submissions emit `progress` notifications before the matching
response. Errors use a stable `code` and a bounded human-readable `message`.
The current worker supports `echo`, `train_linear_regression`, and
`predict_linear_regression`; additional model families require explicit
capability and artifact-version additions.
