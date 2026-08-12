# Plan: Bambu Studio Personal Data Synchronization

## Overview

Implement the specified behavior in `docs/specs/2026-08-12-bambu-studio-personal-data-sync.md` as serial, independently reviewable slices: contract tests, Hub persistence/routes, Rust plugin behavior, then compiled/real Studio verification and documentation.

**Baseline:** `30a2b6e` on `main`.

**Architecture:** Bambu Studio calls the existing ABI. The thin C++ shim adapts STL values and callbacks. A Rust `personal_presets` module owns Studio map conversion, Hub HTTP, list-cycle fencing, and the memory cache. `pandar-hub` authorizes the current `plugin:studio` user and persists user-owned presets through a backend-neutral repository. `pandar-agent` is not involved.

## Global constraints

- Follow the linked spec. Escalate any change to ownership, no-auth behavior, conflict semantics, quotas, or compatibility scope before coding it.
- Keep one writer active at a time and use test-first changes for each slice.
- Do not add Bambu Cloud endpoints, filament-cloud CRUD, AMS sync, Web editing, tenant-shared presets, or Agent work.
- C++ remains ABI/STL/callback glue only. Rust owns HTTP, JSON, validation, cache, lifecycle, and policy.
- Known JSON shapes use typed serde models; only the slicer option map is open-ended.
- All persistence must pass equivalent SQLite and real PostgreSQL tests.
- Preserve full error cause chains in logs and redact preset bodies, G-code/notes, credentials, and paths.
- Keep every production module at or below 400 LOC; split before exceeding the limit and do not use `include!`.
- No legacy fallback: new plugin + old Hub fails the cycle safely; it does not synthesize an empty successful catalogue.
- Do not update compatibility claims until a packaged plugin passes the named real Studio gate.

## Prerequisites

- Review and approve the spec.
- Record a clean baseline:

```bash
git status --short
cargo nextest run -p pandar-network-plugin -E 'binary(studio_abi_probe)' --no-tests=fail
cargo nextest run -p pandar-hub --no-tests=fail
./scripts/sync-hub-migrations.sh --check
```

- Confirm `PANDAR_TEST_POSTGRES_URL` is available before beginning the persistence slice. If it is unavailable, implementation may proceed locally, but Task 2 cannot be marked complete.

---

## Task 1: Lock the personal-preset Studio contract with RED tests

**Files**

- Create: `crates/pandar-network-plugin/tests/personal_presets.rs`
- Create: `crates/pandar-network-plugin/tests/fixtures/studio_personal_presets.cpp`
- Modify: `crates/pandar-network-plugin/tests/studio_abi_probe.rs` and its focused modules/fixture dispatch as needed
- Modify only for contract extraction if necessary: `tools/studio-abi-contract/src/`

**Actions**

1. Add a compiled caller using the exact pinned `NetworkAgent.hpp` for each active ABI series.
2. Exercise all six exports involved in the flow: `get_setting_list`, `get_setting_list2`, `get_user_presets`, `request_setting_id`, `put_setting`, and `delete_setting`.
3. Freeze callback semantics:
   - `CheckFn` receives exactly the four metadata fields;
   - skipped entries still appear in `get_user_presets`;
   - progress is monotonic and reaches 100 on success;
   - cancellation returns `-18` and the subsequent drain is empty;
   - the second drain is empty;
   - callback re-entry does not deadlock.
4. Freeze create/update output mutation:
   - create returns a non-empty opaque id;
   - create/update write the server timestamp into `values_map["updated_time"]`;
   - HTTP status is returned through `http_code`;
   - quota code `14` remains in the mutable map.
5. Prove current RED behavior is specifically `unsupported_cloud_settings`, not a crash or missing symbol.

**Validation**

```bash
cargo nextest run -p pandar-network-plugin -E 'binary(personal_presets) | binary(studio_abi_probe)' --no-tests=fail
cargo run --manifest-path tools/studio-abi-contract/Cargo.toml -- \
  --studio-source <exact-pinned-checkout> \
  --plugin <native-plugin> \
  --boost-archive <boost-1.84.0.tar.gz>
```

**Gate:** A fresh reviewer verifies that RED assertions come from pinned Studio behavior and do not copy a Pandar implementation assumption.

---

## Task 2: Add dual-backend Personal preset persistence

**Files**

