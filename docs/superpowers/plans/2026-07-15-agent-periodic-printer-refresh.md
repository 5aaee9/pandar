# Agent Periodic Printer Refresh Implementation Plan

> **For agentic workers:** Execute this plan task-by-task with fresh implementer and reviewer
> subagents. Keep changes in the working tree; do not create task commits.

**Goal:** Request one full Bambu printer status every 60 seconds from each active Agent MQTT report
forwarder while retaining event-driven report processing and real-response-only presence updates.

**Architecture:** The existing per-printer report task continues to own subscription, firmware
observation, MQTT receive, retry, and teardown. After its existing immediate `pushall`, that same task
owns a delayed Tokio interval and uses a biased select ordered as event-channel closure, periodic
request, then incoming report. A timer tick publishes only the existing typed request; all events and
presence updates continue through the existing real-report parser.

**Tech Stack:** Rust 2024 workspace, Tokio paused-time testing, async-trait, anyhow, serde, cargo-nextest.

## Global Constraints

- Follow `docs/superpowers/specs/2026-07-15-agent-periodic-printer-refresh-design.md` exactly.
- The fixed production interval is 60 seconds and is not configurable in this task.
- Preserve the existing immediate startup `pushall`, report topic, request topic, QoS 1, typed payload,
  firmware processing, idle-timeout behavior, retry ownership, and complete error cause chain.
- A timer tick must not synthesize `PrinterSnapshot`, `PrintJobReport`, or any presence update.
- Preserve runtime abort-and-join ownership; do not spawn a child timer task.
- Do not change Hub, frontend, database, migration, protobuf, or explicit refresh-command behavior.
- Do not contact a live printer. All behavior must be proven with deterministic transports and tests.
- Every touched production Rust module must remain at or below 400 LOC. Do not use `include!`.
- Update `docs/architecture.md` and `docs/roadmap.md` only after implementation review approval.
- OpenCode review gates are explicitly waived for this task after the default model failed. Every
  remaining gate still requires a fresh independent Codex reviewer with literal `VERDICT: APPROVE`.
- SDD commit policy overrides per-task commits: keep the feature in one final Conventional Commit
  after all reviews, docs, and fresh verification. Mandatory verification exposed unrelated
  test-only cleanup races; their independently reviewed corrections must be delivered in one
  separate preceding `test:` commit.

## Baseline and Delivery

- Baseline SHA: `c42508821b624ad6bde46bfe9d3bfdad219bfddf`.
- Expected prerequisite test commit: `test: stabilize firmware lifecycle cleanup`.
- Expected delivery commit: `feat(agent): refresh printer telemetry periodically`.
- Push the current `main` branch to `origin/main`; never force-push.
- `.superpowers/sdd/progress.md` is an ignored local ledger and must not be staged.

## File Ownership

Production files:

- Modify `crates/pandar-agent/src/machine/mqtt/reports.rs` to retain decoding/event construction and
  declare/re-export the focused forwarding module.
- Create `crates/pandar-agent/src/machine/mqtt/reports/forwarding.rs` for the existing forwarding APIs,
  temperature predicate, fixed timer, select loop, and periodic publish error context.

Test/build files:

- Create `crates/pandar-agent/src/machine/mqtt/reports/forwarding/tests.rs` for the controlled transport,
  paused-time schedule/priority/error tests, and production retry regression.
- Modify `crates/pandar-agent/Cargo.toml` only to enable Tokio `test-util` for this crate's tests.

Mandatory verification-only stabilization files (no production behavior):

- Modify `crates/pandar-hub/src/grpc/tests/firmware_commands/prepare_lifecycle.rs` so terminal polling
  also waits for pending completion ownership cleanup.
- Modify the `cfg(test)` pause handle in `crates/pandar-agent/src/machine/mqtt/firmware/session.rs` and
  its positive scheduling windows in `crates/pandar-agent/src/tests/firmware_lifecycle/pump_ownership.rs`.
- Modify `crates/pandar-network-plugin/tests/studio_abi_probe/firmware_mock.rs` so a connection
  cancelled before any HTTP request byte does not terminate the test-only mock Hub.

Documentation/workflow files:

- Modify after implementation approval: `docs/architecture.md` and `docs/roadmap.md`.
- Maintain but do not stage: `.superpowers/sdd/progress.md`.

---

### Task 1: Extract the Report Forwarder Without Behavior Changes

**Files:**

- Modify: `crates/pandar-agent/src/machine/mqtt/reports.rs`
- Create: `crates/pandar-agent/src/machine/mqtt/reports/forwarding.rs`

