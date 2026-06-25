# Phase 3 Bambu Machine Transport Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the agent-side Bambu MQTT and file-transfer transport boundaries in milestones, then route `RefreshPrinters` through the new non-networked gateway path.

**Architecture:** Keep all machine protocol code in `crates/pandar-agent/src/machine/`. Start with pure models, builders, traits, and fake transports so tests never open Bambu sockets. Agent command handling delegates to an async gateway that emits current protobuf `PrinterSnapshot` events before command result events.

**Tech Stack:** Rust 2024, tokio, tonic/prost, serde/serde_json, anyhow, clap, rumqttc.

---

## File Structure

- `crates/pandar-agent/Cargo.toml`: add `serde` and `serde_json` in Task 1; add `async-trait` and `rumqttc` in Task 2 when those traits/adapters are introduced.
- `Cargo.toml`: add workspace dependency entries required by the agent crate.
- `crates/pandar-agent/src/lib.rs`: expose modules, parse `PANDAR_PRINTERS`, and keep gRPC connection setup.
- `crates/pandar-agent/src/commands.rs`: command handling, gateway injection, refresh event sequencing, and command tests. This split keeps `lib.rs` below the 400 LOC project limit.
- `crates/pandar-agent/src/machine/mod.rs`: endpoint/config/snapshot types and public module exports.
- `crates/pandar-agent/src/machine/mqtt.rs`: MQTT constants, topics, commands, payload builders, report mapping, transport trait, fake transport tests.
- `crates/pandar-agent/src/machine/file_transfer.rs`: FTPS-derived constants, mode policy, mode cache, operation request types, trait, tests.
- `README.md`: document `PANDAR_PRINTERS` and no-network test policy.
- `docs/architecture.md`: document Phase 3 machine transport boundaries.
- `docs/roadmap.md`: mark completed Phase 3 milestones and update Immediate Next.

## Task 1: MQTT Model And Payload Builders

**Files:**

- Create: `crates/pandar-agent/src/machine/mod.rs`
- Create: `crates/pandar-agent/src/machine/mqtt.rs`
- Modify: `crates/pandar-agent/src/lib.rs`
- Modify: `crates/pandar-agent/Cargo.toml`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add serde dependencies**

Add workspace dependencies:

```toml
serde = { version = "1.0.228", features = ["derive"] }
serde_json = "1.0.145"
```

Add to `crates/pandar-agent/Cargo.toml`:

```toml
serde.workspace = true
serde_json.workspace = true
```

- [ ] **Step 2: Add the module exports**

In `crates/pandar-agent/src/lib.rs`, add:

```rust
pub mod machine;
```

Create `crates/pandar-agent/src/machine/mod.rs`:

```rust
pub mod mqtt;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct BambuPrinterEndpoint {
    pub host: String,
    pub serial: String,
    pub access_code: String,
    pub model: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSnapshot {
    pub serial: String,
    pub name: String,
    pub state: String,
}
```

- [ ] **Step 3: Write MQTT builder tests first**

Create `crates/pandar-agent/src/machine/mqtt.rs` with tests first. The expected tests:

```rust
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::machine::BambuPrinterEndpoint;

    fn endpoint() -> BambuPrinterEndpoint {
        BambuPrinterEndpoint {
            host: "192.0.2.10".to_string(),
            serial: "01S00EXAMPLE".to_string(),
            access_code: "12345678".to_string(),
            model: Some("A1 Mini".to_string()),
            name: Some("garage-a1".to_string()),
        }
    }

    #[test]
    fn topics_match_bambu_reference_shape() {
        let topics = BambuMqttTopics::for_serial("01S00EXAMPLE");

        assert_eq!(topics.report, "device/01S00EXAMPLE/report");
        assert_eq!(topics.request, "device/01S00EXAMPLE/request");
    }

    #[test]
    fn constants_match_bambu_defaults() {
        assert_eq!(BAMBU_MQTT_PORT, 8883);
        assert_eq!(BAMBU_MQTT_USERNAME, "bblp");
        assert_eq!(BAMBU_MQTT_QOS, 1);
    }

    #[test]
    fn pushall_payload_matches_reference() {
        assert_eq!(
            BambuMqttCommand::RequestPushAll.payload(),
            json!({"pushing": {"command": "pushall"}})
        );
    }

    #[test]
    fn basic_print_control_payloads_match_reference() {
        assert_eq!(
            BambuMqttCommand::PausePrint.payload(),
            json!({"print": {"command": "pause", "sequence_id": "0"}})
        );
        assert_eq!(
            BambuMqttCommand::ResumePrint.payload(),
            json!({"print": {"command": "resume", "sequence_id": "0"}})
        );
        assert_eq!(
            BambuMqttCommand::StopPrint.payload(),
            json!({"print": {"command": "stop", "sequence_id": "0"}})
        );
    }

    #[test]
    fn print_speed_is_limited_to_reference_modes() {
        assert_eq!(
            BambuMqttCommand::SetPrintSpeed(PrintSpeed::new(4).unwrap()).payload(),
            json!({"print": {"command": "print_speed", "param": "4", "sequence_id": "0"}})
        );
        assert!(PrintSpeed::new(0).is_err());
        assert!(PrintSpeed::new(5).is_err());
    }

    #[test]
    fn raw_json_payload_is_preserved() {
        let payload = json!({"print": {"command": "custom", "sequence_id": "9"}});
        assert_eq!(BambuMqttCommand::RawJson(payload.clone()).payload(), payload);
    }

    #[test]
    fn project_file_payload_reserves_dispatch_identity_and_flags() {
        let payload = BambuMqttCommand::ProjectFile(ProjectFileCommand {
            filename: "job.3mf".to_string(),
            plate_id: 2,
            task_id: "task-1".to_string(),
            subtask_id: "subtask-1".to_string(),
            use_ams: true,
            flow_cali: true,
            timelapse: false,
        })
        .payload();

        assert_eq!(
            payload,
            json!({
                "print": {
                    "command": "project_file",
                    "sequence_id": "20000",
                    "param": "Metadata/plate_2.gcode",
                    "url": "ftp://job.3mf",
                    "file": "job.3mf",
                    "task_id": "task-1",
                    "subtask_id": "subtask-1",
                    "use_ams": true,
                    "flow_cali": true,
                    "timelapse": false
                }
            })
        );
    }

    #[test]
    fn report_maps_to_snapshot_with_config_identity() {
        let report = json!({"print": {"gcode_state": "RUNNING"}});

        assert_eq!(
            snapshot_from_report(&endpoint(), &report),
            MachineSnapshot {
                serial: "01S00EXAMPLE".to_string(),
                name: "garage-a1".to_string(),
                state: "RUNNING".to_string(),
            }
        );
    }

    #[test]
    fn report_state_falls_back_through_string_candidates() {
        assert_eq!(
            snapshot_from_report(&endpoint(), &json!({"print": {"state": "READY"}})).state,
            "READY"
        );
        assert_eq!(
            snapshot_from_report(&endpoint(), &json!({"state": "IDLE"})).state,
            "IDLE"
        );
        assert_eq!(
            snapshot_from_report(&endpoint(), &json!({"print": {"gcode_state": 123, "state": "READY"}})).state,
            "READY"
        );
        assert_eq!(
            snapshot_from_report(&endpoint(), &json!({"print": {"gcode_state": 123}})).state,
            "unknown"
        );
    }
}
```

- [ ] **Step 4: Run tests to confirm they fail**

Run:

```bash
cargo test -p pandar-agent machine::mqtt --no-fail-fast
```

Expected: compilation fails because the MQTT types are not implemented yet.

- [ ] **Step 5: Implement MQTT models and builders**

Implement in `crates/pandar-agent/src/machine/mqtt.rs`:

```rust
use anyhow::{anyhow, bail};
use serde_json::{Value, json};

use crate::machine::{BambuPrinterEndpoint, MachineSnapshot};

pub const BAMBU_MQTT_PORT: u16 = 8883;
pub const BAMBU_MQTT_USERNAME: &str = "bblp";
pub const BAMBU_MQTT_QOS: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BambuMqttTopics {
    pub report: String,
    pub request: String,
}

impl BambuMqttTopics {
    pub fn for_serial(serial: &str) -> Self {
        Self {
            report: format!("device/{serial}/report"),
            request: format!("device/{serial}/request"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrintSpeed(u8);

impl PrintSpeed {
    pub fn new(mode: u8) -> anyhow::Result<Self> {
        if !(1..=4).contains(&mode) {
            bail!("invalid Bambu print speed mode {mode}; expected 1..=4");
        }
        Ok(Self(mode))
    }

    pub fn as_u8(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFileCommand {
    pub filename: String,
    pub plate_id: u32,
    pub task_id: String,
    pub subtask_id: String,
    pub use_ams: bool,
    pub flow_cali: bool,
    pub timelapse: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BambuMqttCommand {
    RequestPushAll,
    PausePrint,
    ResumePrint,
    StopPrint,
    SetPrintSpeed(PrintSpeed),
    RawJson(Value),
    ProjectFile(ProjectFileCommand),
}

impl BambuMqttCommand {
    pub fn payload(&self) -> Value {
        match self {
            Self::RequestPushAll => json!({"pushing": {"command": "pushall"}}),
            Self::PausePrint => json!({"print": {"command": "pause", "sequence_id": "0"}}),
            Self::ResumePrint => json!({"print": {"command": "resume", "sequence_id": "0"}}),
            Self::StopPrint => json!({"print": {"command": "stop", "sequence_id": "0"}}),
            Self::SetPrintSpeed(speed) => {
                json!({"print": {"command": "print_speed", "param": speed.as_u8().to_string(), "sequence_id": "0"}})
            }
            Self::RawJson(payload) => payload.clone(),
            Self::ProjectFile(command) => json!({
                "print": {
                    "command": "project_file",
                    "sequence_id": "20000",
                    "param": format!("Metadata/plate_{}.gcode", command.plate_id),
                    "url": format!("ftp://{}", command.filename),
                    "file": command.filename,
                    "task_id": command.task_id,
                    "subtask_id": command.subtask_id,
                    "use_ams": command.use_ams,
                    "flow_cali": command.flow_cali,
                    "timelapse": command.timelapse,
                }
            }),
        }
    }
}

pub fn snapshot_from_report(endpoint: &BambuPrinterEndpoint, report: &Value) -> MachineSnapshot {
    let state = report
        .pointer("/print/gcode_state")
        .or_else(|| report.pointer("/print/state"))
        .or_else(|| report.pointer("/state"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    MachineSnapshot {
        serial: endpoint.serial.clone(),
        name: endpoint.name.clone().unwrap_or_else(|| endpoint.serial.clone()),
        state: state.to_string(),
    }
}
```

- [ ] **Step 6: Verify Task 1**

Run:

```bash
cargo test -p pandar-agent machine::mqtt --no-fail-fast
cargo fmt --check -p pandar-agent
```

Expected: tests pass and formatting is clean.

## Task 2: MQTT Transport Trait, Runtime Adapter, And Refresh Gateway

**Files:**

- Modify: `crates/pandar-agent/src/machine/mqtt.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`
- Modify: `crates/pandar-agent/Cargo.toml`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add async-trait and rumqttc dependencies**

Use `async-trait` for object-safe async transport and gateway traits, and `rumqttc` for the isolated runtime adapter. Add workspace dependencies:

```toml
async-trait = "0.1.89"
rumqttc = "0.25.1"
```

Add to `crates/pandar-agent/Cargo.toml`:

```toml
async-trait.workspace = true
rumqttc.workspace = true
```

- [ ] **Step 2: Write fake transport refresh tests**

Add tests in `crates/pandar-agent/src/machine/mqtt.rs`:

```rust
#[tokio::test]
async fn refresh_subscribes_publishes_pushall_and_maps_report() {
    let endpoint = endpoint();
    let transport = FakeMqttTransport::with_reports(vec![json!({"print": {"gcode_state": "IDLE"}})]);

    let snapshot = refresh_printer(&transport, &endpoint, Duration::from_secs(1)).await.unwrap();

    assert_eq!(transport.subscriptions().await, vec!["device/01S00EXAMPLE/report"]);
    assert_eq!(transport.publishes().await.len(), 1);
    assert_eq!(transport.publishes().await[0].topic, "device/01S00EXAMPLE/request");
    assert_eq!(transport.publishes().await[0].qos, BAMBU_MQTT_QOS);
    assert_eq!(transport.publishes().await[0].payload, json!({"pushing": {"command": "pushall"}}));
    assert_eq!(snapshot.state, "IDLE");
}

#[tokio::test]
async fn refresh_preserves_timeout_context() {
    let endpoint = endpoint();
    let transport = FakeMqttTransport::timeout();

    let err = refresh_printer(&transport, &endpoint, Duration::from_millis(10)).await.unwrap_err();

    assert!(format!("{err:#}").contains("01S00EXAMPLE"));
    assert!(format!("{err:#}").contains("timed out"));
}
```

