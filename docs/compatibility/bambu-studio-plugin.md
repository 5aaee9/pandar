# Bambu Studio Plugin Compatibility

Phase 23 tracks Pandar's Bambu Studio network plugin compatibility evidence. A platform is compatible only after a real Bambu Studio run is recorded here.

## Status Values

| Status | Meaning |
| --- | --- |
| `passed` | Verified in the named environment with evidence captured. |
| `failed` | Attempted and failed; reproduction notes are recorded. |
| `blocked` | Could not complete because of a documented environment or dependency blocker. |
| `unsupported` | Intentionally unsupported by Pandar. |
| `untested` | No evidence has been recorded. |

## Real Studio Evidence

| Studio Version | OS | Arch | Plugin Artifact | Pandar Commit | Test Date | Load | Sign-In Page | Localhost Ticket | Token Exchange | Profile | Printers | Jobs | Print Submission | Logout | Unsupported ABI | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| untested | Linux | x86_64 | `libpandar_network_plugin.so` | none | none | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | No real Studio run recorded. |
| untested | Windows | x86_64 | `pandar_network_plugin.dll` | none | none | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | No real Studio run recorded. |
| untested | macOS | arm64/x86_64 | `libpandar_network_plugin.dylib` | none | none | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | `untested` | No real Studio run recorded. |

## Local Automated Probe Coverage

| Probe | Coverage | Status | Evidence |
| --- | --- | --- | --- |
| `cargo test -p pandar-network-plugin` | Exported symbol list, Rust HTTP helper boundaries, and local C++ ABI call sequence without Bambu Studio. | `untested` | Update after the implementation lands and tests pass. |

## Unsupported ABI Surfaces

| Surface | Status | Reason |
| --- | --- | --- |
| Direct LAN printer connect/message APIs | `unsupported` | Pandar keeps printer sockets in `pandar-agent`; the plugin talks only to `pandar-hub`. |
| `ft_*` direct file-transfer tunnel/job APIs | `unsupported` | Pandar uploads through hub-backed print submission and does not open direct file-transfer sockets in the plugin. |

## Evidence Requirements

- Record the exact Studio version, OS, architecture, plugin artifact name, Pandar commit, and test date.
- Redact bearer tokens, plugin tickets, Bambu access codes, local artifact paths, and filesystem paths.
- Attach or summarize logs/screenshots only after redaction.
- Keep failed and blocked rows; they are compatibility evidence.