**Acceptance boundary:** This task is structural only. It must preserve startup-only `pushall`, report
parsing/event behavior, public forwarding signatures, idle timeout handling, and all retry callers.

- [ ] **Step 1: Record the focused characterization baseline**

Run before editing:

```powershell
cargo nextest run -p pandar-agent -E 'test(forward_print_reports)'
cargo nextest run -p pandar-agent -E 'test(runtime_report)'
cargo nextest run -p pandar-core --test module_size
```

Record pass counts in the SDD ledger.

- [ ] **Step 2: Extract forwarding ownership**

Add `mod forwarding;` and re-export `forward_print_reports` and
`forward_print_reports_with_firmware` from `reports.rs`. Move, without semantic edits:

- `snapshot_has_temperature_telemetry`;
- `forward_print_reports`;
- `forward_print_reports_with_firmware`;
- the private forwarding loop.

The child module may call existing parent decoding/event helpers through `super`; do not duplicate
parsers or widen unrelated visibility. Keep the externally observed API through `machine::mqtt`
unchanged.

- [ ] **Step 3: Prove the extraction is behavior-neutral**

Run the same three commands from Step 1, then:

```powershell
cargo fmt --all -- --check
cargo clippy -p pandar-agent --all-targets -- -D warnings
```

Check production LOC for both `reports.rs` and `reports/forwarding.rs`; each must be at most 400.

---

### Task 2: Add Deterministic RED Tests for the Fixed Schedule

**Files:**

- Modify: `crates/pandar-agent/Cargo.toml`
- Modify: `crates/pandar-agent/src/machine/mqtt/reports/forwarding.rs`
- Create: `crates/pandar-agent/src/machine/mqtt/reports/forwarding/tests.rs`

**Test transport contract:** Keep the new transport local to the test module. It must record an ordered
operation log for subscriptions and exact `PublishedMqttCommand` values, expose publish-attempt
synchronization, allow a queued or continuously-ready report source through a test-local unbounded
channel, block when no report exists, and optionally fail exactly one selected `pushall` ordinal with a
stable lower-level cause. Do not enlarge the shared `FakeMqttTransport`.

The transport must also expose an explicit receive-armed barrier backed by an atomic generation/count
plus notification. It becomes ready only after the first `next_report` future has been polled and
parked. Every paused-time schedule test must observe the startup publish and then await this barrier
before advancing virtual time, proving that the interval and select loop are active rather than racing
task startup.

- [ ] **Step 1: Enable paused-time testing only for tests**

Add a Tokio dev-dependency entry using the workspace version with the `test-util` feature. Do not add
`test-util` to the workspace-wide production feature set.

- [ ] **Step 2: Write RED startup and schedule tests**

Name every new test with the `periodic_printer_refresh_` prefix so the focused filter is discoverable.
Using `#[tokio::test(start_paused = true)]`, startup-publish observation plus the receive-armed barrier,
assert:

1. startup subscribes once and publishes exactly one immediate typed `pushall` with
   `device/{serial}/request`, the current payload shape, and QoS 1;
2. advancing to 60 seconds minus one nanosecond publishes no second request;
3. advancing the final nanosecond publishes exactly one second request with no incoming report;
4. a qualifying report queued before the deadline is forwarded immediately and does not move the
   original 60-second deadline;
5. a successful periodic publish without a response leaves the event receiver empty of snapshot and
   print-report events.

- [ ] **Step 3: Write RED missed-tick and biased-selection tests**

Assert deterministically:

- advancing across several deadlines yields only one catch-up publish; after it is observed, 59 more
  seconds yields none and the 60th second yields one;
- if receiver closure and the deadline are made ready together, the forwarder completes and no
  periodic publish begins;
- if the deadline and a continuously-ready non-qualifying report source are ready together, the
  periodic publish occurs, proving reports cannot starve the timer;
- closing the receiver before the deadline cooperatively completes the forwarder, and advancing past
  later deadlines does not add publishes.

For simultaneous-ready cases, first wait for the receive-armed count, then enqueue the report or drop
the receiver and advance virtual time without any intervening `.await`; inspect the operation log only
after yielding. Do not use `tokio::time::timeout` inside paused-time tests because its automatic virtual
clock advancement can make priority assertions vacuous.

- [ ] **Step 4: Write the RED direct error-chain test**

Configure the transport so the startup publish succeeds and the second `pushall` fails. At the
60-second deadline, require the direct forwarder error formatted with `{:#}` to contain both periodic
request-topic context and the stable lower transport cause.

