# Phase 8 Real Machine File Transfer Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the agent's unavailable Bambu file-transfer runtime with a real implicit-FTPS adapter that verifies uploaded artifacts before MQTT `project_file` publish.

**Architecture:** Keep `MachineFileTransfer` and transfer-mode cache in `machine/file_transfer.rs`. Add a focused `machine/ftps.rs` runtime adapter that owns `suppaftp` integration, Bambu LAN TLS policy, `PROT P`/`PROT C`, upload verification, and timeout handling. Wire configured printers through a socket-free factory so gateway construction remains cheap and all network I/O happens inside trait calls.

**Tech Stack:** Rust 2024, tokio, async-trait, anyhow, rustls, tokio-rustls, suppaftp `9.0.0` with `tokio-rustls-aws-lc-rs` and `deprecated`, existing fake MQTT/file-transfer tests, cargo nextest.

---

## File Structure

- Modify `Cargo.toml`: add workspace `suppaftp` and `tokio-rustls` dependencies with exact features, and enable direct Tokio IO/time helpers needed by the adapter.
- Modify `crates/pandar-agent/Cargo.toml`: depend on workspace `suppaftp` and `tokio-rustls`.
- Modify `crates/pandar-agent/src/machine/mqtt.rs`: expose/reuse the Bambu LAN Rustls verifier policy for FTPS without widening its scope outside agent-to-printer TLS.
- Create `crates/pandar-agent/src/machine/ftps.rs`: runtime FTPS adapter, profile helper, upload verification helper, and unit tests.
- Modify `crates/pandar-agent/src/machine/mod.rs`: export `ftps`, change configured gateway default file-transfer type from unavailable to runtime FTPS, and add a socket-free factory/helper for tests.
- Modify `crates/pandar-agent/src/machine/tests.rs`: assert configured gateway construction selects runtime FTPS without opening sockets and existing fake dispatch behavior still works.
- Modify `docs/roadmap.md` and `docs/architecture.md` after implementation review approval.

## Task 1: Dependency And Real API Compile Proof

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/pandar-agent/Cargo.toml`
- Create: `crates/pandar-agent/src/machine/ftps.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`

- [ ] **Step 1: Add FTPS dependencies and Tokio IO features**

In `/home/indexyz/pandar/Cargo.toml`, add these lines under `[workspace.dependencies]`:

```toml
suppaftp = { version = "9.0.0", default-features = false, features = ["tokio-rustls-aws-lc-rs", "deprecated"] }
tokio-rustls = { version = "0.26", default-features = false, features = ["aws-lc-rs", "tls12", "logging"] }
```

In the existing workspace `tokio` dependency, add direct `io-util` and `time` features so the adapter can use `tokio::io::AsyncReadExt`, `tokio::io::AsyncWriteExt`, and `tokio::time::timeout` without relying on transitive feature unification:

```toml
tokio = { version = "1.48.0", features = ["io-util", "macros", "net", "rt-multi-thread", "signal", "time"] }
```

- [ ] **Step 2: Add FTPS dependencies to pandar-agent**

In `/home/indexyz/pandar/crates/pandar-agent/Cargo.toml`, add:

```toml
suppaftp.workspace = true
tokio-rustls.workspace = true
```

- [ ] **Step 3: Create the FTPS module with a real suppaftp API compile proof**

Create `/home/indexyz/pandar/crates/pandar-agent/src/machine/ftps.rs` with:

```rust
use async_trait::async_trait;
use suppaftp::{
    Status,
    tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream},
    types::FileType,
};
use tokio::io::AsyncWriteExt;

use crate::machine::{
    BambuPrinterEndpoint,
    file_transfer::{
        BAMBU_FILE_TRANSFER_CHUNK_SIZE, BAMBU_FILE_TRANSFER_PORT,
        BAMBU_FILE_TRANSFER_USERNAME, MachineFileTransfer, TransferProtectionMode,
    },
};

