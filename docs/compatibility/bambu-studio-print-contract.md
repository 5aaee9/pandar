# Bambu Studio 02.08.01.55 Print Contract

This document is the canonical contract for Bambu Studio print submission and task metadata through
Pandar's supported paths:

```text
ordinary print: Bambu Studio -> Pandar network plugin -> Hub -> Agent -> printer
model-task read: Bambu Studio -> Pandar network plugin -> Hub
```

It is derived only from Bambu Studio commit
`ba049f6a2e08c3b6033660bb84da80c08722974b` (tag `v02.08.01.55`). The checked-out reference may
advance; a live `reference/BambuStudio` line is not evidence for this contract unless it is read with
`git show ba049f6a2e08c3b6033660bb84da80c08722974b:<path>`.

Implementation status: final16 implements this field, lifecycle, cancellation, stable-id,
Hub-backed `get_user_tasks`, plate/subtask, caller-owned model-task, and explicit slice-unavailable
contract. It no longer uses the historical synthetic empty task-page success. Its model-task
implementation has deterministic compiled-consumer evidence and a controlled official-AppImage
request/response/callback observation. Historical frozen build-input archive
`pandar-bambu-final12-019f7b10.tar.gz` contains 1,543 regular files, is 2,740,698 bytes, and has
SHA-256 `17371828ef7a26cace73cfbed321d094bf38323670e8fa6ccf69d6cbfd4b7eee`, canonical tree
SHA-256 `5aa0038dbc3f0962cc172646876263b0db04e1e6df5fbe571553af1967f242a6`, and member-list
SHA-256 `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`.
The same immutable input passed the historical Windows clean gate and disposable PostgreSQL contract
gate recorded below. Subsequent Linux validation exposed a background-refresh/firmware-callback race,
so final12 and final11 evidence cannot promote the current candidate. Historical final13 contains that
repair and passed its Windows, PostgreSQL, corrected Linux native/ASan, and exact-AppImage
module-load/development-no-auth recovery gates. Final14 remains historical post-final13 capability and
Better Auth return-intent evidence. Final15 is non-promotable pre-correction evidence. Final16 is the
current verified Linux compatibility baseline.

Final16's selected-target rule is exact: a Cloud target is selected or explicitly subscribed, and
heartbeat uses the deduplicated union. Removing either ownership source retains the target while the
other remains. Only loss of both retires Cloud initialization, notifications, and Cloud tickets; that
retirement must not alter the virtual-local generation or any Local ticket.

Automated tests for this contract use a loopback Hub and fake Agent/printer boundaries. They are not
evidence that a real printer started a print and must never submit one. A separate exact-Studio
desktop smoke may prove module loading and account/session behavior without crossing the hardware
print boundary.

## Disposition Vocabulary

Every `BBL::PrintParams` field has exactly one top-level disposition:

- `preserve`: retain the value, or a documented typed derivation of it, through every boundary that
  owns the behavior. Invalid typed input is rejected; the producer-defined empty-string mapping
  sentinel described below is converted to the typed empty collection rather than treated as invalid.
- `default`: Studio may populate the field, but the Hub-backed route does not own its raw value.
  Admission must explicitly validate or scrub it as described below. `default` never means an
  accidental silent discard.
- `reject`: the non-default value is not legal on the supported Hub print route. Reject before
  opening the artifact or making an HTTP request with `BAMBU_NETWORK_ERR_INVALID_RESULT` (`-19`),
  `PrintingStageERROR`, and `{"error":"invalid_print_param","field":"<field>"}`.
- `unsupported`: the field requests behavior for which Pandar has no exact typed implementation.
  Reject before opening the artifact or making an HTTP request with
  `BAMBU_NETWORK_ERR_INVALID_RESULT` (`-19`), `PrintingStageERROR`, and
  `{"error":"unsupported_print_param","field":"<field>"}`.

Passwords, access material, IP addresses, and local paths must not appear in Hub requests, persisted
job metadata, callback bodies, diagnostics, stdout, or stderr. Rejection bodies contain only the
stable error and field name, never the rejected value.

## `PrintParams` Field Contract

The field order is the ABI order in pinned
`src/slic3r/Utils/bambu_networking.hpp:199-250`. Pinned `PrintJob.cpp:214-390` and
`PrintJob.hpp:50-115` prove the ordinary print producer; `SelectMachine.cpp:3136-3143,3165-3201`
proves that calibration, nozzle, AMS, internal-timelapse, and eMMC values are normal Studio input.