- [ ] **Step 5: Run the RED suite and record expected failures**

Run:

```powershell
cargo nextest run -p pandar-agent --no-tests=fail -E 'test(periodic_printer_refresh)'
```

The new tests must compile and fail because the 60-second interval/periodic branch is absent, not due
to fixture deadlock, wall-clock delay, unrelated compilation errors, or a zero-test filter. Record the
failing assertions.

---

### Task 3: Implement the 60-Second Request in the Existing Forwarder

**Files:**

- Modify: `crates/pandar-agent/src/machine/mqtt/reports/forwarding.rs`

- [ ] **Step 1: Define the fixed production period**

Add one focused `Duration::from_secs(60)` constant in the forwarding module. Keep it visible only as
needed by its child tests; do not add configuration plumbing. In this GREEN step, add
`periodic_printer_refresh_uses_exact_sixty_second_constant` to assert the production constant directly.
This resolves RED ordering because Task 2 behavior tests use a test-side 60-second boundary and do not
reference a not-yet-existing symbol.

- [ ] **Step 2: Start a delayed interval after the startup publish**

After the existing immediate `pushall` succeeds, create `tokio::time::interval_at` with its first
deadline at current Tokio time plus 60 seconds. Set `MissedTickBehavior::Delay`. Do not consume an
immediate interval tick and do not reset the deadline after reports.

- [ ] **Step 3: Select with exact priority and existing report semantics**

Replace the outer `is_closed` check plus sequential receive with `tokio::select! { biased; ... }` in
this exact order:

1. `sender.closed()` returns `Ok(())`;
2. interval tick publishes the existing `RequestPushAll` to the resolved request topic at QoS 1;
3. `transport.next_report(report_timeout)` runs the existing report/idle/error handling unchanged.

The periodic branch must only publish. It must not emit an Agent event, mutate the feature cache, or
call firmware observation directly. Give periodic publish failures explicit request-topic context and
preserve the transport error as the lower cause.

- [ ] **Step 4: Make all deterministic tests GREEN**

Run:

```powershell
cargo nextest run -p pandar-agent --no-tests=fail -E 'test(periodic_printer_refresh)'
cargo nextest run -p pandar-agent -E 'test(forward_print_reports)'
cargo fmt --all -- --check
cargo clippy -p pandar-agent --all-targets -- -D warnings
```

Confirm the controlled tests finish under paused time without sleeps tied to wall clock.

---

### Task 4: Prove Production Retry, Firmware, and Lifecycle Ownership

**Files:**

- Modify: `crates/pandar-agent/src/machine/mqtt/reports/forwarding/tests.rs`
- Modify production runtime files only if a failing spec test proves the existing ownership is not
  preserved; any such change requires a focused explanation and independent review.

- [ ] **Step 1: Add the periodic-failure production retry regression**

Call the real `forward_print_reports_with_firmware_retry` with paused time and a controlled transport
that fails only the first generation's periodic `pushall`. Assert this exact sequence:

1. initial operation log: subscription #1, firmware `get_version`, then immediate `pushall`;
2. periodic `pushall` failure at exactly 60 seconds;
3. a real firmware invalidation event and transient device-feature invalidation event;
4. no re-subscription before the existing five-second retry delay;
5. at 65 seconds, subscription #2, a new firmware `get_version`, and the new generation's immediate
   `pushall`;
6. after the replacement receive/select loop is armed, advancing to 120 seconds produces no request
   from the failed generation's old schedule;
7. only at 125 seconds does the new generation publish its first periodic `pushall`.

Prime/drain only the state required to distinguish these events. Do not weaken production retry or
add a test-only production fallback. The `t=120` and `t=125` assertions are mandatory; they make
old-timer cancellation and fresh-new-timer ownership non-vacuous.

- [ ] **Step 2: Re-run teardown and report regressions**

Run:

```powershell
cargo nextest run -p pandar-agent --no-tests=fail -E 'test(periodic_printer_refresh)'
cargo nextest run -p pandar-agent -E 'test(firmware_generation)'
cargo nextest run -p pandar-agent -E 'test(runtime_report)'
cargo nextest run -p pandar-agent -E 'test(forward_print_reports)'
cargo nextest run -p pandar-core --test module_size
```

Confirm existing abort-and-join runtime tests remain authoritative and cooperative receiver closure is
covered separately by the new forwarding test.

- [ ] **Step 3: Verify the full Agent crate**