- [ ] **Step 3: Implement transport trait and fake**

Implement:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedMqttCommand {
    pub topic: String,
    pub payload: Value,
    pub qos: u8,
}

#[async_trait::async_trait]
pub trait BambuMqttTransport: Send + Sync {
    async fn subscribe(&self, topic: &str) -> anyhow::Result<()>;
    async fn publish(&self, topic: &str, payload: Value, qos: u8) -> anyhow::Result<()>;
    async fn next_report(&self, timeout: Duration) -> anyhow::Result<Value>;
}

pub async fn refresh_printer(
    transport: &impl BambuMqttTransport,
    endpoint: &BambuPrinterEndpoint,
    report_timeout: Duration,
) -> anyhow::Result<MachineSnapshot> {
    let topics = BambuMqttTopics::for_serial(&endpoint.serial);
    transport
        .subscribe(&topics.report)
        .await
        .with_context(|| format!("subscribe to Bambu report topic for {}", endpoint.serial))?;
    transport
        .publish(
            &topics.request,
            BambuMqttCommand::RequestPushAll.payload(),
            BAMBU_MQTT_QOS,
        )
        .await
        .with_context(|| format!("publish Bambu pushall for {}", endpoint.serial))?;
    let report = transport
        .next_report(report_timeout)
        .await
        .with_context(|| format!("wait for Bambu report for {}", endpoint.serial))?;
    Ok(snapshot_from_report(endpoint, &report))
}
```

Add `use std::time::Duration;` in this module. The fake transport lives under `#[cfg(test)]`, uses `tokio::sync::Mutex<Vec<_>>`, records subscriptions/publishes, and returns either queued reports or a timeout-shaped error from `next_report(timeout)`.

- [ ] **Step 4: Implement isolated rumqttc runtime adapter**

Implement `RumqttcBambuMqttTransport` in `crates/pandar-agent/src/machine/mqtt.rs` using `rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, Transport}`:

- `new(endpoint: &BambuPrinterEndpoint)` builds `MqttOptions::new(format!("pandar-agent-{}", endpoint.serial), &endpoint.host, BAMBU_MQTT_PORT)`.
- Set credentials with username `bblp` and `endpoint.access_code`.
- Use TLS transport through `mqttoptions.set_transport(Transport::tls_with_default_config())`.
- Map `BAMBU_MQTT_QOS == 1` to `QoS::AtLeastOnce`.
- `subscribe` calls `AsyncClient::subscribe(topic, QoS::AtLeastOnce)`.
- `publish` calls `AsyncClient::publish(topic, QoS::AtLeastOnce, false, serde_json::to_vec(&payload)?)`.
- `next_report(timeout)` polls the owned `EventLoop` with `tokio::time::timeout`, returns the first `Packet::Publish` payload decoded as `serde_json::Value`, and preserves timeout, poll, UTF-8/JSON decode context.

Do not exercise this adapter in unit tests against a live broker. Unit tests continue to use `FakeMqttTransport`; the runtime adapter is covered by compile/clippy and by trait-level tests through the fake.

- [ ] **Step 5: Introduce async gateway**

In `crates/pandar-agent/src/machine/mod.rs`, replace the old label-only gateway concept with:

```rust
#[async_trait::async_trait]
pub trait BambuMachineGateway: Send + Sync {
    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>>;
}

#[derive(Debug, Clone, Default)]
pub struct NoopMachineGateway;

#[async_trait::async_trait]
impl BambuMachineGateway for NoopMachineGateway {
    async fn refresh_printers(&self) -> anyhow::Result<Vec<MachineSnapshot>> {
        Ok(Vec::new())
    }
}
```

Remove the existing label-only `ReferenceBackedGateway`; the replacement boundary is `BambuMachineGateway`.

- [ ] **Step 6: Add configured refresh gateway tests**

