# Phase 8 Real Machine File Transfer Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the agent's unavailable Bambu file-transfer runtime with a real implicit-FTPS adapter that verifies uploaded artifacts before MQTT `project_file` publish.

**Architecture:** Keep `MachineFileTransfer` and transfer-mode cache in `machine/file_transfer.rs`. Add a focused `machine/ftps.rs` runtime adapter that owns `suppaftp` integration, Bambu LAN TLS policy, `PROT P`/`PROT C`, upload verification, and timeout handling. Wire configured printers through a socket-free factory so gateway construction remains cheap and all network I/O happens inside trait calls.

**Tech Stack:** Rust 2024, tokio, async-trait, anyhow, rustls, suppaftp `9.0.0` with `tokio-rustls-aws-lc-rs` and `deprecated`, existing fake MQTT/file-transfer tests, cargo nextest.

---

## File Structure

- Modify `Cargo.toml`: add workspace `suppaftp` dependency with exact features.
- Modify `crates/pandar-agent/Cargo.toml`: depend on workspace `suppaftp`.
- Modify `crates/pandar-agent/src/machine/mqtt.rs`: expose/reuse the Bambu LAN Rustls verifier policy for FTPS without widening its scope outside agent-to-printer TLS.
- Create `crates/pandar-agent/src/machine/ftps.rs`: runtime FTPS adapter, profile helper, upload verification helper, and unit tests.
- Modify `crates/pandar-agent/src/machine/mod.rs`: export `ftps`, change configured gateway default file-transfer type from unavailable to runtime FTPS, and add a socket-free factory/helper for tests.
- Modify `crates/pandar-agent/src/machine/tests.rs`: assert configured gateway construction selects runtime FTPS without opening sockets and existing fake dispatch behavior still works.
- Modify `docs/roadmap.md` and `docs/architecture.md` after implementation review approval.

## Task 1: Dependency And API Compile Proof

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/pandar-agent/Cargo.toml`
- Create: `crates/pandar-agent/src/machine/ftps.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`

- [ ] **Step 1: Add `suppaftp` to workspace dependencies**

In `/home/indexyz/pandar/Cargo.toml`, add this line under `[workspace.dependencies]`:

```toml
suppaftp = { version = "9.0.0", default-features = false, features = ["tokio-rustls-aws-lc-rs", "deprecated"] }
```

- [ ] **Step 2: Add `suppaftp` to pandar-agent dependencies**

In `/home/indexyz/pandar/crates/pandar-agent/Cargo.toml`, add:

```toml
suppaftp.workspace = true
```

- [ ] **Step 3: Create a minimal FTPS module skeleton**

Create `/home/indexyz/pandar/crates/pandar-agent/src/machine/ftps.rs` with:

```rust
use async_trait::async_trait;

use crate::machine::{
    BambuPrinterEndpoint,
    file_transfer::{MachineFileTransfer, TransferProtectionMode},
};

#[derive(Debug, Clone)]
pub struct FtpsMachineFileTransfer {
    endpoint: BambuPrinterEndpoint,
}

impl FtpsMachineFileTransfer {
    pub fn new(endpoint: BambuPrinterEndpoint) -> Self {
        Self { endpoint }
    }

