# Phase 8 Real Machine File Transfer Runtime Design

Phase 8 replaces the agent's unavailable file-transfer runtime with a real Bambu-compatible FTPS adapter. It does not change hub job semantics, protobuf contracts, frontend UX, or physical print-progress reconciliation.

## Scope

- Implement a runtime adapter behind the existing `MachineFileTransfer` trait in `crates/pandar-agent`.
- Wire configured printers to use that runtime adapter instead of `UnavailableMachineFileTransfer`.
- Preserve the Phase 5 dispatch order: read artifact, upload and verify artifact, then publish MQTT `project_file`.
- Keep Bambu access codes agent-local in `PANDAR_PRINTERS`.
- Keep tests fake or local-only; Phase 8 does not require a live printer in CI.

Out of scope:

- MQTT report reconciliation and physical print progress.
- Hub job schema or command protobuf changes.
- LAN discovery.
- AMS/filament modeling.
- Virtual-printer/proxy behavior.

## Reference-Derived Requirements

The adapter should follow the behavior documented from `reference/bambuddy/backend/app/services/bambu_ftp.py`:

- Connect to implicit FTPS on port `990`.
- Login with username `bblp` and the configured printer access code.
- Use manual upload behavior compatible with 64 KiB chunks.
- Try protected data mode first.
- Support clear-data fallback for A1/A1 Mini through the existing transfer-mode policy and success-only cache.
- Verify upload completion before `project_file` publish. A successful upload is one where the transfer finishes and server-side `SIZE` matches the uploaded byte length.
- Preserve lower-level error context for auth failures, missing listener, TLS/profile mismatch, quota/full card, missing path, timeout, and partial upload.

## Dependency Choice

Use `suppaftp` as the FTP/FTPS client instead of hand-writing FTP protocol handling.

- Version: workspace dependency, current crates.io candidate `9.0.0`.
- Features: `tokio-rustls-aws-lc-rs` and `deprecated`.
  - `tokio-rustls-aws-lc-rs` gives the async Tokio client with Rustls and matches the existing agent crypto provider family.
  - `deprecated` is required because `suppaftp` exposes implicit FTPS through `connect_secure_implicit()`, and Bambu printers use implicit FTPS on port `990`.
- Rationale: it provides an async FTP/FTPS client and avoids implementing FTP command, passive data-channel, TLS wrapping, and response parsing manually.

If a missing API prevents implicit FTPS or data-protection mode support, keep the trait boundary intact and implement the smallest adapter-specific wrapper needed, but do not expose `suppaftp` types outside the runtime module.

The implementation plan must include an early compile proof for the selected `suppaftp` API before deeper adapter work.

## TLS Trust Policy

Bambu LAN printers use printer-local/self-signed certificates. Phase 3 already isolated this trust tradeoff for MQTT in a Bambu LAN Rustls verifier that accepts the printer certificate chain while preserving TLS encryption and handshake signature verification.

FTPS must use the same scoped policy:

- The custom verifier is allowed only for agent-to-Bambu LAN connections.
- It must not be reused for hub-facing HTTP/gRPC, frontend, Clerk/Logto, or arbitrary outbound TLS.
- Hostname/WebPKI validation is not required for Bambu FTPS because the printer presents local/self-signed certificates.
- Signature verification and Rustls safe protocol defaults remain enabled where the library API permits.
- The trust policy should be shared with or mirrored from the MQTT verifier to avoid two divergent Bambu LAN TLS policies.

## Components

### `machine::file_transfer`

Add a runtime type such as `FtpsMachineFileTransfer`:

- Constructed from a `BambuPrinterEndpoint`.
- Implements `MachineFileTransfer`.
- Opens a fresh FTPS session per trait call. Phase 8 does not need connection pooling.
- Construction is socket-free. The first network I/O happens inside a trait method call.
- Applies a bounded timeout around network operations.
- Uses `TransferProtectionMode` to select protected or clear data-channel behavior.
- Implements:
  - `list(path, mode)` for diagnostics.
  - `download(path, mode)` for future diagnostics.
  - `upload(path, bytes, mode)` for print dispatch.
  - `delete(path, mode)` for cleanup/diagnostics.

Add a profile helper for model-specific transport quirks:

- Default profile: no TLS cap and protected-data preferred.
- A1/A1 Mini: existing attempt order handles protected first, clear fallback.
- P2S and X2D aliases may cap TLS to 1.2 if the selected FTPS crate exposes that control cleanly. If not, keep the profile shape and document the deferred TLS-cap hook in code comments or docs without adding fake behavior.

### Runtime Wiring

`ConfiguredBambuMachineGateway::new` should build `(endpoint, mqtt, FtpsMachineFileTransfer)` for each configured printer.

`UnavailableMachineFileTransfer` can remain for tests or explicit no-runtime construction, but it must no longer be the default for configured runtime printers.

### Upload Verification

`upload` must fail if server-side verification proves a partial upload:

1. Transfer all bytes to the remote path.
2. Ask the server for `SIZE remote_path`.
3. Succeed only when the returned size equals `bytes.len()`.
4. Return an error with expected and actual sizes when they differ.
5. Return an error with full context when `SIZE` fails after upload, because Phase 8 must not publish `project_file` for an unverified artifact.

This is intentionally stricter than Bambuddy's tolerance path because Pandar currently has no physical-print reconciliation phase to recover from a bad dispatch.

### Protected And Clear Data Modes

`TransferProtectionMode` must map to explicit FTPS data-channel behavior after login:

- `ProtectedData` sends `PROT P` or uses the `suppaftp` equivalent before list/download/upload/delete.
- `ClearData` sends `PROT C` or uses the `suppaftp` equivalent before list/download/upload/delete, while the implicit FTPS control channel remains TLS-encrypted.
- A1/A1 Mini fallback remains controlled by the existing `run_with_transfer_mode` attempt order and success-only cache.
- If `suppaftp` does not expose a typed `PROT` helper for async Rustls streams, the adapter may use the minimal raw-command path needed to issue `PROT P` / `PROT C` inside the adapter module. That raw-command path must stay private and covered by a local test or compile proof.

## Error Handling

- Add context at every boundary: connect, login, mode selection, upload transfer, size verification, list, download, and delete.
- Log or return full error chains with `{err:#}` where errors cross runtime/task boundaries.
- Do not swallow low-level FTP/TLS/socket errors.
- Do not add legacy fallbacks that silently publish MQTT after upload verification fails.

## Tests

Add targeted tests that do not open live Bambu sockets:

- Runtime gateway construction no longer uses `UnavailableMachineFileTransfer` for configured printers. This may be covered by testing the factory/helper that builds file-transfer adapters rather than opening sockets.
- Configured gateway construction is socket-free; tests should be able to build a configured gateway without a live printer or network listener.
- Dependency/API compile proof covers implicit FTPS construction and protected/clear data mode selection with the chosen `suppaftp` feature set.
- Upload verification succeeds only when the reported server size equals uploaded bytes.
- Upload verification fails on mismatched size and preserves expected/actual size context.
- Upload verification fails when the server cannot report size.
- Mode policy and cache tests from Phase 3 remain green.
- Print gateway fake tests continue to prove upload-before-publish and no-publish-on-upload-failure.

If the concrete `suppaftp` adapter is hard to unit test without sockets, isolate its verification decision in a pure helper and cover that helper directly. The adapter itself can be smoke-compiled through trait construction.

## Documentation

Update:

- `docs/roadmap.md` to mark Phase 8 complete and move Immediate Next to Phase 9.
- `docs/architecture.md` to state the runtime FTPS adapter is now wired for configured agents and upload verification gates MQTT publish.

## Acceptance Criteria

- `pandar-agent` configured with `PANDAR_PRINTERS` uses a real FTPS adapter for print file upload.
- Configured gateway construction does not open Bambu sockets; network I/O starts on trait method calls.
- The print gateway does not publish MQTT `project_file` unless file upload and size verification succeed.
- The selected `suppaftp` dependency and feature set compile with implicit FTPS and protected/clear data mode support or a private adapter wrapper that proves equivalent behavior.
- Runtime FTPS errors preserve context useful for operators.
- Existing fake tests still verify no-publish-on-upload-failure.
- Relevant docs are updated.
- `cargo fmt`, `cargo clippy`, and `cargo nextest run --manifest-path "Cargo.toml" --workspace` pass, or any unavailable command is explicitly reported.