|   # | Field                         | Disposition   | Admission and ownership contract                                                                                                                                                                                                                                                                                                                                 |
| --: | ----------------------------- | ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|   1 | `dev_id`                      | `preserve`    | Resolve the Studio device id/serial to an authorized Hub printer. Forward only the Hub printer id internally; return the Studio id in task JSON. Empty or unknown is invalid.                                                                                                                                                                                    |
|   2 | `task_name`                   | `preserve`    | Preserve as task display metadata. Do not collapse it into `project_name`.                                                                                                                                                                                                                                                                                       |
|   3 | `project_name`                | `preserve`    | Preserve independently as project metadata.                                                                                                                                                                                                                                                                                                                      |
|   4 | `preset_name`                 | `preserve`    | Preserve as profile/preset metadata.                                                                                                                                                                                                                                                                                                                             |
|   5 | `filename`                    | `preserve`    | This is a plugin-local artifact source path. Read that file, send its bytes, and preserve only its basename/typed artifact metadata. Never send or persist the local path.                                                                                                                                                                                       |
|   6 | `config_filename`             | `preserve`    | This is plugin-local input for typed slice/config metadata. Preserve only parsed typed values; never send or persist the local path.                                                                                                                                                                                                                             |
|   7 | `plate_index`                 | `preserve`    | Preserve the positive final ABI value end to end and persist it as the Studio plate index. Pinned `PrintJob.cpp:191-204,259` converts zero-based `job_data.plate_idx` to this one-based value, including calibration jobs; reject `0`, negatives, and overflow.                                                                                                  |
|   8 | `ftp_folder`                  | `default`     | Accept Studio's normal cloud value, use only for local admission if needed, then scrub. It gives no authority to bypass Hub.                                                                                                                                                                                                                                     |
|   9 | `ftp_file`                    | `reject`      | Empty is the only legal Hub-route value. A non-empty value requests a different file-transfer operation.                                                                                                                                                                                                                                                         |
|  10 | `ftp_file_md5`                | `reject`      | Empty is the only legal Hub-route value. Pandar does not claim an ignored checksum was verified.                                                                                                                                                                                                                                                                 |
|  11 | `nozzle_mapping`              | `preserve`    | Parse as its known JSON schema and preserve typed data end to end. Pinned Studio's exact empty-string sentinel means the typed empty array; reject every other malformed or schema-invalid value.                                                                                                                                                                |
|  12 | `ams_mapping`                 | `preserve`    | Parse as its known JSON schema and preserve typed data end to end. Pinned Studio's exact empty-string sentinel means the typed empty array; reject every other malformed or schema-invalid value.                                                                                                                                                                |
|  13 | `ams_mapping2`                | `preserve`    | Parse as its known JSON schema and preserve typed data end to end. Pinned Studio's exact empty-string sentinel means the typed empty array; reject every other malformed or schema-invalid value.                                                                                                                                                                |
|  14 | `ams_mapping_info`            | `preserve`    | Parse as its known JSON schema and preserve typed data end to end. Pinned Studio's exact empty-string sentinel means the typed empty array; reject every other malformed or schema-invalid value.                                                                                                                                                                |
|  15 | `nozzles_info`                | `preserve`    | Parse as its known JSON schema and preserve typed data as job metadata and in a printer command only where the exact printer contract consumes it. Pinned Studio's exact empty-string sentinel means the typed empty array; reject every other malformed or schema-invalid value.                                                                                |
|  16 | `connection_type`             | `preserve`    | Validate at the plugin boundary. Canonical status projects `cloud`; before the first status, pinned Studio may pass its exact empty default through the cloud print entrypoint, which is normalized to `cloud`. The supported route rejects every other value and still uses plugin -> Hub -> Agent; it never selects direct Agent/MQTT or Bambu cloud behavior. |
|  17 | `comments`                    | `preserve`    | Preserve as typed task metadata; do not interpret it as printer policy.                                                                                                                                                                                                                                                                                          |
|  18 | `origin_profile_id`           | `preserve`    | Preserve as Studio profile metadata. It may become `profileId` when positive.                                                                                                                                                                                                                                                                                    |
|  19 | `stl_design_id`               | `preserve`    | Preserve as Studio design metadata and expose it as numeric `designId`.                                                                                                                                                                                                                                                                                          |
|  20 | `origin_model_id`             | `preserve`    | Preserve as opaque Studio model metadata.                                                                                                                                                                                                                                                                                                                        |
|  21 | `print_type`                  | `preserve`    | Validate at the plugin boundary. The supported submission accepts `from_normal`; `from_sdcard_view` belongs to the explicitly unsupported SD-card entrypoint.                                                                                                                                                                                                    |
|  22 | `dst_file`                    | `unsupported` | Any non-empty value requests SD-card destination behavior, which is not implemented by this route.                                                                                                                                                                                                                                                               |
|  23 | `dev_name`                    | `preserve`    | Preserve as submitted metadata only when useful for audit; task output should prefer the authoritative joined printer name.                                                                                                                                                                                                                                      |
|  24 | `dev_ip`                      | `default`     | Accept Studio's normal value, then scrub it. It is not Hub printer identity and must not be logged, sent, or persisted.                                                                                                                                                                                                                                          |
|  25 | `use_ssl_for_ftp`             | `default`     | Accept and scrub. Hub/Agent transport policy is authoritative.                                                                                                                                                                                                                                                                                                   |
|  26 | `use_ssl_for_mqtt`            | `default`     | Accept and scrub. Hub/Agent transport policy is authoritative.                                                                                                                                                                                                                                                                                                   |
|  27 | `username`                    | `default`     | Accept Studio's normal local username and scrub it. Do not log, send, or persist it.                                                                                                                                                                                                                                                                             |
|  28 | `password`                    | `default`     | Accept and immediately scrub this secret. Do not log, send, persist, or place it in an error/callback.                                                                                                                                                                                                                                                           |
|  29 | `task_bed_leveling`           | `preserve`    | Preserve as typed printer behavior end to end.                                                                                                                                                                                                                                                                                                                   |
|  30 | `task_flow_cali`              | `preserve`    | Preserve as typed printer behavior end to end.                                                                                                                                                                                                                                                                                                                   |
|  31 | `task_vibration_cali`         | `preserve`    | Preserve as typed printer behavior end to end; do not hard-code `false`.                                                                                                                                                                                                                                                                                         |
|  32 | `task_layer_inspect`          | `preserve`    | Preserve as typed printer behavior end to end; do not hard-code `false`.                                                                                                                                                                                                                                                                                         |
|  33 | `task_record_timelapse`       | `preserve`    | Preserve as typed printer behavior end to end.                                                                                                                                                                                                                                                                                                                   |
|  34 | `task_timelapse_use_internal` | `preserve`    | Preserve end to end. Pinned `ProjectTask.hpp:226` defines it as task `cfg` bit 2.                                                                                                                                                                                                                                                                                |
|  35 | `task_use_ams`                | `preserve`    | Preserve as typed printer behavior end to end.                                                                                                                                                                                                                                                                                                                   |
|  36 | `task_bed_type`               | `preserve`    | Preserve one of the five concrete values emitted by pinned `bed_type_to_gcode_string` (`supertack_plate`, `cool_plate`, `eng_plate`, `hot_plate`, or `textured_plate`) end to end. Reject `unknown`, legacy `auto`, and arbitrary strings instead of silently replacing them.                                                                                    |
|  37 | `extra_options`               | `reject`      | Empty is the only legal value until an exact typed schema exists. Do not accept open-ended JSON or strings.                                                                                                                                                                                                                                                      |
|  38 | `auto_bed_leveling`           | `preserve`    | Preserve end to end and accept only the pinned modes `0`, `1`, or `2`.                                                                                                                                                                                                                                                                                           |
|  39 | `auto_flow_cali`              | `preserve`    | Preserve end to end and accept only the pinned modes `0`, `1`, or `2`.                                                                                                                                                                                                                                                                                           |
|  40 | `auto_offset_cali`            | `preserve`    | Preserve end to end and accept only the pinned modes `0`, `1`, or `2`.                                                                                                                                                                                                                                                                                           |
|  41 | `extruder_cali_manual_mode`   | `preserve`    | Preserve end to end and accept only `-1`, `0`, or `1`. Pinned `SelectMachine.cpp:3190-3201` normally emits `0` or `1`.                                                                                                                                                                                                                                           |
|  42 | `task_ext_change_assist`      | `unsupported` | `false` is admitted. The status projection clears pinned Studio `fun` bit 48 so the checkbox remains hidden; a caller-supplied `true` is still rejected until an exact downstream command encoding is proven.                                                                                                                                                    |
|  43 | `try_emmc_print`              | `preserve`    | Preserve as transfer policy: `false` prohibits BRTC/eMMC, while `true` permits it only when the Agent reports exact support. It never guarantees eMMC use.                                                                                                                                                                                                       |
|  44 | `svc_context`                 | `preserve`    | Preserve as opaque Hub task metadata; never interpret it as printer policy.                                                                                                                                                                                                                                                                                      |
|  45 | `slicer_uid`                  | `preserve`    | Preserve as opaque Hub task metadata; never send it to the printer.                                                                                                                                                                                                                                                                                              |

