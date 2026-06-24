# Bambu Studio Plugin Smoke Runbook

## Scope

This runbook records real Bambu Studio compatibility evidence for Phase 23. A successful local ABI probe is not a real Studio compatibility claim.

## Prerequisites

- A running `pandar-hub` reachable from the desktop host.
- A running `pandar-web` with external auth configured.
- A tenant with at least one user who can create plugin login tickets.
- A verified plugin sign-in path: open the Pandar web UI, authenticate, select the tenant, and confirm `frontend/app/plugin-sign-in` can create a short-lived plugin login ticket before replacing the Studio plugin.
- A linked `pandar-agent`.
- Optional: a real printer connected through the agent for print submission.

Current workspace check on 2026-06-24: no local Bambu Studio command was found, so this runbook is blocked here until a Studio installation is provided. Release artifact availability is tracked separately in `docs/compatibility/release-artifacts.md`.

## Build Or Select Plugin Artifact

Prefer an existing plugin artifact built for the same OS and architecture as the Bambu Studio installation. Record the artifact filename and Pandar commit in the manifest; do not record local filesystem paths.

Expected artifact names:

| OS | Artifact |
| --- | --- |
| Linux | `libpandar_network_plugin.so` |
| Windows | `pandar_network_plugin.dll` |
| macOS | `libpandar_network_plugin.dylib` |

Repo-local build option for Linux:

```bash
cargo build -p pandar-network-plugin --release
```

Repo-local build option for Windows from a Windows Rust environment:

```powershell
cargo build -p pandar-network-plugin --release
```

Repo-local build option for macOS from a macOS Rust environment:

```bash
cargo build -p pandar-network-plugin --release
```

Cross-platform release packaging and signing are Phase 24 work. For Phase 23 manual testing, use a same-platform artifact you can trace to a Pandar commit.

## Environment

```bash
export PANDAR_PLUGIN_HUB_URL="https://your-hub.example"
export PANDAR_PLUGIN_FRONTEND_URL="https://your-web.example"
```

## Preflight

Run the local preflight helper before replacing any Bambu Studio plugin file:

```bash
cargo run --manifest-path tools/studio-plugin-smoke/Cargo.toml -- \
  --preflight \
  --studio-path /path/to/BambuStudio \
  --plugin-artifact /path/to/libpandar_network_plugin.so \
  --hub-url "$PANDAR_PLUGIN_HUB_URL" \
  --frontend-url "$PANDAR_PLUGIN_FRONTEND_URL" \
  --os linux \
  --arch x86_64 \
  --studio-version "1.10.2" \
  --test-date 2026-06-24 \
  --pandar-commit "$(git rev-parse HEAD)"
```

A passing preflight only proves the prerequisite paths, plugin filename, URL shape, and evidence metadata are ready for a manual run. It does not launch Bambu Studio, exercise the plugin, or provide real Studio compatibility evidence; every Studio checklist item remains `untested` until Studio is launched and exercised manually.

## Replace And Roll Back

1. Locate the original Bambu Studio network plugin dynamic library.
2. Copy it to a timestamped backup path.
3. Replace it with the Pandar plugin artifact for the same OS and architecture.
4. To roll back, quit Studio and restore the backup file.

## Smoke Checklist

| Step | Expected Result | Status | Evidence |
| --- | --- | --- | --- |
| Studio starts and loads plugin | No missing-symbol or dynamic-loader error. | `untested` | |
| Login opens Pandar sign-in | Studio WebView displays Pandar sign-in. | `untested` | |
| Localhost ticket callback completes | Studio receives plugin ticket through its local callback. | `untested` | |
| Token exchange completes | Studio exchanges the plugin ticket for a tenant-scoped plugin credential. | `untested` | |
| Profile loads | Studio receives Bambu-shaped login state. | `untested` | |
| Printer list loads | Hub-backed printers display or an empty list is accepted. | `untested` | |
| Job list loads | Hub-backed jobs display or an empty list is accepted. | `untested` | |
| Print submission | Optional print submits through `/api/v1/plugin/prints`. | `untested` | |
| Logout | Studio receives `studio_useroffline`. | `untested` | |
| Direct-printer and `ft_*` paths | Unsupported behavior is stable and does not open machine sockets. | `untested` | |

## Evidence Capture And Redaction

- Capture Studio version, OS, architecture, artifact name, Pandar commit, and test date.
- Redact bearer tokens, plugin tickets, Bambu access codes, local artifact paths, and filesystem paths.
- Prefer short log excerpts over full logs.

## Updating The Manifest

After the run, update `docs/compatibility/bambu-studio-plugin.md` with one row per Studio version, OS, and architecture. Keep failed or blocked attempts because they are compatibility evidence.