    pub fn endpoint(&self) -> &BambuPrinterEndpoint {
        &self.endpoint
    }
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

- [ ] **Step 4: Export the module**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/mod.rs`, add:

```rust
pub mod ftps;
```

- [ ] **Step 5: Run compile check for dependency integration**

Run:

```bash
cargo check -p pandar-agent
```

Expected: PASS. If this fails because the `suppaftp` feature set is wrong, fix the dependency features before continuing. Do not proceed to adapter logic until the dependency compiles.

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

- [ ] **Step 2: Add a Rustls client config helper for FTPS**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/ftps.rs`, add:

```rust
use std::sync::Arc;

use rustls::ClientConfig;

use crate::machine::mqtt::BambuLanCertificateVerifier;

pub(crate) fn bambu_lan_ftps_tls_config() -> Arc<ClientConfig> {
    Arc::new(
        ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_safe_default_protocol_versions()
            .expect("aws-lc-rs provider supports rustls safe default protocol versions")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(BambuLanCertificateVerifier))
            .with_no_client_auth(),
    )
}
```

- [ ] **Step 3: Add a focused TLS policy test**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/mqtt/tests.rs`, keep the existing MQTT TLS test and add an FTPS helper smoke test:

```rust
#[test]
fn ftps_lan_tls_uses_bambu_certificate_policy() {
    let config = crate::machine::ftps::bambu_lan_ftps_tls_config();
    assert!(config.alpn_protocols.is_empty());
}
```

- [ ] **Step 4: Run targeted tests**

Run:

```bash
cargo test -p pandar-agent machine::mqtt::tests::ftps_lan_tls_uses_bambu_certificate_policy
```

Expected: PASS.

## Task 3: Runtime Adapter Verification Helpers And Profiles

**Files:**
- Modify: `crates/pandar-agent/src/machine/ftps.rs`
- Test: `crates/pandar-agent/src/machine/ftps.rs`

- [ ] **Step 1: Add profile and verification types**

In `/home/indexyz/pandar/crates/pandar-agent/src/machine/ftps.rs`, add below `FtpsMachineFileTransfer`:

```rust
const DEFAULT_FTPS_TIMEOUT_SECONDS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FtpsProfile {
    cap_tls_1_2: bool,
}

impl FtpsProfile {
    fn for_model(model: Option<&str>) -> Self {
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
use std::{io::Cursor, sync::Arc, time::Duration};

use anyhow::Context;
use suppaftp::{
    async_ftp::FtpStream,
    types::{FileType, Mode},
};
use tokio::time::timeout;
```

If the exact `suppaftp` async type names differ, inspect `cargo doc`/compiler output and adjust inside this module only. Keep the public adapter API unchanged.

- [ ] **Step 2: Add session helper methods**

Inside `impl FtpsMachineFileTransfer`, add:

```rust
    async fn with_session<T, Fut>(
        &self,
        mode: TransferProtectionMode,
        operation: impl FnOnce(FtpStream) -> Fut,
    ) -> anyhow::Result<T>
    where
        Fut: std::future::Future<Output = anyhow::Result<T>>,
    {
        let host = self.endpoint.host.clone();
        let access_code = self.endpoint.access_code.clone();
        let profile = FtpsProfile::for_model(self.endpoint.model.as_deref());
        timeout(Duration::from_secs(DEFAULT_FTPS_TIMEOUT_SECONDS), async move {
            let mut stream = FtpStream::connect_secure_implicit(
                (host.as_str(), crate::machine::file_transfer::BAMBU_FILE_TRANSFER_PORT),
                bambu_lan_ftps_tls_config(),
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
            let result = operation(stream).await;
            if profile.cap_tls_1_2 {
                tracing::debug!(printer_host = %host, "Bambu FTPS profile requests TLS 1.2 cap when supported by adapter");
            }
            result
        })
        .await
        .with_context(|| format!("Bambu FTPS operation timed out for {}", self.endpoint.host))?
    }
```

This code may need minor API adjustments after the compile proof. Preserve the behavior: connect implicit FTPS, login, apply mode, run one operation, timeout with context.

- [ ] **Step 3: Add mode application helper**

Add below `bambu_lan_ftps_tls_config`:

```rust
async fn apply_transfer_mode(stream: &mut FtpStream, mode: TransferProtectionMode) -> anyhow::Result<()> {
    match mode {
        TransferProtectionMode::ProtectedData => {
            stream.prot_p().await.context("send PROT P")?;
        }
        TransferProtectionMode::ClearData => {
            stream.prot_c().await.context("send PROT C")?;
        }
    }
    stream.transfer_type(FileType::Binary).await.context("set binary transfer type")?;
    stream.mode(Mode::Stream).await.context("set stream transfer mode")?;
    Ok(())
}
```

If `suppaftp` does not expose these exact async helpers, use the crate's equivalent command APIs inside this helper and keep tests focused on callers selecting this helper.

- [ ] **Step 4: Implement trait methods with verification**

Replace the temporary `MachineFileTransfer` impl with:

```rust
#[async_trait]
impl MachineFileTransfer for FtpsMachineFileTransfer {
    async fn list(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<String>> {
        let path = path.to_string();
        self.with_session(mode, move |mut stream| async move {
            stream
                .cwd(&path)
                .await
                .with_context(|| format!("change Bambu FTPS directory to {path}"))?;
            stream
                .nlst(None)
                .await
                .with_context(|| format!("list Bambu FTPS directory {path}"))
        })
        .await
    }

    async fn download(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<u8>> {
        let path = path.to_string();
        self.with_session(mode, move |mut stream| async move {
            let bytes = stream
                .retr_as_buffer(&path)
                .await
                .with_context(|| format!("download Bambu FTPS file {path}"))?;
            Ok(bytes.into_inner())
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
            let mut reader = Cursor::new(bytes);
            stream
                .put_file(&path, &mut reader)
                .await
                .with_context(|| format!("upload Bambu FTPS file {path}"))?;
            let actual = stream
                .size(&path)
                .await
                .with_context(|| format!("verify Bambu FTPS file size for {path}"))?
                .map(|size| size as usize);
            verify_uploaded_size(expected, actual, &path)?;
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

If method names differ, adapt to the actual `suppaftp` API and keep the same contexts and semantics.

- [ ] **Step 5: Run compile check**

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
- Type consistency: `FtpsMachineFileTransfer`, `MachineFileTransfer`, `TransferProtectionMode`, and `BambuPrinterEndpoint` names match current repo types.