`nozzle_mapping`, `ams_mapping`, `ams_mapping2`, `ams_mapping_info`, and `nozzles_info` are typed JSON
arrays. Pinned `PrintJob.hpp:65`, `PrintJob.cpp:266`, and `SelectMachine.cpp:1354-1367,3163-3168`
show that the exact producer leaves these strings empty when no nozzle or AMS mapping is available;
that exact empty string is the producer-defined typed empty array and is serialized onward as `[]`.
A non-empty parse failure, a type mismatch, or an unknown field where the schema is closed is an
invalid parameter. Converting any other invalid value to an empty array, omitting the multipart part,
or accepting a successful print is forbidden.

## Submission Stages And Truth Boundary

Pinned stages are defined in `bambu_networking.hpp:153-163`. `PrintJob.cpp:397-492` converts them to
Studio progress/error UI, and `PrintJob.cpp:502-553` consumes the wait callback. Pandar must emit the
following monotonic sequence; it must not emit callbacks while holding an internal state lock.

| Stage                            | Observable fact                                                                                                                                           | Facts that are not yet proven                                                                               |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `PrintingStageCreate` (`0`)      | Parameters, identity, authorization, and printer admission passed.                                                                                        | No artifact byte has reached Hub.                                                                           |
| `PrintingStageUpload` (`1`)      | Artifact bytes are being streamed plugin -> Hub. The callback code is bounded progress `0..100`.                                                          | No durable Hub job exists until the request commits.                                                        |
| `PrintingStageWaiting` (`2`)     | Hub returned `201` after atomically persisting artifact, job, command, and stable Studio id. Hub state is `JobStatus::Queued` and `PrintStatus::Pending`. | The Agent has not accepted the command; the printer has not received it; physical printing has not started. |
| `PrintingStageSending` (`3`)     | Hub reports `JobStatus::Acknowledged`; the Agent accepted the command and started work.                                                                   | Printer upload/MQTT publish and physical printing are not proven.                                           |
| `PrintingStageRecord` (`4`)      | Hub reports persisted `JobStatus::Succeeded`, meaning the Agent uploaded the artifact to the machine and published the typed print command.               | A publish result is delivery completion, not evidence that the printer entered a printing state.            |
| `PrintingStageWaitPrinter` (`5`) | Pandar is about to invoke `OnWaitFn` outside locks with the stable Studio job id.                                                                         | The callback can return on timeout/cancel in pinned Studio and therefore cannot prove physical start.       |
| `PrintingStageFinished` (`6`)    | Record/delivery completed and `OnWaitFn` returned `true`. Preserve Studio's success body `"3"`.                                                           | This means "successfully sent", never "physical printing started".                                          |
| `PrintingStageERROR` (`7`)       | A pre-terminal admission, upload, Hub, Agent, polling, or wait failure occurred; the callback code is the stable negative error.                          | A post-`201` error does not erase or cancel a durable job by itself.                                        |

