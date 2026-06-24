# Phase 28: Reference-Backed Slicer Metadata

Status: Draft for SDD reviewer gate
Date: 2026-06-24

## Goal

Extract safe, advisory metadata from Bambu-style 3MF project artifacts so operators can inspect practical print information before dispatch and after job creation. The Hub must not become a slicer: it only reads bounded metadata files from uploaded archives, stores the parsed summary when a job is created, and never lets parsed values override explicit user settings or compatibility rules.

## Reference Evidence

`reference/bambuddy/backend/app/api/routes/printers.py` implements plate inspection by opening `.3mf` files as ZIP archives and reading only known metadata members:

- `Metadata/plate_*.gcode` as the primary plate-index signal.
- `Metadata/plate_*.json` and `Metadata/plate_*.png` as fallback plate signals.
- `Metadata/model_settings.config` for plate display names through `<metadata key="plater_id">` and `<metadata key="plater_name">`.
- `Metadata/slice_info.config` for plate index, estimated print time, estimated weight, filament hints, and object names.
- `Metadata/plate_N.json` for object names under `bbox_objects`.

`reference/bambuddy/backend/app/services/archive.py` keeps this behavior best-effort: metadata parsing may return partial data with warnings, and file-name display stems strip `.gcode.3mf`, `.3mf`, and `.gcode`.

Pandar Phase 28 follows the same evidence boundary: read Bambu 3MF metadata members, return partial summaries, and ignore unsupported archive contents rather than executing slicer logic or interpreting arbitrary G-code.

## Scope

In scope:

- Add a narrow Hub parser for Bambu/3MF project metadata.
- Parse plate count, plate IDs, default/selected plate hints, filename-derived display name, plate names, object-name summaries, material/filament hints, estimated print time, and estimated filament weight when safely available.
- Add an operator metadata-preview endpoint that accepts a project artifact without storing it, so the dashboard can show metadata before dispatch.
- Persist parsed metadata with `job_artifacts` when a job is created through the dashboard API or plugin API.
- Include persisted metadata in Hub job list/detail responses and plugin job/print responses where it is useful.
- Show metadata in the dispatch form and job history without changing the existing explicit `plate_id`, AMS, calibration, or timelapse inputs.
- Keep `.gcode`, unknown files, malformed archives, and unsupported 3MF layouts accepted by the existing upload flow when the artifact itself is otherwise valid.
- Add no-network fixtures and tests for parser behavior, route behavior, repository persistence, and UI rendering/build.

Out of scope:

- Slicing, re-slicing, geometry processing, mesh inspection, thumbnail extraction, object positioning, or G-code execution.
- New compatibility decisions based on parsed metadata.
- Automatically changing `plate_id`, material mapping, AMS settings, or dispatch defaults on the server.
- Blocking job creation only because metadata parsing failed.
- Storing raw uploaded metadata files separately from the existing artifact.
- Parsing arbitrary XML outside the known Bambu metadata members.

## Metadata Shape

Add a typed Rust metadata model in the Hub parser boundary and expose the same JSON shape in API responses:

```json
{
  "source": "bambu_3mf",
  "display_name": "Project name",
  "default_plate_id": 1,
  "plate_count": 2,
  "plates": [
    {
      "plate_id": 1,
      "name": "Plate 1",
      "object_count": 3,
      "objects": ["part-a", "part-b"],
      "estimated_time_seconds": 3600,
      "filament_weight_grams": 18.5,
      "filaments": [
        {
          "filament_id": "1",
          "type": "PLA",
          "color": "#ffffff",
          "used_grams": 18.5,
          "used_meters": 6.2
        }
      ],
      "has_thumbnail": true
    }
  ],
  "warnings": ["metadata_file_too_large"]
}
```

Rules:

- `metadata` is nullable in API responses and storage.
- `source` is currently `bambu_3mf` only.
- `display_name` is always derived from the uploaded filename stem for Phase 28. It strips path separators and the suffixes `.gcode.3mf`, `.3mf`, and `.gcode`, following the reference archive display-stem behavior. No project XML member is parsed for project name in this phase because the checked reference evidence does not provide a narrower project-name source than known slicer metadata files.
- `default_plate_id` is advisory and deterministic:
  - collect plate IDs from `Metadata/plate_*.gcode`;
  - if none exist, collect plate IDs from `<plate>` entries in `Metadata/slice_info.config`;
  - if none exist, collect plate IDs from `Metadata/plate_*.json`;
  - if none exist, collect plate IDs from `Metadata/plate_*.png`;
  - sort numeric IDs ascending, drop invalid or zero IDs, and use the lowest ID as `default_plate_id`;
  - when sources disagree, keep the union for `plates` but do not let lower-precedence sources replace fields already parsed from higher-precedence sources for the same plate.