Add a generic configured gateway that owns endpoints, a `BambuMqttTransport`, and a report timeout. Test it with the fake transport:

```rust
#[tokio::test]
async fn configured_gateway_refreshes_all_printers() {
    let gateway = ConfiguredBambuMachineGateway::new(
        vec![endpoint()],
        FakeMqttTransport::with_reports(vec![json!({"state": "READY"})]),
        Duration::from_secs(1),
    );

    let snapshots = gateway.refresh_printers().await.unwrap();

    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].state, "READY");
}
```

The production `run_once` path uses `NoopMachineGateway` when `PANDAR_PRINTERS` is empty. For non-empty valid config it constructs a `ConfiguredBambuMachineGateway` with one `RumqttcBambuMqttTransport` per endpoint and the default report timeout. Runtime machine sockets are opened only after explicit printer config exists.

- [ ] **Step 7: Verify Task 2**

Run:

```bash
cargo test -p pandar-agent machine::mqtt --no-fail-fast
cargo test -p pandar-agent refresh_printers --no-fail-fast
cargo clippy -p pandar-agent --all-targets -- -D warnings
cargo fmt --check -p pandar-agent
```

Expected: tests and lint pass.

## Task 3: File Transfer Boundary

**Files:**

- Create/Modify: `crates/pandar-agent/src/machine/file_transfer.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`

- [ ] **Step 1: Export the file-transfer module**

Update `crates/pandar-agent/src/machine/mod.rs`:

```rust
pub mod file_transfer;
```

- [ ] **Step 2: Write file-transfer tests first**

Add tests in `file_transfer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_reference_ftps_defaults() {
        assert_eq!(BAMBU_FILE_TRANSFER_PORT, 990);
        assert_eq!(BAMBU_FILE_TRANSFER_USERNAME, "bblp");
        assert_eq!(BAMBU_FILE_TRANSFER_CHUNK_SIZE, 64 * 1024);
    }

    #[test]
    fn a1_models_are_fallback_candidates() {
        assert!(is_a1_model(Some("A1")));
        assert!(is_a1_model(Some("A1 Mini")));
        assert!(!is_a1_model(Some("X1C")));
        assert!(!is_a1_model(None));
    }

    #[test]
    fn mode_cache_only_returns_cached_successes() {
        let mut cache = TransferModeCache::default();
        let endpoint = "192.0.2.10";

        assert_eq!(cache.get(endpoint), None);
        cache.store_success(endpoint, TransferProtectionMode::ProtectedData);

        assert_eq!(cache.get(endpoint), Some(TransferProtectionMode::ProtectedData));
    }

    #[test]
    fn operation_requests_use_expected_shapes() {
        let list = FileTransferRequest::list("/cache");
        assert_eq!(list.operation, FileTransferOperation::List);
        assert_eq!(list.remote_path, "/cache");
        assert_eq!(list.size_bytes, None);

        let download = FileTransferRequest::download("/cache/file.3mf");
        assert_eq!(download.operation, FileTransferOperation::Download);
        assert_eq!(download.remote_path, "/cache/file.3mf");
        assert_eq!(download.size_bytes, None);

        let request = FileTransferRequest::upload("/cache/file.3mf", 100);

        assert_eq!(request.operation, FileTransferOperation::Upload);
        assert_eq!(request.remote_path, "/cache/file.3mf");
        assert_eq!(request.size_bytes, Some(100));

        let delete = FileTransferRequest::delete("/cache/file.3mf");
        assert_eq!(delete.operation, FileTransferOperation::Delete);
        assert_eq!(delete.remote_path, "/cache/file.3mf");
        assert_eq!(delete.size_bytes, None);
    }
}
```

- [ ] **Step 3: Implement constants, mode policy, cache, request types**

Implement:

```rust
use std::collections::HashMap;

pub const BAMBU_FILE_TRANSFER_PORT: u16 = 990;
pub const BAMBU_FILE_TRANSFER_USERNAME: &str = "bblp";
pub const BAMBU_FILE_TRANSFER_CHUNK_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferProtectionMode {
    ProtectedData,
    ClearData,
}

pub fn is_a1_model(model: Option<&str>) -> bool {
    matches!(model, Some("A1" | "A1 Mini"))
}

#[derive(Debug, Default)]
pub struct TransferModeCache {
    modes: HashMap<String, TransferProtectionMode>,
}

impl TransferModeCache {
    pub fn get(&self, endpoint: &str) -> Option<TransferProtectionMode> {
        self.modes.get(endpoint).copied()
    }

    pub fn store_success(&mut self, endpoint: &str, mode: TransferProtectionMode) {
        self.modes.insert(endpoint.to_string(), mode);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTransferOperation {
    List,
    Download,
    Upload,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTransferRequest {
    pub operation: FileTransferOperation,
    pub remote_path: String,
    pub size_bytes: Option<u64>,
}

impl FileTransferRequest {
    pub fn list(remote_path: impl Into<String>) -> Self {
        Self {
            operation: FileTransferOperation::List,
            remote_path: remote_path.into(),
            size_bytes: None,
        }
    }

    pub fn download(remote_path: impl Into<String>) -> Self {
        Self {
            operation: FileTransferOperation::Download,
            remote_path: remote_path.into(),
            size_bytes: None,
        }
    }

    pub fn upload(remote_path: impl Into<String>, size_bytes: u64) -> Self {
        Self {
            operation: FileTransferOperation::Upload,
            remote_path: remote_path.into(),
            size_bytes: Some(size_bytes),
        }
    }

    pub fn delete(remote_path: impl Into<String>) -> Self {
        Self {
            operation: FileTransferOperation::Delete,
            remote_path: remote_path.into(),
            size_bytes: None,
        }
    }
}
```

- [ ] **Step 4: Add file transfer trait and fake boundary tests**

Add an async trait with operation-specific methods and test it only through a fake:

```rust
#[async_trait::async_trait]
pub trait MachineFileTransfer: Send + Sync {
    async fn list(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<String>>;
    async fn download(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<Vec<u8>>;
    async fn upload(&self, path: &str, bytes: &[u8], mode: TransferProtectionMode) -> anyhow::Result<()>;
    async fn delete(&self, path: &str, mode: TransferProtectionMode) -> anyhow::Result<()>;
}
```

The fake records `FileTransferRequest` and selected `TransferProtectionMode` for list, download, upload, and delete. Add tests that call every trait method and assert no real sockets are opened.

- [ ] **Step 5: Add fallback executor tests**

Add tests for mode order:

```rust
#[test]
fn transfer_attempt_order_uses_cache_then_model_policy() {
    let mut cache = TransferModeCache::default();
    assert_eq!(
        transfer_attempt_order("192.0.2.10", Some("A1 Mini"), &cache, false),
        vec![TransferProtectionMode::ProtectedData, TransferProtectionMode::ClearData]
    );
    cache.store_success("192.0.2.10", TransferProtectionMode::ClearData);
    assert_eq!(
        transfer_attempt_order("192.0.2.10", Some("A1 Mini"), &cache, false),
        vec![TransferProtectionMode::ClearData]
    );
    assert_eq!(
        transfer_attempt_order("192.0.2.10", Some("X1C"), &TransferModeCache::default(), false),
        vec![TransferProtectionMode::ProtectedData]
    );
    assert_eq!(
        transfer_attempt_order("192.0.2.10", Some("X1C"), &TransferModeCache::default(), true),
        vec![TransferProtectionMode::ClearData]
    );
}
```

Implement `transfer_attempt_order`.

Add no-network fallback executor tests:

```rust
#[tokio::test]
async fn transfer_fallback_caches_only_successful_mode() {
    let mut cache = TransferModeCache::default();
    let transfer = FakeMachineFileTransfer::protected_fails_clear_succeeds();

    run_with_transfer_mode(
        "192.0.2.10",
        Some("A1 Mini"),
        false,
        &mut cache,
        |mode| transfer.upload("/cache/file.3mf", b"data", mode),
    )
    .await
    .unwrap();

    assert_eq!(cache.get("192.0.2.10"), Some(TransferProtectionMode::ClearData));
}

#[tokio::test]
async fn transfer_fallback_error_contains_protected_and_clear_context() {
    let mut cache = TransferModeCache::default();
    let transfer = FakeMachineFileTransfer::all_modes_fail();

    let err = run_with_transfer_mode(
        "192.0.2.10",
        Some("A1 Mini"),
        false,
        &mut cache,
        |mode| transfer.upload("/cache/file.3mf", b"data", mode),
    )
    .await
    .unwrap_err();

    let chain = format!("{err:#}");
    assert!(chain.contains("protected data mode failed"));
    assert!(chain.contains("clear data mode failed"));
    assert_eq!(cache.get("192.0.2.10"), None);
}
```

