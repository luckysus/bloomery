# Bloomery Security Model

Bloomery is a Windows-first, local-first desktop application. The security
boundary is enforced in the Rust host and the desktop bridge; the React UI is
not treated as a security boundary.

## Data and secrets

- Conversations, memories, settings, knowledge metadata, task state, and
  provider configuration are stored in the local SQLite database.
- API credentials are stored in Windows Credential Manager. SQLite stores only
  a credential reference and a generation counter.
- Logs, diagnostics, exports, protocol events, and provider errors redact
  credentials before they leave the host.
- Backups contain application data and metadata, but never credential values.

## Capability boundaries

- The React application calls Rust only through `frontend/src/bridge/desktop.ts`.
- Tauri commands are adapters. SQL and provider construction stay below the
  command boundary.
- Concrete provider implementations are created only by the provider adapter
  layer. Agent and RAG code consumes normalized capabilities.
- Tools are registered with stable IDs, versions, risk levels, concurrency
  policies, output limits, cancellation, and timeouts.
- Shell, file, network, and other side effects require the permission policy
  appropriate to the tool risk.

## Files and archives

- User paths are canonicalized and checked against authorized roots before
  reading or writing.
- Relative, device, UNC, alternate-data-stream, traversal, symlink, and
  changed-target paths are rejected.
- Domain packages and MinerU archives reject traversal, symlink entries,
  executable assets, invalid checksums, oversized entries, and expansion
  bombs before activation or extraction.
- Archive restore uses bounded entry counts, sizes, and total output size.

## Network behavior

- Provider URLs are explicit user configuration; the desktop client does not
  call the Web application's private API or require Web authentication.
- Bearer credentials are sent only to the configured provider origin.
- Redirects that could move signed or credentialed requests to another origin
  are rejected.
- Response bodies, retries, timeouts, and retry-after delays are bounded.
- MCP stdio starts from an empty environment. Only the fixed runtime allowlist
  `SystemRoot`, `windir`, `ComSpec`, `COMSPEC`, `PATHEXT`, `TEMP`, and `TMP`
  can be inherited; credential-like or arbitrary names are rejected at both
  configuration and process-spawn boundaries.

## Release gates

`scripts/security-check.ps1` runs source scans, the frontend bridge boundary
check, and the Rust security integration suites. A public release also needs
dependency, license, SBOM, signing, update, lifecycle, and clean-machine
evidence; passing this script alone is not a release approval.