The successful callback order is exactly `Create -> Upload* -> Waiting -> Sending -> Record ->
WaitPrinter -> Finished`. An HTTP `201` may emit `Waiting`; it must not immediately emit `Finished`
or call `OnWaitFn`. Downstream Agent failure must emit `ERROR`, not `Finished`.

`OnWaitFn` receives `BAMBU_NETWORK_SUCCESS` and a JSON object with a numeric stable id:

```json
{ "job_id": 38191 }
```

Additional Pandar identifiers may be present under separately named keys, but `job_id` remains a
positive JSON integer. The callback is invoked exactly once after `Record`. A `false` result is a
wait failure and prevents `Finished`.

The physical start boundary is later printer telemetry that transitions persisted `PrintStatus` to
`Running`. It is exposed by task lookup but never inferred from Hub `201`, Agent acknowledgement, or
MQTT publish success. Conversely, later physical `Failed` or `Cancelled` telemetry does not turn a
persisted `JobStatus::Succeeded` delivery into a submission error: the submission still proceeds
through `Record -> WaitPrinter -> Finished`, while task lookup exposes the separate physical fact.

## Cancellation

Pinned `bambu_networking.hpp:14-46` defines `BAMBU_NETWORK_ERR_CANCELED` as `-18`, and
`PrintJob.cpp:631-660` handles `-18` separately from generic errors. Pandar therefore returns exactly
`-18` after a confirmed cancellation; it must not return `-19` for cancellation.

The plugin polls `WasCancelledFn` before `Create`, for every upload chunk, during queued/acknowledged
polling, and immediately before and after `OnWaitFn`:

- Before durable Hub creation, abort the upload and create no job.
- While queued and before Agent claim, Hub cancellation atomically prevents leasing and marks the
  command/job cancelled.