#[derive(Debug, Clone)]
pub struct FtpsMachineFileTransfer {
    endpoint: BambuPrinterEndpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FtpsProfile {
    cap_tls_1_2: bool,
}

impl FtpsProfile {
    pub(crate) fn for_model(model: Option<&str>) -> Self {
        let Some(model) = model else {
            return Self { cap_tls_1_2: false };
        };
        let key = model.trim().to_ascii_uppercase();
        let key = match key.as_str() {
            "N7" => "P2S",
            "N6" => "X2D",
            _ => key.as_str(),
        };
        Self {
            cap_tls_1_2: matches!(key, "P2S" | "X2D"),
        }
    }
}

impl FtpsMachineFileTransfer {
    pub fn new(endpoint: BambuPrinterEndpoint) -> Self {
        Self { endpoint }
    }

    pub fn endpoint(&self) -> &BambuPrinterEndpoint {
        &self.endpoint
    }
}

#[allow(dead_code)]
async fn suppaftp_api_compile_proof(
    host: String,
    access_code: String,
    connector: AsyncRustlsConnector,
) -> suppaftp::FtpResult<()> {
    let mut stream = AsyncRustlsFtpStream::connect_secure_implicit(
        (host.as_str(), BAMBU_FILE_TRANSFER_PORT),
        connector,
        host.as_str(),
    )
    .await?;
    stream
        .login(BAMBU_FILE_TRANSFER_USERNAME, access_code.as_str())
        .await?;
    stream.custom_command("PBSZ 0", &[Status::CommandOk]).await?;
    stream.custom_command("PROT P", &[Status::CommandOk]).await?;
    stream.custom_command("PROT C", &[Status::CommandOk]).await?;
    stream.transfer_type(FileType::Binary).await?;
    let mut data = stream.put_with_stream("pandar-api-proof.3mf").await?;
    for chunk in b"proof".chunks(BAMBU_FILE_TRANSFER_CHUNK_SIZE) {
        data.write_all(chunk)
            .await
            .map_err(suppaftp::FtpError::ConnectionError)?;
    }
    stream.finalize_put_stream(data).await?;
    let _size = stream.size("pandar-api-proof.3mf").await?;
    stream.rm("pandar-api-proof.3mf").await?;
    Ok(())
}

#[async_trait]
impl MachineFileTransfer for FtpsMachineFileTransfer {
    async fn list(&self, _path: &str, _mode: TransferProtectionMode) -> anyhow::Result<Vec<String>> {
        anyhow::bail!("FTPS list compile proof is not implemented yet")
    }

    async fn download(&self, _path: &str, _mode: TransferProtectionMode) -> anyhow::Result<Vec<u8>> {
        anyhow::bail!("FTPS download compile proof is not implemented yet")
    }

    async fn upload(
        &self,
        _path: &str,
        _bytes: &[u8],
        _mode: TransferProtectionMode,
    ) -> anyhow::Result<()> {
        anyhow::bail!("FTPS upload compile proof is not implemented yet")
    }