- Create: `crates/pandar-hub/migrations/shared/20260812000000_personal_presets.sql`
- Create only if genuinely needed: paired files under `crates/pandar-hub/migrations/overrides/{sqlite,postgres}/`
- Regenerate: `crates/pandar-hub/migrations/{sqlite,postgres}/20260812000000_personal_presets.sql`
- Create: `crates/pandar-hub/src/entities/personal_presets.rs`
- Modify: `crates/pandar-hub/src/entities/mod.rs`
- Create: `crates/pandar-hub/src/repositories/personal_presets.rs`
- Split below that module as needed, for example `personal_presets/{rows,mutation,validation}.rs`
- Modify: `crates/pandar-hub/src/repositories/mod.rs`
- Modify: `crates/pandar-hub/src/lib.rs` (`AppState` repository ownership/accessor)
- Create: `crates/pandar-hub/src/repositories/tests/personal_presets.rs`
- Modify: `crates/pandar-hub/src/repositories/tests/mod.rs`
- Modify if a new typed uniqueness classifier is needed: `crates/pandar-hub/src/db.rs`

**Actions**

1. Add the `personal_presets` and `personal_preset_clocks` tables with the equivalent constraints/indexes from the spec.
2. Add typed domain/repository models:
   - `PersonalPresetType` exhaustive enum (`Print`, `Filament`, `Printer`);
   - `PersonalPreset`, metadata row, and create/replace inputs;
   - typed options `BTreeMap<String, String>` serialized once at the repository seam.
3. Implement owner-scoped methods with a small interface:
   - `list_metadata(tenant_id, owner_user_id)`;
   - `get(tenant_id, owner_user_id, setting_id)`;
   - `create_with_audit(...)`;
   - `replace_with_audit(...)`;
   - `delete_with_audit(...)`.
4. Use one write transaction for validation, owner timestamp allocation, mutation, and audit insertion.
5. Allocate strictly increasing owner timestamps from a durable per-owner clock row in the same transaction. Lock/upsert that row through dialect-specific repository internals so concurrent first writes and writes after deleting the final preset cannot regress the clock.
6. Enforce global owner name uniqueness across all three preset types, the 1,000-owner quota, lengths, and Studio's 350 KiB sum-of-key/value-bytes limit before write. Make an identical create replay return the existing id/timestamp without advancing the clock or duplicating audit; reject a same-name non-identical create.
7. Make delete owner-scoped and idempotent; never disclose another owner's existence.
8. Add stable `RepositoryError` variants and uniqueness classification only where required.
9. Test:
   - all three preset types;
   - create/get/list/replace/delete;
   - strictly increasing timestamps under same-second writes, concurrent first writes, and recreation after deleting the final preset;
   - identical create replay after an ambiguous response, plus duplicate name and rename conflicts including the same name across different types;
   - quota/size/key/value/name/type validation;
   - cascade on user/tenant deletion;
   - cross-user and cross-tenant isolation;
   - audit metadata redaction;
   - malformed persisted JSON surfaces its cause instead of being dropped.

**Validation**

```bash
./scripts/sync-hub-migrations.sh
./scripts/sync-hub-migrations.sh --check
cargo nextest run -p pandar-hub -E 'test(personal_preset)' --no-tests=fail
PANDAR_TEST_POSTGRES_URL=postgres://... \
  cargo nextest run -p pandar-hub -E 'test(personal_preset)' --no-tests=fail
cargo fmt --all -- --check
cargo clippy -p pandar-hub --all-targets --all-features -- -D warnings
```

**Gate:** Both backends pass with zero runtime skips. A fresh reviewer checks ownership, locking/timestamps, SQL parity, and absence of preset contents in audit/log paths.

---

## Task 3: Add authenticated Hub preset routes

**Files**

- Create: `crates/pandar-hub/src/routes/plugin/personal_presets.rs`
- Modify: `crates/pandar-hub/src/routes/plugin.rs`
- Modify: `crates/pandar-hub/src/routes.rs`
- Modify: `crates/pandar-hub/src/routes/auth.rs` to expose a preset-specific user authorization result without weakening other plugin routes
- Create: `crates/pandar-hub/src/routes/tests/plugin/personal_presets.rs`
- Modify: `crates/pandar-hub/src/routes/tests/plugin.rs`
- Modify shared route-test typed fixtures only as needed

**Actions**

1. Add the five REST routes from the spec.
2. Introduce typed request/response models with `BTreeMap<String, String>` only for open-ended options.
3. Derive `(tenant_id, owner_user_id, audit actor)` from the authenticated plugin session. Reject no-auth or ownerless sessions with `personal_presets_require_user`.
4. Validate `bundle_version` syntax but do not filter the complete catalogue by version.
5. Map repository failures to stable HTTP/error/code shapes, including `code: 14` for quota.
6. Add `cache-control: no-store` to preset responses.
7. Route tests cover:
   - successful catalogue/full/create/update/delete;
   - plugin-only scope requirement;
   - revoked token, downgraded/missing user, and no-auth rejection;
   - guessed cross-user/cross-tenant ids;
   - malformed and oversize body behavior;
   - identical POST replay, non-identical duplicate name, and quota responses;
   - idempotent missing DELETE;
   - options round trip without server-owned field injection;
   - audit redaction.
