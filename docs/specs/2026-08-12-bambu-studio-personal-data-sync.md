# Spec: Bambu Studio Personal Data Synchronization

## Status and baseline

- Status: proposed.
- Pandar baseline: `30a2b6e` on `main` (2026-08-12).
- Studio contract sources:
  - the ABI-series catalog in `studio-abi-profiles.json`;
  - the exact pinned Bambu Studio source commit for each catalog entry;
  - `reference/open-bamboo-networking` commit `8b38c337ab5741216738d4755954a4d314e0f704` as behavioral corroboration, not as the authoritative ABI source.

## Problem

Bambu Studio offers **Synchronize personal data**, covering user-created Process, Filament, and Printer presets. Pandar exports the required ABI symbols, but all six preset operations currently return `unsupported_cloud_settings`. A signed-in Studio therefore cannot download presets created on another installation, upload a local preset, update it, or propagate deletion.

## Goal

Implement user-owned preset synchronization through the existing Hub-backed Studio plugin architecture so a person can use the same Studio presets on multiple Studio installations connected to the same Pandar tenant.

The feature must:

1. support Process, Filament, and Printer user presets;
2. preserve Studio's established ABI, callback, cache, timestamp, and error semantics;
3. isolate presets by tenant and Pandar user;
4. support equivalent SQLite and PostgreSQL behavior;
5. keep the C++ shim limited to ABI/STL/callback adaptation, with policy, HTTP, validation, and synchronization state in Rust;
6. fail explicitly without deleting or replacing valid local presets when a remote sync is incomplete.

## Non-goals

- Bambu Cloud interoperability or migration from a Bambu account.
- Cloud filament catalogue CRUD (`get/create/update/delete_filament`) or `sync_ams_filaments`.
- Synchronizing projects, build plates, print history, MakerWorld data, printer credentials, app preferences, or arbitrary files.
- Web or Android preset editors.
- Shared tenant preset libraries, admin access to another user's presets, or preset sharing between tenants.
- Content-aware three-way merging of slicer parameters.
- Background work in `pandar-agent`; preset synchronization is Studio plugin → Hub only.
- Compatibility with Studio builds outside `studio-abi-profiles.json`.

## Domain decisions

### Personal preset

A **Personal preset** is a user-created Process, Filament, or Printer preset owned by one tenant-local Pandar user. It is not tenant-shared configuration.

Identity is the Hub-generated opaque `setting_id`; `name` is mutable display data, not authority. Studio's drained catalogue is a single map keyed only by name, not by `(type, name)`. The initial implementation therefore rejects a second active preset with the same `(tenant, owner, name)`, including when the two presets have different types, rather than silently overwriting one catalogue entry.

Add this term to `CONTEXT.md` when implementation starts.

### Ownership

Authenticated plugin tickets already create a `plugin:studio` token with `created_by_user_id`. Preset routes require both:

- exactly the `plugin:studio` scope; and
- a live `session_user` with at least Operator role.

The preset owner is `session_user.id`. The client cannot supply or override it. All reads and mutations constrain both `tenant_id` and `owner_user_id`.

A Studio token whose user has been removed or downgraded is denied by the existing `authorize_plugin_studio` gate.

### No-auth mode

No-auth plugin sessions do not have a durable user owner. Personal preset synchronization is therefore unavailable in no-auth mode. Preset ABI calls return their operation-specific failure instead of falling back to a tenant-wide shared namespace.

This is deliberate: inventing a synthetic owner would make later authenticated ownership and deletion ambiguous.

### Conflict policy

Pandar does not invent a merge algorithm. The Hub serializes each mutation and assigns a strictly increasing `updated_time` in Unix seconds per owner:

```text
next_updated_time = max(current_unix_seconds, owner_last_updated_time + 1)
```

`owner_last_updated_time` is stored in a durable per-owner clock row, so deleting the owner's final preset cannot make a later create move the clock backwards. This preserves Studio's comparison behavior even when two writes occur within one second or hosts have skewed clocks. The client-supplied `updated_time` is compatibility metadata only and never controls Hub ordering.

Concurrent updates to the same `setting_id` are accepted in Hub commit order; last committed write wins. On the next list, Studio compares the Hub timestamp with its local timestamp according to its existing behavior. Dirty local edits remain protected by Studio's own loader logic.

### Deletion

Deletion is idempotent for an owned `setting_id`: an already absent preset returns success. A `setting_id` owned by another user or tenant is indistinguishable from absent.