Also test protected-first success, forced clear mode, cached clear mode, and failed protected attempt not poisoning the cache.

- [ ] **Step 6: Verify Task 3**

Run:

```bash
cargo test -p pandar-agent machine::file_transfer --no-fail-fast
cargo clippy -p pandar-agent --all-targets -- -D warnings
cargo fmt --check -p pandar-agent
```

Expected: tests and lint pass.

## Task 4: Agent Config And Command Integration

**Files:**

- Create: `crates/pandar-agent/src/commands.rs`
- Modify: `crates/pandar-agent/src/lib.rs`
- Modify: `crates/pandar-agent/src/machine/mod.rs`

- [ ] **Step 1: Split command handling out of lib.rs**

Move `handle_command`, refresh command handling, command event helpers, and their tests from `crates/pandar-agent/src/lib.rs` into `crates/pandar-agent/src/commands.rs`. Re-export the public helpers needed by existing tests:

```rust
mod commands;
pub use commands::{ack_event, handle_command_with_gateway, success_event};
```

Keep `lib.rs` below 400 LOC after the split.

- [ ] **Step 2: Add config parsing tests**

Add tests in `crates/pandar-agent/src/lib.rs`:

```rust
#[test]
fn parses_printer_config_json() {
    let printers = parse_printer_config(
        r#"[{"host":"192.0.2.10","serial":"01S00EXAMPLE","access_code":"12345678","model":"A1 Mini","name":"garage-a1"}]"#,
    )
    .unwrap();

    assert_eq!(printers.len(), 1);
    assert_eq!(printers[0].host, "192.0.2.10");
    assert_eq!(printers[0].serial, "01S00EXAMPLE");
    assert_eq!(printers[0].access_code, "12345678");
    assert_eq!(printers[0].model.as_deref(), Some("A1 Mini"));
    assert_eq!(printers[0].name.as_deref(), Some("garage-a1"));
}

#[test]
fn invalid_printer_config_keeps_context() {
    let err = parse_printer_config(r#"[{"host":""}]"#).unwrap_err();

    assert!(format!("{err:#}").contains("PANDAR_PRINTERS"));
    assert!(format!("{err:#}").contains("serial"));
}

#[test]
fn empty_printer_config_means_no_configured_printers() {
    assert!(parse_printer_config("").unwrap().is_empty());
    assert!(parse_printer_config("   ").unwrap().is_empty());
    assert!(parse_printer_config("[]").unwrap().is_empty());
}

#[test]
fn malformed_printer_config_keeps_parse_context() {
    let err = parse_printer_config("not-json").unwrap_err();

    assert!(format!("{err:#}").contains("PANDAR_PRINTERS"));
}
```

- [ ] **Step 3: Add command event sequence tests**

Refactor command handling so tests can inject a fake gateway:

```rust
#[tokio::test]
async fn refresh_printers_emits_ack_snapshot_and_success() {
    let config = test_config();
    let command_id = uuid::Uuid::new_v4().to_string();
    let gateway = FakeGateway::success(vec![MachineSnapshot {
        serial: "01S00EXAMPLE".to_string(),
        name: "garage-a1".to_string(),
        state: "IDLE".to_string(),
    }]);
    let (sender, mut receiver) = mpsc::channel(4);

    handle_command_with_gateway(
        &config,
        &sender,
        &gateway,
        HubCommand {
            command_id: command_id.clone(),
            command: Some(hub_command::Command::RefreshPrinters(Default::default())),
        },
    )
    .await
    .unwrap();
    drop(sender);

    assert_eq!(receiver.recv().await.unwrap(), ack_event(&config, &command_id));
    let snapshot = receiver.recv().await.unwrap();
    assert!(matches!(snapshot.event, Some(agent_event::Event::PrinterSnapshot(_))));
    assert_eq!(receiver.recv().await.unwrap(), success_event(&config, &command_id));
    assert!(receiver.recv().await.is_none());
}
```