8. Run the route suite on SQLite and real PostgreSQL.

**Validation**

```bash
cargo nextest run -p pandar-hub -E 'test(personal_preset)' --no-tests=fail
PANDAR_TEST_POSTGRES_URL=postgres://... \
  cargo nextest run -p pandar-hub -E 'test(personal_preset)' --no-tests=fail
cargo clippy -p pandar-hub --all-targets --all-features -- -D warnings
```

**Gate:** Fresh security review approves user/tenant isolation and no-auth denial.

---

## Task 4: Implement the Rust plugin Personal presets module

**Files**

- Create: `crates/pandar-network-plugin/src/personal_presets.rs`
- Split as needed: `crates/pandar-network-plugin/src/personal_presets/{model,http,cache,ffi}.rs`
- Modify: `crates/pandar-network-plugin/src/lib.rs`
- Modify: `crates/pandar-network-plugin/src/shim_types.hpp` only for flat FFI carrier declarations
- Modify: `crates/pandar-network-plugin/src/shim_abi_operations.hpp`
- Modify: `crates/pandar-network-plugin/src/shim_abi_user.hpp`
- Modify: `crates/pandar-network-plugin/src/studio_disposition.rs`
- Modify: `crates/pandar-network-plugin/tests/personal_presets.rs`
- Modify mock-Hub helpers under `crates/pandar-network-plugin/tests/studio_abi_probe/mock_hub/`

**Actions**

1. Add typed Hub catalogue/full/create/update response and request models.
2. Implement flat Studio-map admission:
   - validate name/type/version/size/lengths;
   - remove server-owned fields from options;
   - preserve open-ended string values;
   - reconstruct required download keys including empty `base_id` and session `user_id`.
3. Implement a process-local cache scoped by agent/account/configuration generation:
   - reset at list-cycle admission;
   - build in temporary Rust state;
   - publish atomically only after complete success;
   - drain atomically exactly once;
   - clear on failure, cancellation, logout, account replacement, and destroy.
4. Implement `get_setting_list2`:
   - one bounded catalogue request;
   - check cancellation before each item and full fetch;
   - invoke `CheckFn` outside locks for each metadata item;
   - retain minimal entries for false checks;
   - fetch full entries for true checks;
   - reject a full-fetch failure rather than publishing a partial catalogue;
   - invoke monotonic progress and cancellation as specified;
   - reject stale account/configuration generation at completion.
5. Implement `get_setting_list` through the same core with “fetch all” policy.
6. Implement create/update/delete HTTP calls and exact ABI return codes (`-7`, `-8`, `-9`, `-10`, `-18`).
7. Preserve HTTP status and mutable `updated_time`/`code` outputs.
8. Change only operations 9–14 from the unsupported disposition. Keep cloud filament and all unrelated unsupported surfaces unchanged.
9. Keep the shim changes mechanical: STL/function adaptation and copying only. No JSON parsing, URL construction, or policy in C++.
10. Add deterministic tests for:
    - complete and skipped catalogue drains;
    - empty catalogue;
    - second drain;
    - cancellation at each item;
    - list/full HTTP failure and malformed response;
    - stale generation/account swap/logout;
    - callback re-entry;
    - create/update/delete and quota code 14;
    - invalid local input before Hub I/O;
    - token redaction and absence of preset body in logs/errors;
    - no-auth explicit failure.

**Validation**

```bash
cargo nextest run -p pandar-network-plugin \
  -E 'binary(personal_presets) | binary(studio_abi_probe) | binary(logout_revoke)' \
  --no-tests=fail
cargo fmt --all -- --check
cargo clippy -p pandar-network-plugin --all-targets --all-features -- -D warnings
cargo test -p pandar-core --test module_size
```

**Gate:** Fresh correctness/concurrency review approves callback lock discipline, generation fencing, cache atomicity, and C++ thinness.

---

## Task 5: Turn the compiled contract GREEN for every ABI series

**Files**

- Modify Task 1 fixtures only to assert production success; do not relax source-derived types or callbacks
- Modify: release-smoke/ABI-series test configuration only if the new behavior needs a new probe mode

**Actions**

1. Run the exact pinned-source caller against each catalogued ABI-series plugin artifact.
2. Confirm no export count or signature changes.
3. Exercise at least these end-to-end mock-Hub scenarios through the built dynamic library:
   - one preset of each type;
   - `CheckFn=false` item retained;
   - create → update → list/full → delete;
   - quota code 14;
   - cancellation and Hub failure expose no partial map;
   - account replacement fences an in-flight cycle.