The first implementation uses hard deletion. Studio represents deletion through absence from the complete catalogue, so the Hub does not need a tombstone for current clients.

## Studio behavior contract

Bambu Studio loads and saves the feature through these existing ABI functions:

| ABI function                       | Required behavior                                                                                                                                                                                          |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bambu_network_get_setting_list2`  | List the complete owner catalogue, call `CheckFn` once per item, fetch/cache full content only when requested, retain metadata entries for skipped items, report bounded progress, and honor cancellation. |
| `bambu_network_get_setting_list`   | Compatibility form that downloads every listed preset.                                                                                                                                                     |
| `bambu_network_get_user_presets`   | Atomically drain the cache produced by the last successful list cycle into Studio's nested map. A second drain is empty.                                                                                   |
| `bambu_network_request_setting_id` | Create an owned preset, return its opaque id, set HTTP status, and replace `values_map["updated_time"]` with the Hub timestamp.                                                                            |
| `bambu_network_put_setting`        | Update an owned preset by id, set HTTP status, and replace `values_map["updated_time"]`.                                                                                                                   |
| `bambu_network_delete_setting`     | Idempotently delete an owned preset by id.                                                                                                                                                                 |

### Catalogue completeness invariant

A successful `get_setting_list`/`get_setting_list2` cycle must cache an entry for **every** remote preset name, including presets for which `CheckFn` returns false. Studio uses the returned key set to remove presets deleted by another session. Omitting a skipped item would make Studio delete a valid local file.

If listing fails or cancellation occurs, the newly built cache is discarded and the ABI call fails. `get_user_presets` must not expose a partial catalogue. The previous cache is also invalidated at the start of a new cycle so stale data cannot be mistaken for a successful refresh.

### Callback rules

- Callback invocation occurs without holding account or cache locks.
- `CheckFn` receives exactly `type`, `name`, `setting_id`, and `updated_time`.
- `ProgressFn` is monotonic in `[0, 100]`; an empty successful catalogue reports `100`.
- `WasCancelledFn` is checked before each catalogue item and again before each full preset fetch. Cancellation returns `BAMBU_NETWORK_ERR_CANCELED (-18)` and publishes no partial cache.
- A changed account/configuration generation during a cycle invalidates the result.

### Flat Studio map

Pandar accepts a flat string map because that is the fixed C++ ABI, but immediately validates and models the known envelope:

Required on create/update:

- `type`: exactly `print`, `filament`, or `printer`;
- `version`: valid Studio semantic version text;
- `name`: supplied as the ABI argument and must be non-empty after validation.

Known optional metadata:

- `base_id`;
- `inherits`;
- `filament_id` for Filament presets;
- client `updated_time`, ignored for ordering.

All remaining keys are open-ended slicer option strings and are stored in a JSON object without interpretation. Fields owned by the Hub (`setting_id`, `user_id`, `updated_time`, `name`, and `type`) are removed from client option data and reconstructed on output.

Downloaded full entries include at least:

- `name`, `type`, `version`, `setting_id`, `base_id`, `user_id`, `updated_time`;
- `inherits` and `filament_id` when present;
- every stored open-ended option.

`base_id` is always present, including as an empty string, because Studio's loader requires the key.

### Limits

At the Hub boundary:

- maximum sum of UTF-8 key and value byte lengths: **350 KiB**, matching Studio's upload guard before JSON encoding;
- maximum key length: **256 bytes**;
- maximum value length: **64 KiB**;
- maximum name length: **255 bytes**;
- maximum **1,000 active presets per owner per tenant**, across all three types.

Exceeding the count limit returns HTTP `409` with response code `14`, allowing Studio's existing “user presets limit” behavior. Invalid shape or size returns HTTP `400`. Limits are enforced before a database write.

## Hub interface

Add authenticated routes under the existing plugin surface:

| Method and path                                 | Purpose                                                             |
| ----------------------------------------------- | ------------------------------------------------------------------- |
| `GET /api/v1/plugin/presets?bundle_version=...` | Complete metadata catalogue for the current owner.                  |
| `GET /api/v1/plugin/presets/{setting_id}`       | Full preset body for the current owner.                             |
| `POST /api/v1/plugin/presets`                   | Create and return `setting_id` plus `updated_time`.                 |
| `PATCH /api/v1/plugin/presets/{setting_id}`     | Replace the preset name/metadata/options and return `updated_time`. |
| `DELETE /api/v1/plugin/presets/{setting_id}`    | Idempotent owner-scoped deletion.                                   |

All request and response shapes use typed serde structs. The open-ended option map is the only dynamic JSON portion.

`bundle_version` is recorded in diagnostics and may be used to reject a malformed or unsupported major version; it does not filter out owned presets. Studio itself performs forward-compatibility handling, and hiding a preset from the complete catalogue could trigger local deletion.

Suggested response shape:

```json
{
  "message": "success",
  "presets": [
    {
      "setting_id": "uuid",
      "type": "filament",
      "name": "My PLA",
      "version": "2.8.1.55",
      "base_id": "...",
      "inherits": "Bambu PLA Basic @BBL X1C",
      "filament_id": "P...",
      "updated_time": 1786492800
    }
  ]
}
```

Create/update bodies carry typed metadata plus `options: BTreeMap<String, String>`.

Create is replay-safe for an ambiguous network failure: when the same owner retries the same name, type, metadata, and normalized options, POST returns the existing `setting_id` and timestamp without another mutation or audit record. The same name with different content remains `personal_preset_name_conflict`. Client `updated_time` is excluded from replay comparison.

### Hub errors

| HTTP               | Stable error                                            | Studio mapping                                                           |
| ------------------ | ------------------------------------------------------- | ------------------------------------------------------------------------ |
| `400`              | `invalid_personal_preset` / `personal_preset_too_large` | create `-7`, update `-8`, list `-9`                                      |
| `401`              | `invalid_auth_token`                                    | existing plugin account-loss/session handling, then operation failure    |
| `403`              | `role_forbidden` / `personal_presets_require_user`      | operation-specific failure                                               |
| `404`              | `personal_preset_not_found`                             | update failure; owner-scoped DELETE still succeeds                       |
| `409`              | `personal_preset_name_conflict`                         | create/update failure                                                    |
| `409` + `code: 14` | `personal_preset_limit_exceeded`                        | preserve `values_map["code"] = "14"`                                     |
| `413`              | `personal_preset_too_large`                             | create/update failure                                                    |
| `5xx`              | `internal_server_error`                                 | operation-specific failure with full redacted cause retained in Hub logs |

## Persistence

Add a backend-neutral `PersonalPresetRepository`, `personal_presets` entity, and durable owner clock.

Suggested preset table:

```text
personal_presets
- id TEXT primary key                 # Studio setting_id
- tenant_id TEXT not null
- owner_user_id TEXT not null
- preset_type TEXT not null           # print | filament | printer
- name TEXT not null
- version TEXT not null
- base_id TEXT not null
- inherits TEXT
- filament_id TEXT
- options_json TEXT not null
- updated_time BIGINT not null
- created_at TEXT not null
- updated_at TEXT not null
```

Constraints and indexes:

- composite foreign key `(tenant_id, owner_user_id)` → `users(tenant_id, id)` with cascade delete;
- unique `(tenant_id, owner_user_id, name)` because the Studio catalogue key is name-only;
- index `(tenant_id, owner_user_id, updated_time, id)`;
- validation that `preset_type` is one of the three supported values, expressed equivalently in both backend migrations or enforced through the typed repository if a portable check is not used.

Suggested clock table:

```text
personal_preset_clocks
- tenant_id TEXT not null
- owner_user_id TEXT not null
- last_updated_time BIGINT not null
- primary key (tenant_id, owner_user_id)
```

The clock row has the same composite user foreign key and cascade behavior. Create or lock it in the same write transaction as every create, replace, or delete; advance it before committing the mutation. PostgreSQL uses a locked owner row and SQLite uses its serialized write transaction. The repository exposes one backend-neutral operation and keeps dialect details below that boundary.

Create the migration in `migrations/shared/`, add only genuine backend overrides, and regenerate `migrations/sqlite/` and `migrations/postgres/` with `scripts/sync-hub-migrations.sh`.

## Plugin module design

Create a deep Rust module, `crates/pandar-network-plugin/src/personal_presets/`, whose interface owns:

- list-cycle orchestration and account-generation fencing;
- typed Hub request/response models;
- flat map validation and conversion;
- the process-local atomic drain cache;
- HTTP/error-to-ABI translation.

The C++ exports in `shim_abi_operations.hpp` and `shim_abi_user.hpp` only adapt `std::string`, nested `std::map`, and `std::function` callbacks to flat Rust FFI calls and copy returned maps/timestamps back to Studio.

Do not add preset policy, JSON parsing, URLs, or synchronization state to the C++ shim.

## Security and privacy

- Preset contents are personal data and must never be logged in full.
- Logs and audit metadata may include preset type, setting id, byte size, and a redacted or bounded name; they must not include options, G-code fields, notes, bearer tokens, or filesystem paths.
- Audit successful create/update/delete actions as `personal_preset.create`, `.update`, and `.delete`, targeting `personal_preset`.
- Reads are not audited individually.
- HTTP request/response bodies use normal authenticated transport and `cache-control: no-store`.
- The plugin cache is memory-only, scoped to the current account/configuration generation, and cleared on logout, account replacement, destroy, and failed/cancelled list cycles.

## Compatibility scope

The six personal-preset ABI functions exist in all currently catalogued Pandar ABI series (`02.06.00` through `02.08.02`). Implement the behavior once behind the shared Rust module and verify every catalogued series with its exact pinned source contract.

This feature does not change export counts or ABI signatures. It changes the disposition of operations 9–14 from explicitly unsupported to handled; all unrelated cloud settings and cloud filament surfaces remain unsupported.

## Acceptance criteria

- [ ] Exact pinned Studio for each active ABI series passes the preset ABI contract without signature/export drift.
- [ ] A preset created in Studio installation A appears after synchronization in installation B for the same Pandar user and tenant.
- [ ] Process, Filament, and Printer presets round-trip open-ended option strings plus required metadata.
- [ ] Updating a preset in one installation produces a greater Hub timestamp and converges on another installation.
- [ ] Deleting a clean preset remotely removes it locally on the next successful complete sync.
- [ ] A dirty local preset is not silently destroyed by a remote deletion or failed/partial catalogue; existing Studio behavior is preserved.
- [ ] `CheckFn=false` entries remain in the drain map and cannot be mistaken for deletion.
- [ ] Cancellation, Hub outage, malformed data, account change, and callback re-entry expose no partial or stale cache.
- [ ] Presets are inaccessible across user and tenant boundaries, including guessed `setting_id` values.
- [ ] No-auth sessions receive explicit personal-preset unavailability and never share tenant-wide preset data.
- [ ] Retrying an identical create returns the original id/timestamp; non-identical duplicate names, including the same name across different preset types, return stable errors without mutation.
- [ ] Oversize maps, invalid types, missing required metadata, and the owner quota return stable errors without mutation.
- [ ] Create/update/delete audit entries contain no preset body or secret.
- [ ] SQLite and real PostgreSQL repository/route behavior is equivalent.
- [ ] Existing printer, print, task, firmware, AMS, login, logout, and unsupported-cloud tests remain green.
- [ ] A packaged plugin is exercised in a real Studio session before compatibility documentation calls the feature passed.

## Validation commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cargo test -p pandar-core --test module_size
./scripts/sync-hub-migrations.sh --check

# Focused suites added by implementation
cargo nextest run -p pandar-hub -E 'test(personal_preset)' --no-tests=fail
cargo nextest run -p pandar-network-plugin -E 'binary(personal_presets) | binary(studio_abi_probe)' --no-tests=fail

# Required dual-backend gate
PANDAR_TEST_POSTGRES_URL=postgres://... cargo nextest run -p pandar-hub -E 'test(personal_preset)' --no-tests=fail
```

The exact focused test selectors may be adjusted to the final test binary names, but an unset PostgreSQL URL is a blocked gate, not parity evidence.

## Rollout and rollback

Rollout order:

1. equivalent additive SQLite/PostgreSQL migration;
2. Hub routes and repository;
3. network-plugin release for all active ABI series;
4. real Studio verification, then documentation claim.

An older plugin continues returning `unsupported_cloud_settings` against the new Hub. A new plugin against an older Hub receives route failures and must fail the sync cycle without exposing a partial catalogue.

Rollback disables/releases the plugin producer first, then rolls back Hub code. Leave the additive table in place; do not destructively remove personal preset data during binary rollback.

## Documentation impact

After implementation and verification:

- update `CONTEXT.md` with **Personal preset**;
- update `docs/architecture.md` with ownership and plugin/Hub flow;
- update `docs/compatibility/bambu-studio-plugin.md` with exact-version evidence and remove only the personal-preset unsupported claim;
- update `docs/development.md` with focused dual-backend and real-Studio verification;
- update `docs/roadmap.md` with completion and immediate remaining evidence.

No ADR is required at spec time. User ownership follows existing authenticated plugin identity rather than introducing a surprising new architectural choice.