- The existing submitted `plate_id` remains authoritative.
- Numeric estimates are advisory and may be absent.
- `warnings` are stable low-cardinality strings, not raw parser errors or file paths.
- Object and filament lists are capped to keep responses bounded.

## Parser Design

Create a Hub-owned parser module, for example `crates/pandar-hub/src/artifacts/metadata.rs`, because uploaded artifact bytes are already staged in Hub routes and the agent does not need this metadata to print.

Dependency changes:

- Add `zip` to workspace dependencies and `pandar-hub` dependencies for bounded ZIP archive reading.
- Add `quick-xml` to workspace dependencies and `pandar-hub` dependencies for event-based XML parsing of the known Bambu config files.
- Do not add frontend ZIP/XML parsing dependencies; the browser asks the Hub preview endpoint for authoritative metadata.

Parser input:

- `filename`
- `content_type`
- staged artifact path

Parser behavior:

- Treat `.3mf` and `.gcode.3mf` filenames or `model/3mf` content type as candidate 3MF artifacts.
- Return `Ok(None)` for `.gcode`, unknown filenames, unknown content types, non-ZIP files, archives without known metadata members, or malformed metadata.
- Open ZIP archives read-only and inspect only member names under `Metadata/`.
- Read only:
  - `Metadata/slice_info.config`
  - `Metadata/model_settings.config`
  - `Metadata/plate_*.json`
  - `Metadata/plate_*.png` presence only
  - `Metadata/plate_*.gcode` presence only
- Never read arbitrary archive paths, absolute paths, parent-directory paths, symlinks, or member payloads outside the allowlist.
- Limit inspected entry count, per-member uncompressed bytes, total parsed metadata bytes, plates, objects per plate, and filaments per plate.
- On limit hits, keep partial data and add a warning when useful.
- Log unexpected parser failures at debug or warn level with full error context using `{err:#}`, but do not expose raw errors in API responses.

Display names:

- The artifact-level `display_name` is derived from the upload filename only.
- Plate-level `name` may come from `Metadata/model_settings.config` plate metadata where `plater_id` matches a numeric plate ID and `plater_name` is present.
- If no plate-level name exists, use `Plate <plate_id>`.

Suggested limits:

- ZIP entries inspected: 512.
- XML/JSON metadata member payload: 1 MiB each.
- Total metadata payload read: 4 MiB.
- Plates returned: 64.
- Objects per plate returned: 32.
- Filaments per plate returned: 32.

XML parsing:

- Use `quick-xml` event parsing.
- Extract only tag/attribute data needed for plate, metadata, object, and filament summaries.
- Ignore entity expansion and unsupported nodes.
- Unknown attributes are ignored.

JSON parsing:

- Use `serde_json` for `Metadata/plate_N.json`.
- Extract `bbox_objects[].name` when present.
- Ignore all other structure.

## API Design

Add:

`POST /api/v1/tenants/{tenant_id}/artifact-metadata-preview`

Request:

- `multipart/form-data`
- file field named `file` or `artifact`
- optional `filename` and `content_type` text fields, matching the existing print-job upload conventions

Response:

```json
{
  "metadata": { "...": "..." }
}
```

Rules:

- Caller must have `Operator` role for the tenant.
- Viewer-only tokens receive the existing authorization error.
- The same artifact size limit and multipart validation errors as job creation apply: `artifact_invalid_upload`, `artifact_empty`, and `artifact_too_large`.
- Unsupported or malformed metadata returns `200` with `"metadata": null`.
- The endpoint does not create `job_artifacts`, `jobs`, `commands`, audit events, or stored artifact files.
- Temporary staged files are always removed.
- Preview must not call `prepare_print_job` and must not require `printer_id`, `plate_id`, `use_ams`, `flow_cali`, `timelapse`, `ams_mapping`, or `ams_mapping2`.
- Refactor the multipart module so preview and job create share only file staging, duplicate-file rejection, text-field byte limits, filename/content-type extraction, size checks, empty-file checks, and cleanup. Job creation keeps the existing print-job field requirements in its own preparation path.

Update:

- `POST /api/v1/tenants/{tenant_id}/printers/{printer_id}/jobs`
- `POST /api/v1/plugin/prints`

Both creation paths parse metadata from the staged artifact before storage cleanup and pass the optional metadata JSON into the repository create path. Parser failure must not block create. The existing `plate_id` request field stays authoritative.

Update job responses:

```json
{
  "artifact": {
    "id": "...",
    "tenant_id": "...",
    "filename": "...",
    "content_type": "model/3mf",
    "size_bytes": 123,
    "created_at": "...",
    "metadata": null
  }
}
```

Plugin responses:

- `PluginPrintResponse` includes optional `artifact_metadata` for the newly created print.
- `PluginJobResponse` includes optional `artifact_metadata` for listed jobs.

The plugin response names intentionally avoid returning storage paths or internal artifact IDs.

## Persistence Design

Add nullable metadata storage to `job_artifacts` for both SQLite and PostgreSQL:

```sql
ALTER TABLE job_artifacts ADD COLUMN metadata_json TEXT;
```

Repository changes:

- Extend `pandar_core::JobArtifact` with `metadata_json: Option<String>`.
- Extend SeaORM `job_artifacts` entity with `metadata_json: Option<String>`.
- Extend `CreatePrintJob` with `artifact_metadata_json: Option<String>`.
- Insert, hydrate, list, get, duplicate, retry, reprint, and agent artifact access paths must continue to work with the new nullable column.
- Duplicating or reprinting a job keeps the original artifact metadata because it references or copies the same artifact behavior already established by existing repository methods.

Validation:

- The repository accepts only metadata JSON generated by the Hub parser or `None`.
- Route response conversion parses persisted metadata JSON into `serde_json::Value`; if persisted metadata is unexpectedly invalid, return a repository/data error with full context. Parser-generated metadata must be valid by construction, and persisted corruption must not be silently hidden as `metadata: null`.

## Frontend Design

Update `frontend/app/dispatch-form.tsx`:

- When an operator selects a file, call the Next proxy route for metadata preview.
- Show a compact metadata summary before dispatch:
  - display name,
  - plate count,
  - detected/default plate,
  - selected plate estimate if available,
  - filament estimate and material hints when available.
- Keep the existing `plate_id` input editable and authoritative.
- Do not silently overwrite `plate_id`; an advisory button or small control may copy the detected default plate into the input only through explicit user action.
- Show unsupported or unavailable metadata as neutral text, not an error state.
- Keep dispatch enabled for valid files even when preview metadata is null.

Add a Next API proxy route:

`frontend/app/api/tenants/[tenantId]/artifact-metadata-preview/route.ts`

It forwards multipart requests to the Hub endpoint with existing `apiHeaders()`, `duplex: 'half'`, and response-header behavior matching the current job-upload proxy.

Update dashboard types and job history:

- Add `artifact.metadata`.
- Display a compact persisted metadata summary in `JobHistory` and the recovery/job cards where it helps identify the project.
- Keep dense operational layout; no marketing-style panels.

## Documentation

Update:

- `docs/roadmap.md`: mark Phase 28 progress/completion and define the next phase after metadata.
- `docs/development.md` or `docs/architecture.md`: document that slicer metadata is advisory, parser failures are non-blocking, and explicit dispatch fields remain authoritative.
- Add `docs/compatibility/phase-28-slicer-metadata.md` with:
  - reference files and extraction rules,
  - parser limits,
  - API response shape,
  - no-network verification evidence,
  - real slicer/project fixture status.

## Tests And Verification

Required parser tests:

- `.gcode` returns `None`.
- malformed `.3mf`/non-ZIP returns `None` without bubbling an upload error.
- slice-info fixture extracts plate count, plate IDs, estimated time, estimated weight, object names, and filament hints.
- model-settings fixture extracts plate display names.
- plate JSON fixture extracts fallback object names.
- oversized metadata member returns partial metadata with a stable warning.
- unknown ZIP members and path-traversal-like member names are ignored.

Required repository tests:

- create/list/get preserve artifact metadata JSON.
- missing metadata remains `None`.
- both SQLite and PostgreSQL repository test paths cover the new column through existing backend-neutral helpers.

Required route tests:

- preview endpoint authorizes operators and rejects viewers.
- preview endpoint succeeds with only a `file` field plus optional `filename` and `content_type`.
- preview endpoint does not require printer, plate, AMS, calibration, timelapse, or mapping fields.
- preview endpoint returns `metadata: null` for unsupported artifacts.
- preview endpoint returns parsed metadata for a fixture 3MF.
- preview endpoint cleans staged uploads after success and validation failures.
- preview endpoint creates no `job_artifacts`, `jobs`, `commands`, or audit rows.
- job create persists parsed metadata but still succeeds when parsing returns null.
- plugin print/list responses include artifact metadata when present.

Required frontend checks:

- Dispatch form calls preview route on file selection and renders advisory metadata.
- Dispatch still posts the explicit `plate_id`.
- Build passes.

Required verification commands after implementation:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
npm --prefix frontend run build
git diff --check
```