    async fn delete(&self, _path: &str, _mode: TransferProtectionMode) -> anyhow::Result<()> {
        anyhow::bail!("FTPS delete compile proof is not implemented yet")
    }
}
```

This function is not called by tests because it would open a socket, but Rust still type-checks its body. It proves the selected crate feature set exposes implicit FTPS, login, `PBSZ`, protected/clear data commands, binary type selection, streamed upload, `SIZE`, and delete before deeper adapter work starts.

- [ ] **Step 4: Export the module**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/mod.rs`, add:

```rust
pub mod ftps;
```

- [ ] **Step 5: Run compile check for the real API proof**

Run:

```bash
cargo check -p pandar-agent
```

Expected: PASS. If this fails because a type or feature is wrong, fix the dependency features or the private compile-proof function before continuing. Do not proceed to adapter logic until the real `suppaftp` implicit-FTPS/upload/`PROT` path type-checks.

## Task 2: Shared Bambu LAN TLS Policy

**Files:**
- Modify: `crates/pandar-agent/src/machine/mqtt.rs`
- Modify: `crates/pandar-agent/src/machine/ftps.rs`
- Test: `crates/pandar-agent/src/machine/mqtt/tests.rs`

- [ ] **Step 1: Make the verifier type reusable inside `machine`**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/mqtt.rs`, change:

```rust
struct BambuLanCertificateVerifier;
```

to:

```rust
pub(crate) struct BambuLanCertificateVerifier;
```

Keep the existing verifier implementation unchanged.

- [ ] **Step 2: Add Rustls client config helpers for FTPS profiles**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/ftps.rs`, add:

```rust
use std::sync::Arc;

use rustls::{ClientConfig, version};

use crate::machine::mqtt::BambuLanCertificateVerifier;

pub(crate) fn bambu_lan_ftps_tls_config(profile: FtpsProfile) -> Arc<ClientConfig> {
    let builder =
        ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into());
    let builder = if profile.cap_tls_1_2 {
        builder
            .with_protocol_versions(&[&version::TLS12])
            .expect("aws-lc-rs provider supports TLS 1.2 for Bambu FTPS profiles")
    } else {
        builder
            .with_safe_default_protocol_versions()
            .expect("aws-lc-rs provider supports rustls safe default protocol versions")
    };

    Arc::new(
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(BambuLanCertificateVerifier))
            .with_no_client_auth(),
    )
}

fn bambu_lan_ftps_connector(profile: FtpsProfile) -> AsyncRustlsConnector {
    tokio_rustls::TlsConnector::from(bambu_lan_ftps_tls_config(profile)).into()
}

pub(crate) fn bambu_lan_ftps_tls_config_for_default_profile() -> Arc<ClientConfig> {
    bambu_lan_ftps_tls_config(FtpsProfile::for_model(None))
}
```

- [ ] **Step 3: Add a focused TLS policy test**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/mqtt/tests.rs`, keep the existing MQTT TLS test and add an FTPS helper smoke test:

```rust
#[test]
fn ftps_lan_tls_default_profile_config_constructs() {
    let config = crate::machine::ftps::bambu_lan_ftps_tls_config_for_default_profile();
    assert!(config.alpn_protocols.is_empty());
}
```

Also add a local FTPS profile test in `/home/indexyz/pandar/crates/pandar-agent/src/machine/ftps.rs` after `FtpsProfile` exists:

```rust
#[test]
fn p2s_profile_builds_tls_config() {
    let config = bambu_lan_ftps_tls_config(FtpsProfile::for_model(Some("P2S")));
    assert!(config.alpn_protocols.is_empty());
    assert_eq!(config.max_fragment_size, None);
}
```

These smoke tests verify the profile-specific helpers construct valid configs without pretending to perform a TLS handshake in unit tests.

- [ ] **Step 4: Run targeted tests**

Run:

```bash
cargo test -p pandar-agent machine::mqtt::tests::ftps_lan_tls_default_profile_config_constructs
```

Expected: PASS.

## Task 3: Runtime Adapter Verification Helpers And Profiles

**Files:**
- Modify: `crates/pandar-agent/src/machine/ftps.rs`
- Test: `crates/pandar-agent/src/machine/ftps.rs`

- [ ] **Step 1: Add profile and verification types**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/ftps.rs`, add below `impl FtpsMachineFileTransfer`:

```rust
const DEFAULT_FTPS_TIMEOUT_SECONDS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadVerification {
    Verified,
}

fn verify_uploaded_size(expected: usize, actual: Option<usize>, path: &str) -> anyhow::Result<UploadVerification> {
    let actual = actual.ok_or_else(|| {
        anyhow::anyhow!("FTPS upload verification failed for {path}: server did not return SIZE")
    })?;
    if actual != expected {
        anyhow::bail!(
            "FTPS upload verification failed for {path}: expected {expected} bytes, printer reported {actual} bytes"
        );
    }
    Ok(UploadVerification::Verified)
}
```

- [ ] **Step 2: Add unit tests for profile aliases and size verification**

At the bottom of `/home/indexyz/pandar/crates/pandar-agent/src/machine/ftps.rs`, add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_caps_tls_for_known_aliases_only() {
        assert!(!FtpsProfile::for_model(None).cap_tls_1_2);
        assert!(!FtpsProfile::for_model(Some("P1S")).cap_tls_1_2);
        assert!(FtpsProfile::for_model(Some("P2S")).cap_tls_1_2);
        assert!(FtpsProfile::for_model(Some("N7")).cap_tls_1_2);
        assert!(FtpsProfile::for_model(Some("X2D")).cap_tls_1_2);
        assert!(FtpsProfile::for_model(Some("N6")).cap_tls_1_2);
    }

    #[test]
    fn upload_size_verification_accepts_exact_match() {
        assert_eq!(
            verify_uploaded_size(42, Some(42), "plate.3mf").unwrap(),
            UploadVerification::Verified
        );
    }

