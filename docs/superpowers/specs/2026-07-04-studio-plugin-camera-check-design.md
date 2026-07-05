# Studio Plugin Camera Check Design

## Problem

The Bambu Studio networking plugin can be installed, loaded, logged in, and can list/select the Pandar printer. Studio's Lamp command no longer disconnects after mapping Studio `system.ledctrl` JSON to Pandar's light operations, but clicking Studio's camera play button still shows `Failed to connect to the printer`.

Directly requesting Hub's camera route with the selected printer ID returns `200 OK` and then closes without a media payload. The local machine has FFmpeg installed through WinGet, but `ffmpeg.exe` is not on the process `PATH`, so the Agent camera bridge cannot spawn FFmpeg in the current development environment.

The Studio printer selector also displayed the serial number because the local development database currently stores that serial as the printer name. This is local data, not the plugin response shape.

## Scope

- Keep the existing Hub camera route and reverse camera gRPC design.
- Make Agent camera streaming able to locate FFmpeg through an explicit environment variable as well as `PATH`.
- Preserve full diagnostic context when FFmpeg cannot be spawned.
- Do not add a new camera transport or change browser camera UI.
- Repair the local development printer name through the existing API for the runtime check, not through a schema or route change.

## Design

Agent camera streaming will choose the FFmpeg executable from `PANDAR_FFMPEG_PATH` when it is set and non-empty after trimming whitespace; otherwise it will keep using `ffmpeg` from `PATH`. The command builder remains otherwise unchanged.

The Agent will continue to return a full anyhow context chain when FFmpeg spawn fails, including the attempted executable path in the spawn context, so missing or invalid FFmpeg paths are visible in logs and Hub stream failures.

For the local Bambu Studio check, restart the Agent with `PANDAR_FFMPEG_PATH` pointing at the installed WinGet FFmpeg executable, then repeat the Studio camera play action.

## Acceptance Criteria

- The network plugin installs to Bambu Studio's plugin directory and is not overwritten on Studio startup.
- Studio can login through the Pandar plugin local callback.
- Studio's printer selector shows the local printer with the corrected display name.
- Selecting the printer enables Studio printer controls.
- Clicking Studio's Lamp control does not disconnect or produce invalid-printer-control behavior.
- Clicking Studio's camera play control opens Hub's camera route and the stream produces media bytes instead of immediately closing empty because FFmpeg is missing from `PATH`.
- Targeted network plugin tests pass for Studio status/camera URL/light-command parsing.
- Agent camera command tests cover unset, whitespace-only, and explicit `PANDAR_FFMPEG_PATH` selection with a pure helper instead of mutating process environment in parallel tests.
- Agent camera command tests cover FFmpeg spawn failure diagnostics preserving the attempted executable path and lower spawn error context.
- `docs/roadmap.md` records the completed Studio plugin runtime check/fix.

## Verification

- `cargo test -p pandar-network-plugin`
- Agent camera unit test for FFmpeg executable selection.
- Manual Studio runtime check with the installed plugin.
- `cargo fmt`
- `cargo clippy --workspace`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
