# Phase 28 Slicer Metadata

Phase 28 adds advisory slicer metadata extraction for uploaded `.3mf` and `.gcode.3mf` artifacts. The hub still treats the uploaded project file as the source artifact; parsed metadata is stored as optional JSON for inspection and is never used to override explicit print settings.

## Implemented Boundary

- Parses only allowlisted 3MF ZIP members under `Metadata/`.
- Reads `slice_info.config`, `model_settings.config`, `plate_*.json`, and plate file presence.
- Ignores unknown members and traversal paths.
- Enforces entry, member-size, total-size, plate, object, and filament caps.
- Returns no metadata for unsupported artifacts and malformed ZIP files.
- Logs parser failures with redacted context and continues dispatch.

The default plate is deterministic: `plate_*.gcode` first, then `slice_info.config`, then `plate_*.json`, then `plate_*.png`, with sorted non-zero plate ids. Lower-precedence sources can enrich missing fields but do not replace fields already found from higher-precedence sources.

## Surfaces

- `POST /api/v1/tenants/{tenant_id}/artifact-metadata-preview` accepts multipart upload data and returns `{ "metadata": ... }` or `null` without creating a job or artifact row.
- Job create, list, get, duplicate, reprint, and plugin print/list responses preserve stored artifact metadata.
- The dashboard upload form previews metadata before dispatch when available.
- Job history and recovery rows display compact slicer metadata summaries.

## Verification

Local verification covers parser precedence, malformed and unsupported files, repository round trips, duplicate/reprint reuse, invalid persisted metadata errors, tenant role enforcement, no-row preview behavior, create-job persistence, plugin response/list metadata, and frontend production build.

PostgreSQL behavior uses the same migration column and repository boundary as SQLite.

| Date | Database | Command | Result | Notes |
| --- | --- | --- | --- | --- |
| 2026-06-24 | disposable local PostgreSQL 17.10 | `PANDAR_TEST_POSTGRES_URL=<disposable-url> cargo test -p pandar-hub postgres_job_metadata_round_trips_and_reuses_artifact_when_configured -- --nocapture` | passed | Verifies artifact metadata JSON create/list/get hydration and reprint/duplicate artifact reuse through the PostgreSQL repository path. |
| 2026-06-24 | disposable local PostgreSQL 17.10 | `PANDAR_TEST_POSTGRES_URL=<disposable-url> cargo test -p pandar-hub metadata -- --nocapture` | passed | 26 metadata tests passed, including the PostgreSQL metadata repository test plus parser, route, plugin, and migration parity coverage. |