    #[test]
    fn upload_size_verification_rejects_mismatch() {
        let err = verify_uploaded_size(42, Some(7), "plate.3mf").unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("plate.3mf"));
        assert!(message.contains("expected 42 bytes"));
        assert!(message.contains("reported 7 bytes"));
    }

    #[test]
    fn upload_size_verification_rejects_missing_size() {
        let err = verify_uploaded_size(42, None, "plate.3mf").unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("plate.3mf"));
        assert!(message.contains("server did not return SIZE"));
    }
}
```

- [ ] **Step 3: Run targeted helper tests**

Run:

```bash
cargo test -p pandar-agent machine::ftps::tests::
```

Expected: PASS.

## Task 4: Implement `suppaftp` Session Operations

**Files:**
- Modify: `crates/pandar-agent/src/machine/ftps.rs`

- [ ] **Step 1: Add imports for runtime session work**

At the top of `/home/indexyz/pandar/crates/pandar-agent/src/machine/ftps.rs`, expand imports to include:

```rust
use std::{sync::Arc, time::Duration};

use anyhow::Context;
use suppaftp::{
    Status,
    tokio::AsyncRustlsFtpStream,
    types::FileType,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
```

Use `AsyncRustlsFtpStream` as the session type. Do not use the no-TLS `AsyncFtpStream` alias.

- [ ] **Step 2: Add session helper methods**

Inside `impl FtpsMachineFileTransfer`, add:

```rust
    async fn with_session<T, Fut>(
        &self,
        mode: TransferProtectionMode,
        operation: impl FnOnce(AsyncRustlsFtpStream) -> Fut,
    ) -> anyhow::Result<T>
    where
        Fut: std::future::Future<Output = anyhow::Result<T>>,
    {
        let host = self.endpoint.host.clone();
        let access_code = self.endpoint.access_code.clone();
        let profile = FtpsProfile::for_model(self.endpoint.model.as_deref());
        timeout(Duration::from_secs(DEFAULT_FTPS_TIMEOUT_SECONDS), async move {
            let connector = bambu_lan_ftps_connector(profile);
            let mut stream = AsyncRustlsFtpStream::connect_secure_implicit(
                (host.as_str(), crate::machine::file_transfer::BAMBU_FILE_TRANSFER_PORT),
                connector,
                host.as_str(),
            )
            .await
            .with_context(|| format!("connect implicit FTPS to {host}:990"))?;
            stream
                .login(crate::machine::file_transfer::BAMBU_FILE_TRANSFER_USERNAME, &access_code)
                .await
                .with_context(|| format!("login to Bambu FTPS at {host} as bblp"))?;
            apply_transfer_mode(&mut stream, mode)
                .await
                .with_context(|| format!("set Bambu FTPS transfer mode {mode:?} for {host}"))?;
            operation(stream).await
        })
        .await
        .with_context(|| format!("Bambu FTPS operation timed out for {}", self.endpoint.host))?
    }
```

This code uses the profile-specific connector so P2S/X2D aliases cap the Rustls client to TLS 1.2, while other models use Rustls safe defaults.

- [ ] **Step 3: Add mode application helper**

Add below `bambu_lan_ftps_tls_config`:

```rust
async fn apply_transfer_mode(
    stream: &mut AsyncRustlsFtpStream,
    mode: TransferProtectionMode,
) -> anyhow::Result<()> {
    stream
        .custom_command("PBSZ 0", &[Status::CommandOk])
        .await
        .context("send PBSZ 0")?;
    match mode {
        TransferProtectionMode::ProtectedData => {
            stream
                .custom_command("PROT P", &[Status::CommandOk])
                .await
                .context("send PROT P")?;
        }
        TransferProtectionMode::ClearData => {
            stream
                .custom_command("PROT C", &[Status::CommandOk])
                .await
                .context("send PROT C")?;
        }
    }
    stream.transfer_type(FileType::Binary).await.context("set binary transfer type")?;
    Ok(())
}
```

`suppaftp` does not expose typed async `PROT` helpers; the private adapter helper uses `custom_command` and keeps the raw commands out of the public API.

- [ ] **Step 4: Add a 64 KiB upload helper**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/ftps.rs`, add:

```rust
async fn upload_in_bambu_chunks(
    stream: &mut AsyncRustlsFtpStream,
    path: &str,
    bytes: &[u8],
) -> anyhow::Result<()> {
    let mut data = stream
        .put_with_stream(path)
        .await
        .with_context(|| format!("start Bambu FTPS upload for {path}"))?;
    for chunk in bytes.chunks(BAMBU_FILE_TRANSFER_CHUNK_SIZE) {
        data.write_all(chunk)
            .await
            .with_context(|| format!("write Bambu FTPS upload chunk for {path}"))?;
    }
    stream
        .finalize_put_stream(data)
        .await
        .with_context(|| format!("finalize Bambu FTPS upload for {path}"))
}
```

This intentionally avoids `put_file` because Phase 8 requires Bambuddy-compatible manual upload behavior with 64 KiB chunking.

- [ ] **Step 5: Implement trait methods with verification**

Replace the temporary `MachineFileTransfer` impl with:

```rust
#[async_trait]
impl MachineFileTransfer for FtpsMachineFileTransfer {
    async fn list(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<String>> {
        let path = path.to_string();
        self.with_session(mode, move |mut stream| async move {
            stream
                .nlst(Some(&path))
                .await
                .with_context(|| format!("list Bambu FTPS directory {path}"))
        })
        .await
    }

    async fn download(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<u8>> {
        let path = path.to_string();
        self.with_session(mode, move |mut stream| async move {
            stream
                .retr(&path, |mut data| {
                    Box::pin(async move {
                        let mut bytes = Vec::new();
                        data.read_to_end(&mut bytes)
                            .await
                            .map_err(suppaftp::FtpError::ConnectionError)?;
                        Ok((bytes, data))
                    })
                })
                .await
                .with_context(|| format!("download Bambu FTPS file {path}"))
        })
        .await
    }

    async fn upload(
        &self,
        path: &str,
        bytes: &[u8],
        mode: TransferProtectionMode,
    ) -> anyhow::Result<()> {
        let path = path.to_string();
        let bytes = bytes.to_vec();
        self.with_session(mode, move |mut stream| async move {
            let expected = bytes.len();
            upload_in_bambu_chunks(&mut stream, &path, &bytes).await?;
            let actual = stream
                .size(&path)
                .await
                .with_context(|| format!("verify Bambu FTPS file size for {path}"))?;
            verify_uploaded_size(expected, Some(actual), &path)?;
            Ok(())
        })
        .await
    }

    async fn delete(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<()> {
        let path = path.to_string();
        self.with_session(mode, move |mut stream| async move {
            stream
                .rm(&path)
                .await
                .with_context(|| format!("delete Bambu FTPS file {path}"))
        })
        .await
    }
}
```

This keeps publish gating strict: `upload` returns `Ok(())` only after the streamed transfer finalizes and `SIZE` exactly equals `bytes.len()`.

- [ ] **Step 6: Run compile check**

Run:

```bash
cargo check -p pandar-agent
```

Expected: PASS. Fix only API mismatches inside `ftps.rs` before moving on.

## Task 5: Runtime Gateway Wiring

**Files:**
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Test: `crates/pandar-agent/src/machine/tests.rs`

- [ ] **Step 1: Change configured gateway default type**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/mod.rs`, import `FtpsMachineFileTransfer`:

```rust
use ftps::FtpsMachineFileTransfer;
```

Change the gateway struct default:

```rust
pub struct ConfiguredBambuMachineGateway<T, F = FtpsMachineFileTransfer> {
```

- [ ] **Step 2: Wire runtime FTPS adapters in `new`**

Replace the current `ConfiguredBambuMachineGateway::new` map body:

```rust
.map(|(endpoint, mqtt)| (endpoint, mqtt, UnavailableMachineFileTransfer))
```

with:

```rust
.map(|(endpoint, mqtt)| {
    let transfer = FtpsMachineFileTransfer::new(endpoint.clone());
    (endpoint, mqtt, transfer)
})
```

- [ ] **Step 3: Add a socket-free construction test**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/tests.rs`, add:

```rust
#[test]
fn configured_gateway_construction_uses_runtime_ftps_without_network_io() {
    let mqtt = FakeMqttTransport::default();
    let endpoint = endpoint("SERIAL1");
    let gateway = ConfiguredBambuMachineGateway::new(
        vec![(endpoint.clone(), mqtt)],
        Duration::from_secs(1),
    );

    assert_eq!(gateway.configured_printer_count(), 1);
}
```

- [ ] **Step 4: Add a count helper used by the test**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/mod.rs`, inside `impl<T, F> ConfiguredBambuMachineGateway<T, F>`, add:

```rust
    pub fn configured_printer_count(&self) -> usize {
        self.printers.len()
    }
```

This helper is intentionally narrow and socket-free.

- [ ] **Step 5: Run targeted gateway tests**

Run:

```bash
cargo test -p pandar-agent machine::tests::configured_gateway_construction_uses_runtime_ftps_without_network_io machine::tests::configured_print_project_file_uploads_and_publishes_project_file machine::tests::configured_print_project_file_does_not_publish_when_upload_fails
```

If cargo rejects multiple filters, run these three tests as separate commands. Expected: PASS.

## Task 6: Docs And Roadmap

**Files:**
- Modify: `docs/roadmap.md`
- Modify: `docs/architecture.md`

- [ ] **Step 1: Update Phase 8 roadmap status**

In `/home/indexyz/pandar/docs/roadmap.md`, under `## Completed`, add:

```markdown
- Added Phase 8 real Bambu FTPS runtime upload for configured agents, including implicit FTPS port 990, Bambu LAN TLS policy, protected/clear data mode selection, and upload size verification before MQTT publish.
```

In `## Phase 8: Real Machine File Transfer Runtime`, change the bullet list to completed tense or add a completion summary:

```markdown
- Completed runtime implicit FTPS adapter behind `MachineFileTransfer`.
- Completed socket-free configured gateway construction with network I/O deferred to trait calls.
- Completed upload size verification before MQTT `project_file` publish.
- Completed no-live-printer tests for verification helpers and gateway publish gating.
```

In `## Immediate Next`, replace the Phase 8 start bullets with:

```markdown
- Start Phase 9 so print job state is driven by MQTT report reconciliation instead of dispatch result alone.
- Keep Phase 9 scoped to physical print progress/completion/failure and normalized machine events.
- Do Phase 10 before browser-facing multi-tenant installs so Clerk/Logto users are authenticated by Rust and authorized through local tenant memberships.
```

- [ ] **Step 2: Update architecture runtime notes**

In `/home/indexyz/pandar/docs/architecture.md`, update the Phase 5 agent paragraph that currently says the default runtime file-transfer adapter is unavailable. Replace it with:

```markdown
- Configured runtime agents use the Bambu FTPS adapter for machine file upload. The adapter uses implicit FTPS on port `990`, Bambu LAN TLS policy, protected/clear data mode selection, and server-side size verification before MQTT `project_file` publish. Tests still use fake file-transfer transports and do not require live Bambu sockets.
```

- [ ] **Step 3: Run markdown/diff checks**

Run:

```bash
git diff --check
```

Expected: PASS.

## Task 7: Final Verification

**Files:**
- All changed files.

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt
```

Expected: exits 0.

- [ ] **Step 2: Clippy**

Run:

```bash
cargo clippy --workspace --all-targets
```

Expected: exits 0.

- [ ] **Step 3: Workspace tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: exits 0. If `cargo nextest` is unavailable, report that explicitly and run `cargo test --workspace` as the fallback.

- [ ] **Step 4: Generated protobuf check**

Run:

```bash
git status --short
```

Expected: no generated `.pb.rs` or `.tonic.rs` files are staged or modified.

- [ ] **Step 5: Final diff review**

Run:

```bash
git diff --stat
git diff --check
```

Expected: only Phase 8 files/docs changed and diff check passes.

## Self-Review

- Spec coverage: Tasks cover dependency selection, Bambu LAN TLS policy, runtime adapter, PROT P/C selection, upload verification, socket-free construction, docs, and final verification.
- Placeholder scan: No task uses TBD/TODO/fill-in placeholders. API mismatch handling is bounded to `ftps.rs` because the dependency API must be compile-proven before adapter logic proceeds.
- Type consistency: `FtpsMachineFileTransfer`, `MachineFileTransfer`, `TransferProtectionMode`, and `BambuPrinterEndpoint` names match current repo types. A throwaway compile check in `/tmp/pandar-suppaftp-proof` verified the planned `suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream}`, `custom_command("PBSZ 0" / "PROT P" / "PROT C")`, `put_with_stream`, `finalize_put_stream`, `size`, `rm`, and `retr` API usage before this plan review.