Run:

```powershell
cargo nextest run -p pandar-agent
cargo fmt --all -- --check
cargo clippy -p pandar-agent --all-targets -- -D warnings
```

Record exact pass counts and production LOC.

---

### Task 5: Independent Implementation Review Gate

**Inputs:** Reviewed spec, reviewed plan, baseline SHA, complete implementation diff, focused test
output, Agent test output, and production LOC.

- [ ] Dispatch a fresh independent Codex reviewer to judge spec compliance, test adequacy, ownership,
      error chains, missed-tick behavior, and unintended scope. Require the exact implementation verdict:

```text
VERDICT: APPROVE | REVISE
SPEC_COVERAGE:
- [implemented requirement or missing requirement]
BLOCKERS:
- [blocking gap or "None"]
REQUIRED_CHANGES:
- [change or "None"]
```

- [ ] If the reviewer returns `REVISE` or omits literal approval, fix only the identified gaps, rerun
      focused and Agent verification, and re-dispatch a fresh reviewer. Do not update delivery docs before
      literal `VERDICT: APPROVE`.

---

### Task 6: Mandatory Post-Approval Documentation and Delivery

This task is mandatory and begins only after Task 5 returns literal `VERDICT: APPROVE`.

**Files:**

- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`
- Maintain, do not stage: `.superpowers/sdd/progress.md`

- [ ] **Step 1: Update required documentation**

In `docs/architecture.md`, distinguish the 30-second MQTT protocol keep-alive, 10-second idle receive
timeout, 15-second Agent heartbeat, and fixed 60-second full-status request. State that a scheduled
publish alone never proves presence; only a qualifying real MQTT response can reach the existing Hub
presence paths.

In `docs/roadmap.md`, record the completed Agent behavior, `MissedTickBehavior::Delay`, retry-generation
ownership, deterministic coverage, real-response-only presence semantics, and the fact that no live
printer test was performed. Record rollback as: revert the delivery commit and restart the Agent;
Hub, frontend, database, and protobuf require no rollback.

- [ ] **Step 2: Run fresh required verification**

Run exactly:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p pandar-core --test module_size
cargo nextest run -p pandar-agent
cargo nextest run --manifest-path Cargo.toml --workspace
cargo nextest run -p pandar-agent --no-tests=fail -E 'test(periodic_printer_refresh)'
```

The explicit Agent run is required in addition to the full workspace run so its exact pass count is
recorded. All commands must be fresh after docs and any review fixes.

- [ ] **Step 3: Run the complete-delivery independent review**

Give a fresh independent Codex reviewer the spec, reviewed plan, baseline SHA, full diff including
docs, focused and full verification output, and module LOC. Require literal `VERDICT: APPROVE` using
the implementation verdict format. If it returns `REVISE`, fix only blocking gaps, rerun every affected
focused command and the complete fresh command set, then re-review.

- [ ] **Step 4: Inspect and stage only intended scope**

Run:

```powershell
git status --short
git diff --check
git diff --stat
git diff --name-only
```

Every staged path must map to the reviewed spec/plan, Agent forwarding/test/dependency changes, the
four independently reviewed verification-only stabilization files, or the two required docs.
Explicitly exclude `.superpowers/sdd/progress.md`, `target/`, generated probe paths, and unrelated
user changes. Stage paths explicitly rather than using an unbounded add.

- [ ] **Step 5: Create the reviewed Conventional Commits**

Load the `conventional-commits` skill. First stage only the four verification-only stabilization
paths and commit them with:

```text
test: stabilize firmware lifecycle cleanup
```

Then stage only the Agent periodic-refresh feature, dependency, spec, plan, architecture, and roadmap
paths and commit them with:

```text
feat(agent): refresh printer telemetry periodically
```

Record both resulting SHAs. Do not amend unrelated history.

- [ ] **Step 6: Push without force and verify remote SHA**

Run:

```powershell
git push origin main
git rev-parse HEAD
git rev-parse origin/main
git ls-remote origin refs/heads/main
```

The local SHA, tracking ref, and remote branch SHA must match. If `main` advanced, fetch and rebase the
single reviewed commit without force-pushing, rerun affected verification, then push. If credentials,
network, or branch policy block push, report the local SHA and complete error chain.

- [ ] **Step 7: Record the operational handoff**

Report the commit SHA, push result, exact focused/Agent/workspace pass counts, module-size and lint
results, docs updated, no-live-printer limitation, and rollback procedure: revert the commit and
restart the Agent.