4. Run on native C++ ABI families used for release: MSVC Windows, native Linux/libstdc++, AppleClang/libc++ arm64, and Rosetta x86_64 where that artifact is claimed.

**Validation**

```bash
# Repeat with each PANDAR_STUDIO_ABI_SERIES and native artifact
PANDAR_STUDIO_ABI_SERIES=02.06.00 cargo build -p pandar-network-plugin
# ... through 02.08.01

cargo run --manifest-path tools/studio-abi-contract/Cargo.toml -- \
  --studio-source <series-pinned-checkout> \
  --plugin <series-native-plugin> \
  --boost-archive <boost-1.84.0.tar.gz>
```

**Gate:** Independent ABI reviewer confirms the same Rust behavior is reached through every series-specific C++ layout.

---

## Task 6: Real Studio two-installation verification

**Files**

- Modify: `docs/compatibility/bambu-studio-plugin-smoke.md`
- Add a redacted evidence record under the existing compatibility evidence convention; do not place secrets or preset bodies in git

**Actions**

1. Build a three-file packaged release artifact for the exact Studio build under test.
2. Use one authenticated Pandar user/tenant and two isolated Studio data directories or hosts, A and B.
3. With harmless synthetic presets containing no credentials or machine-specific secrets:
   - create one Process, Filament, and Printer preset in A;
   - sync B and verify exact names/selected representative option values;
   - update one preset in B and verify A converges after sync;
   - delete a clean preset in A and verify B removes it after a complete sync;
   - make a local preset dirty, delete remotely, and verify Studio preserves the dirty local work according to its native behavior;
   - disable Hub during list and verify no valid local preset disappears;
   - cancel progress and verify no partial result is applied;
   - logout and verify cache/account isolation.
4. Record exact Studio version, ABI series, OS/architecture, plugin/package SHA-256, Hub snapshot, database backend, date, and redacted outcomes.
5. Repeat the Hub storage flow once with SQLite and once with PostgreSQL. The UI flow may be one platform initially, but compatibility claims remain exact-version/platform scoped.

**Gate:** The feature stays `in_progress` until the packaged real Studio flow passes. Automated fixtures alone are not a compatibility claim.

---

## Task 7: Documentation and final verification

**Files**

- Modify: `CONTEXT.md`
- Modify: `docs/architecture.md`
- Modify: `docs/development.md`
- Modify: `docs/compatibility/bambu-studio-plugin.md`
- Modify: `docs/compatibility/bambu-studio-plugin-smoke.md`
- Modify: `docs/roadmap.md`

**Actions**

1. Add **Personal preset** to the domain glossary without implementation details.
2. Document Hub/user ownership, plugin-only flow, no-auth exclusion, and rollout/rollback in architecture/development docs.
3. Update compatibility rows only for exact Studio/platform evidence completed in Task 6.
4. Update the roadmap with completed behavior and immediate untested platform/evidence work.
5. Run final fresh-context reviews along distinct axes:
   - spec and Studio compatibility;
   - authorization/privacy and dual-backend correctness;
   - concurrency/callback/cache behavior;
   - test/evidence completeness and simplicity.
6. Apply accepted fixes through one writer, rerun affected gates, then inspect the final diff.

**Final validation**

```bash
./scripts/sync-hub-migrations.sh --check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cargo test -p pandar-core --test module_size
git diff --check
git status --short
```

## Delivery slices and commit boundaries

Use one Conventional Commit per reviewed deployable slice:

1. `test(network-plugin): lock personal preset sync contract`
2. `feat(hub): persist personal Studio presets`
3. `feat(network-plugin): synchronize personal Studio presets`
4. `docs(studio): record personal preset synchronization evidence`

Do not commit a RED-only test if the repository's delivery policy requires green main; keep Task 1 uncommitted until its corresponding implementation slice turns it green, or place RED fixtures behind the established contract-test workflow.

## Dependencies

```text
Task 1 contract RED
  → Task 2 persistence
  → Task 3 Hub routes
  → Task 4 plugin implementation
  → Task 5 compiled ABI matrix
  → Task 6 real Studio evidence
  → Task 7 documentation/final gates
```

Task 2 and Task 3 may be implemented in one Hub delivery slice, but persistence tests must pass independently before route code is accepted. Do not parallelize writers across these tasks in the same worktree.

## Rollback

- Stop/replace the new plugin producer first so no client expects preset routes during Hub rollback.
- Roll Hub binaries back after active sync requests drain.
- Leave the additive `personal_presets` table and data in place.
- Restore Studio's prior plugin and companion together while Studio is stopped.
- Never translate a route-missing rollback failure into an empty successful catalogue.