- After Agent acknowledgement, return `-18` only after Hub/Agent confirms typed cancellation before
  printer publish. Otherwise return a stable `cancel_too_late` error and keep the task visible.
- After publish, the submission is delivered. A physical printer abort is a separate correlated
  Hub -> Agent operation; submission cancellation must not pretend to perform it.

A successful cancellation response must echo the same positive `studio_submission_id` and the exact
`JobStatus::Cancelled` plus `PrintStatus::Cancelled` pair. A mismatched id, one-sided state, or
malformed response is an invalid response and must never produce `-18`.

If account/printer snapshot freshness changes after Hub `201`, the plugin must either confirm a safe
Hub cancellation or treat the submission as accepted and visible. Returning failure while leaving a
queued/woken job is forbidden because it creates a ghost-print race. If a cancellation request fails
and the snapshot became stale during that request, retain the durable job as accepted and visible.

## Stable Studio Identity

Every accepted submission receives one stable `studio_submission_id` in the range
`1..=2147483647`. It is unique within a tenant, survives process restarts, and never changes when the
internal UUID, command, job status, or printer telemetry changes.

Use this id consistently for:

- `get_user_tasks.hits[].id`;
- `OnWaitFn`'s numeric `job_id`;
- Studio-facing task/subtask identity in the printer command;
- `get_task_plate_index` and `get_subtask_info` lookup.

Internal Hub UUIDs remain internal. In the task-list JSON contract only, `profileId` is the positive
`origin_profile_id` when supplied; otherwise it is `studio_submission_id`, never `0`. `designId` is
the positive `stl_design_id` or `0`. The printer command must not replace these values with unrelated
per-call synthetic ids. This task-list fallback does not apply to the model-task overload below.

## Caller-Owned Model-Task Contract

Pinned `StatusPanel.cpp:4145-4162` calls the `bambu_network_get_subtask(BBLModelTask*, callback)`
overload, whose eight-field layout is defined by pinned `ProjectTask.hpp:154-167`. This is distinct
from the JSON `bambu_network_get_subtask_info` entrypoint. Studio owns the `BBLModelTask`; Pandar must
neither replace nor retain ownership of that pointer.

For an ordinary, authorized Pandar submission, Hub returns exactly these fields:

| Field                                    | Value                                                                   |
| ---------------------------------------- | ----------------------------------------------------------------------- |
| `job_id`, `task_id`                      | The same canonical positive `studio_submission_id` requested by Studio. |
| `design_id`, `profile_id`, `instance_id` | `0`.                                                                    |
| `model_id`                               | Empty string.                                                           |
| `model_name`                             | The real nonempty persisted project name.                               |
| `profile_name`                           | The real nonempty persisted preset name.                                |

`instance_id=0` is an explicit no-rating sentinel. It must never be filled with the submission id or
another synthetic value. Any MakerWorld marker (`stl_design_id != 0`, `origin_profile_id != 0`, or a
nonempty `origin_model_id`) requires rating/model metadata Pandar does not own and therefore returns
Hub `409 studio_model_task_metadata_unavailable`. Missing, unauthorized, malformed, or semantically
unusable metadata is also non-success; no field is invented.

The Hub lookup key is the token-derived tenant plus canonical Studio submission id and requires the
current plugin session. It uses the same backend-neutral repository query on SQLite and PostgreSQL;
there is no per-user task-ownership rule and no backend-specific response behavior.

A valid ABI call returns `0` when the request is admitted to the persistent worker. This is an
asynchronous admission result, not a synchronous HTTP-success result. After successful authorized
retrieval, the worker writes all eight fields into the same caller-owned object and invokes the exact
callback once. Hub 409/404, malformed successful JSON, and stale account/configuration leave the
object untouched and invoke no callback. Cancellation or destroy observed before the response/
callback gate has the same outcome. Destroy waits for a callback that already won the gate and
guarantees no callback after destroy returns. Cancellation interrupts pending initial/retry
model-task GET, no-auth-session POST through its response body, pending/direct revocation DELETE,
and same-key rotation-follower waits. Once a successful no-auth response is available, candidate
persistence plus rotation/revocation bookkeeping drains to a consistent state. Cancellation after
server delivery but before that response remains the ordinary unknowable HTTP-create outcome; the
same in-process create is not automatically retried. Cross-process locking, persistence, and fsync
have no hard real-time bound.

## Task Query Contract

Pinned `TaskManager.cpp:321-381` consumes `get_user_tasks`, and `WebViewDialog.cpp:1389-1406`
forwards the same top-level object to print history. `TaskQueryParams` is defined at
`bambu_networking.hpp:252-258`.