Also add tests for no configured printers producing ack+success only, multiple snapshots preserving order, and failed refresh producing ack+failed result with `format!("{err:#}")` preserved in the result error string.

- [ ] **Step 4: Implement config parsing and gateway command handling**

Add to `AgentConfig`:

```rust
#[arg(long, env = "PANDAR_PRINTERS", default_value = "[]")]
pub printers: String,
```

Implement:

```rust
pub fn parse_printer_config(raw: &str) -> anyhow::Result<Vec<BambuPrinterEndpoint>> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    let printers: Vec<BambuPrinterEndpoint> =
        serde_json::from_str(raw).context("failed to parse PANDAR_PRINTERS JSON")?;
    for printer in &printers {
        if printer.host.trim().is_empty() {
            anyhow::bail!("invalid PANDAR_PRINTERS entry: host is required");
        }
        if printer.serial.trim().is_empty() {
            anyhow::bail!("invalid PANDAR_PRINTERS entry: serial is required");
        }
        if printer.access_code.trim().is_empty() {
            anyhow::bail!("invalid PANDAR_PRINTERS entry: access_code is required");
        }
    }
    Ok(printers)
}
```

Derive `serde::Deserialize` for `BambuPrinterEndpoint`.

Change `run_once` to parse printers at startup. Construct `NoopMachineGateway` for empty config. For non-empty config, construct `ConfiguredBambuMachineGateway` with one `RumqttcBambuMqttTransport` per endpoint and a default bounded report timeout. Do not silently ignore configured printers.

Change refresh command handling to send accepted ack, call gateway, emit `PrinterSnapshot` events from `AgentConfig` identity, then send success or failed command result. Gateway failures are converted into failed command result events; the function returns send/queue errors only after the failed result has been queued.

- [ ] **Step 5: Verify Task 4**

Run:

```bash
cargo test -p pandar-agent refresh_printers --no-fail-fast
cargo test -p pandar-agent parses_printer_config --no-fail-fast
cargo test -p pandar-agent invalid_printer_config --no-fail-fast
cargo clippy -p pandar-agent --all-targets -- -D warnings
cargo fmt --check -p pandar-agent
```

Expected: tests and lint pass.

## Task 5: Documentation And Roadmap

**Files:**

- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update README**

Document:

```bash
PANDAR_PRINTERS='[{"host":"192.0.2.10","serial":"01S00EXAMPLE","access_code":"12345678","model":"A1 Mini","name":"garage-a1"}]'
```

State that tests do not open Bambu MQTT or FTPS sockets, and real printer runtime requires explicit printer config.

- [ ] **Step 2: Update architecture**

Add an agent machine transport section:

- MQTT request/report topics.
- QoS 1 publish requirement.
- `pushall` refresh flow.
- FTPS constants and fallback policy.
- No hub persistence changes in Phase 3.

- [ ] **Step 3: Update roadmap**

Mark completed Phase 3 milestones for pure MQTT builders, MQTT runtime boundary plus fake refresh path, file-transfer boundary, and agent integration. Move Immediate Next to FTPS runtime adapter selection and live-printer compatibility validation.

- [ ] **Step 4: Verify docs changed**

Run:

```bash
git diff -- README.md docs/architecture.md docs/roadmap.md
```

Expected: docs describe Phase 3 accurately and do not claim real printer compatibility unless implemented.

## Task 6: Full Verification And Final Review

**Files:**

- No new implementation files.

- [ ] **Step 1: Run full verification**

Run:

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
git diff --check
```

Expected: all pass.

- [ ] **Step 2: Confirm generated protobuf outputs are absent**

Run:

```bash
find . -path ./target -prune -o \( -name '*.pb.rs' -o -name '*.tonic.rs' \) -print
```

Expected: no output.

- [ ] **Step 3: Request final implementation review**

Dispatch a fresh reviewer with:

- Spec path: `docs/superpowers/specs/2026-06-21-phase-3-bambu-machine-transport-design.md`
- Plan path: `docs/superpowers/plans/2026-06-21-phase-3-bambu-machine-transport.md`
- Diff/base/head SHA.
- Verification output.

Expected: reviewer returns `VERDICT: APPROVE`.

- [ ] **Step 4: Commit and push**

Commit using Lore protocol and push current branch.