`get_user_tasks` honors `dev_id`, `status`, `offset`, and `limit` at the tenant-authorized server
boundary. `total` is the count after filters and before pagination. Results use deterministic newest
first ordering. The exact Studio-facing shape is:

```json
{
  "total": 1,
  "hits": [
    {
      "id": 38191,
      "status": 1,
      "designId": 0,
      "title": "gearbox.3mf",
      "deviceName": "Workshop X1C",
      "deviceId": "01P00A000000001",
      "cover": "",
      "startTime": "2026-07-20T12:00:00Z",
      "endTime": "",
      "profileId": 38191
    }
  ]
}
```

`id`, `status`, `designId`, and `profileId` are JSON integers. `deviceId` is the Studio device
id/serial, not a Hub UUID. If `designId > 0`, emit `designTitle`; otherwise emit `title`. Emit `cover`
only as a real tenant-authorized URL; the safe default is the empty string.

Studio status mapping is deterministic:

| Pandar state                                                                    |            Studio `status` |
| ------------------------------------------------------------------------------- | -------------------------: |
| `PrintStatus::Completed`                                                        |              `2` (success) |
| `JobStatus::Failed`, `PrintStatus::Failed`, or `PrintStatus::Cancelled`         |               `3` (failed) |
| queued, sent, acknowledged, delivery-succeeded-but-pending, stalled, or running | `1` (printing/in progress) |

Status `4` is reserved until a distinct pinned semantic is proven. A Hub outage, invalid JSON, or
authorization failure returns non-success; it must not become `{"total":0,"hits":[]}`.

## Plate, Subtask, And Slice Contract

`get_task_plate_index(task_id, out)` returns `0` and the persisted `plate_index` for a known stable
Studio id. An unknown, unauthorized, or unavailable task returns non-success and leaves `out` at
`-1`; success with `-1` is forbidden.

Pinned `DeviceManager.cpp:3886-3985` consumes `get_subtask_info`. A known stable subtask id returns
HTTP `200` and this exact type shape:

```json
{
  "content": "{\"info\":{\"plate_idx\":7}}",
  "context": {
    "plates": [
      {
        "index": 7,
        "thumbnail": { "url": "" },
        "prediction": 3600,
        "weight": 12.5,
        "filaments": [
          {
            "color": "#FFFFFFFF",
            "type": "PLA",
            "used_g": "12.5",
            "used_m": "4.2"
          }
        ]
      }
    ]
  }
}
```

`content` is a JSON **string**, not an object. Its nested `info.plate_idx` and each plate `index` are
integers. `prediction` is an integer, `weight` a JSON number, `filaments` an array, and `used_g` /
`used_m` are strings because pinned Studio parses them with `std::stof`. Thumbnail URL is empty unless
Pandar can return a real tenant-authorized resource. `color` and `type` must be non-empty; `weight`,
`used_g`, and `used_m` must be finite, non-negative, and representable by the target floating-point
consumer. Existing typed artifact metadata is the source; no arbitrary `serde_json::Value`
persistence is allowed. Semantically unusable persisted metadata returns Hub `409
studio_task_metadata_unavailable`; malformed successful Hub JSON is remapped to plugin `502`.
Unknown or unavailable data returns a non-success result and meaningful
`http_code`/redacted `http_body`, never HTTP `200` with `{}`.

No non-wrapper consumer of `get_slice_info` exists in the pinned source. Until a real pinned consumer
shape and real Pandar data source are both proven, it returns a stable non-success unavailable result
and clears `slice_json`. Success with an empty string is forbidden.

## SQLite And PostgreSQL Persistence

Print-task identity and metadata are durable behavior and must be identical on both first-class
backends. The minimal storage contract is:

- a non-null `studio_submission_id` with a unique `(tenant_id, studio_submission_id)` constraint;
- a persisted `plate_index`;
- a typed, versioned Studio print metadata payload containing the preserved non-secret fields;
- indexes for tenant/id lookup and tenant/printer/status/newest-first task listing;
- deterministic migration/backfill for existing jobs without changing an id after restart.

Do not use backend-specific JSON operators for required behavior. Deserialize the known metadata
shape into typed `serde` structs after backend-neutral row retrieval. Every schema/query change must
have paired SQLite and PostgreSQL migrations and repository tests. PostgreSQL completion evidence
requires a real disposable PostgreSQL run through `PANDAR_TEST_POSTGRES_URL`; an unset variable is an
explicit skip, not dual-backend proof.

Neither backend stores `filename`/`config_filename` local paths, `dev_ip`, `username`, `password`,
FTP credentials, access codes, auth tokens, or raw rejected JSON.

## Required Automated Evidence

The implementation is not complete until deterministic tests prove all of the following without a
real printer:

1. A compiled C++ ABI table sets every field to a non-default sentinel and observes its exact
   `preserve`, `default`, `reject`, or `unsupported` outcome at the loopback Hub boundary.
2. All five JSON fields reject malformed/schema-invalid JSON before artifact I/O or HTTP.
3. Secret and local-path sentinels are absent from requests, persisted metadata, callbacks, stdout,
   and stderr.
4. Cancellation returns `-18`; HTTP `201` emits only the queued boundary and cannot immediately emit
   `Finished` or call `OnWaitFn`.
5. Agent acknowledgement, printer-command publish, downstream failure, wait, and physical-running
   telemetry remain distinct observable states.
6. The compiled task consumer parses the exact list and double-encoded subtask shapes; plate/slice
   unavailable paths return non-success rather than fake empty success.
7. Stable id allocation, filters, pagination, restart stability, isolation, and migrations pass on
   SQLite and a real PostgreSQL instance.
8. The exact pinned model-task consumer observes the same caller pointer and all eight ordinary-task
   fields with one callback on ordinary success and during the serialized callback/account race. It
   observes an untouched object with no callback for metadata 409, missing task, malformed 2xx, stale
   account, and cancellation before the response/callback gate, including half-open-request destroy.

## Current Final16 Linux Baseline

Final16 freezes source archive SHA-256
`24b45dd30c3509c02b609548409f05fa72490512525621dbc0574a05aa62a039`. Its Linux release archive
has SHA-256 `023dcad198674c8ad1c20eb9bc34df9ef9685f49dfeca6e6b5ea58188f3a24a3`; packaged network
plugin and BambuSource companion SHA-256 values are
`3bcce9085205d6af67dc9671cf58cd6f9fb694d5a587b43d160dc8b6a9b0712f` and
`88d34358be39ed3d239aeb317df8f34a92d4652877e86a9849c66e32347c1df2`.

From that exact source, workspace Nextest passed 1,808/1,808 with one configured skip; fmt, strict
Clippy, and module-size gates passed; the ABI tools passed 22/22; release-smoke tools passed 25/25;
packaged tasks passed 18/18; all 130 exports were present; all 21 File Transfer entrypoints completed
256 ASan cycles; and PostgreSQL passed 7/7 with zero skipped.

The controlled official-AppImage gate used Ubuntu 22.04 Bambu Studio `02.08.01.55` AppImage SHA-256
`e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995`. Studio automatically
selected the only fixture printer without an explicit add-subscription call. The mock observed exactly
one model-task request and HTTP 200, while the plugin trace observed the four request-started,
response-accepted, callback-started, and callback-returned lifecycle events exactly once and in order.
Unexpected, legacy, and unsafe mutating request counts were zero. Evidence manifest, result-summary, and
runner SHA-256 values are `c6ba9b6282581119d3baec720e26990ad63efc20eb394b0c71dced89081d5fd9`,
`771d0a657e235eff40dffd1637175a4991bbbac7672b231133a20fddc11e3220`, and
`7ab2c4cb8816ae4488e40fce71ec69684997739567d3a76f17d2c9e2a324873f`.

Deterministic redacted bundle `pandar-final16-real-studio-evidence-019f7b10.tar.gz` is 245,225 bytes
with 26 tar entries (23 files and three directories) and SHA-256
`f07c369ad9e0354ef40142294d9385e9c454fd534a04badce4be000f49c06eca`; an independent second
generation produced the same hash. Its sidecar SHA-256 is
`30c6e5d43b74f9770d19638b86cefddd96d4d861c16155c74d30b488adf7f1b6`. It contains only
manifest-covered safe evidence plus outer success/provenance and excludes the runner and mock
implementation plus synthetic token contents.

This AppImage evidence is deliberately narrower than a production print path. It used a synthetic
persisted authenticated-shaped session and a loopback fail-closed mock. It did not use real
authentication, Hub, Agent, database, hardware, print submission/control/cancel, or firmware action.
Studio's logs are encrypted, so no downstream post-callback Studio behavior is claimed. No GitHub
Action was used.

## Historical Final14 And Final15 Evidence

Final14 froze source SHA-256 `c422d80d89052732db6b8ae87b68fd1e4145c64f588d8382deafef3345d86681`
and passed its Linux native/package/ASan and exact-AppImage module-load/development-no-auth gates. It
predates the caller-owned model-task and selected-target corrections and is historical only. Final15
passed a narrower native run, but its pre-correction selected-only Studio attempt is non-promotable;
neither candidate replaces final16.

## Historical Final13 Evidence

- Final13 source identity is `HEAD` `2ba0d1f2755501ea9e7d4babcf176db40638f643` plus archive
  `pandar-bambu-final13-019f7b10.tar.gz`, 2,751,227 bytes and 1,543 regular members. Archive,
  canonical-tree, member-list, and freeze-evidence SHA-256 values are
  `71080abb1e7392b0440a179b5bca9fd80638de74a614105b8dc11a0f70959c34`,
  `db0b7c3385c29ff0cdee1930a66f554a6845b58907373ef543563b829c245761`,
  `87a6ad1dfaa404731ed30d7e265303cca64fc4278a478f9c12192c09373eb880`, and
  `4d132e16f91365795f54c97f608483c34b55726c5f614f5bb8ffaac2ede1fb7f`. Archive determinism
  passed; unsafe member, duplicate, case-collision, reparse-point, membership-diff, and content-diff
  counts were all zero. Pre-freeze plugin run `da32fbc4-f37e-4198-af5e-c35f73512dcb` passed 368/368
  with one separately reported skip.
- Windows workspace Nextest run `90cb6a69-08a5-4421-a661-58e696c374a3` passed 1,778/1,778 executed
  tests with one separately reported skip in 1,050.084 seconds; the firmware probe passed in 28.858
  seconds. Fmt, zero-warning strict Clippy, module-size 2/2, ABI-tool 21/21, release-smoke-tool 21/21,
  frontend 37 files/324 tests, typecheck, zero-warning lint, and production build passed. `npm ci`
  reported six audit vulnerabilities (three moderate and three high), retained as dependency-audit
  evidence rather than a parity failure. Clean-gate evidence SHA-256 is
  `c1ac8807a427ae4b7003681e9ad343d668dab1d6aa7c143d14bc699fe58b7b89`.
- PostgreSQL 16.14 harness `0c292295-f9ab-459b-89c2-ea74f2c9ff56` ran Nextest
  `24b49c19-cd07-42b5-a5a3-6d220345bd7e` and `1f4b8458-6397-4c0b-8ab3-23d37779c68a`; each passed
  55/55 explicitly selected PostgreSQL cases, with 831 filtered and zero runtime skip markers.
  Per-run log SHA-256 values are
  `b123f495e09de3c57c2c175000a37cc1fa7395dd0a9c52f1c2f72426c2f4dc08` and
  `b3e233f50fe1be9df43867e34307fd6193f09a2dc00940318bdfb8827f0a8d54`; normalized evidence SHA-256 is
  `7e04ae355f7bca3fb409bbc700b5c8f160194c0d2f9ec82df823c859566a2db7`. Source read-only and
  cleanup checks passed.
- SQLite coverage is part of the complete workspace run; paired SQLite/PostgreSQL migrations and the
  backend-neutral repository behavior therefore use the same frozen implementation.
- The final13 Windows MSVC package/ABI/release-smoke gate passed separately; archive
  `pandar-final13-windows-amd64-019f7b10.tar.gz` is 21,285,752 bytes with SHA-256
  `6c50e77a0b4008ce46d86de51411117061c5118e18849ca1fb94f4a3f319db64`, and native evidence
  SHA-256 is `3dab4bffa359e4c46eec77cbfb278ce3a1497f806a1d80343a1735b5a68f025b`.
- Final13 Linux native/ASan attempt 2 passed as a whole. Nextest run
  `6ec3a215-9430-4ad2-adc7-f692ca156333` passed 1,779/1,779 with one separately reported skip; the
  exact three-file archive has SHA-256
  `4166e6012e6c1bf7cdf056ba3bfb28f0fbc9d216c31e5ed2e8620adb8b5fcccc`, and the evidence bundle
  has SHA-256 `aa7478fe0f74debcc5f3d1f5ec53a2222d726beafe5224935aa3382c24f6097a`. All five ABI modes and
  21 File Transfer entrypoints x 256 ASan/LSan cycles passed. Attempt 1 run
  `c8a134c4-e775-4f37-b6ed-74ccb1b79123` remains non-promotable harness history because its outer
  wrapper expected 21 exports from the FT-only invocation while the checker reported all 130 library
  contract exports.
- Final13 exact-AppImage attempt 8 passed load and same-process development no-auth recovery with the
  official `02.08.01.55` AppImage SHA-256
  `e633a116e900a2652915d4a8897f6e48122f0431bf10f642a62796505bb68995` and the passed Linux
  package. Redacted evidence SHA-256 is
  `a4453c8dce3829cc1a84a372a772b516812fe1564b310e61db9e9009a11cf9d2`. No print submission,
  authenticated Studio flow, printer/task UI, logout UI, hardware, or firmware operation ran; those
  surfaces remain untested. The final evidence-document review under Task 8 is complete.
