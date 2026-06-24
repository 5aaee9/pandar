# Windows DLL Smoke Test - 2026-06-25

This note records a local Windows smoke test for the Pandar Bambu Studio network
plugin DLL. Local usernames and repository paths are redacted.

## Environment

| Item | Value |
| --- | --- |
| OS | Windows x86_64 |
| Bambu Studio | `02.07.01.62` |
| Bambu Studio install | `C:\Program Files\Bambu Studio\bambu-studio.exe` |
| Plugin directory | `%APPDATA%\BambuStudio\plugins` |
| Pandar commit | `00380cf` |
| Tested DLL | `<repo>\target\debug\pandar_network_plugin.dll` |
| Tested DLL SHA256 | `B7756734C18DC496B801413CFDFABC6C061BBC3D800D1B3DFCFBACCB04CB446E` |
| Official Bambu DLL SHA256 | `87AE4BD4AD142AEBEF7ADFC1087D1F7E145A07F0E113577011B915797FDC0098` |

## Test Coverage

The smoke covered DLL build output, exported symbols, direct loading, and real
Bambu Studio startup with the Pandar DLL installed through the network plugin
path.

It did not perform real user authentication, printer listing against a live hub,
or a real print submission from Bambu Studio.

## Automated Checks

Focused plugin tests passed:

```powershell
cmd /s /c '"%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cargo test -p pandar-network-plugin -- --nocapture'
```

Results:

- `exports_phase_21_abi_symbols` passed.
- 17 HTTP boundary and error-mapping tests passed.
- 2 Studio ABI probe tests passed at the Rust test level. The fixture compile
  path reported no `CXX`, `c++`, `g++`, or `clang++` compiler on this host, so
  the probe's external C++ fixture path was skipped by its existing guard.

Export inspection passed:

```powershell
cmd /s /c '"%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && dumpbin /exports target\debug\pandar_network_plugin.dll | findstr /R "bambu_network_ ft_"'
```

`dumpbin` listed the required `bambu_network_*` and `ft_*` exports, including
`ft_abi_version`.

Direct load and function call passed:

- Loaded `<repo>\target\debug\pandar_network_plugin.dll` with `LoadLibrary`.
- Resolved `ft_abi_version` with `GetProcAddress`.
- Called `ft_abi_version()`.
- Result: `1`.

## Bambu Studio Load Test

The DLL was installed using the `reference/open-bamboo-networking` manual
installation pattern:

- Copy the Pandar DLL to `%APPDATA%\BambuStudio\plugins\bambu_networking.dll`.
- Copy the same DLL to `%APPDATA%\BambuStudio\plugins\backup\bambu_networking.dll`.
- Create `%APPDATA%\BambuStudio\ota\plugins\network_plugins.json`.
- Patch `%APPDATA%\BambuStudio\BambuStudio.conf` under `app`:
  - `installed_networking = "1"`
  - `update_network_plugin = "false"`
  - `ignore_module_cert = "1"`

Bambu Studio then launched successfully. The visible UI reached the normal main
window with the expected top-level areas such as `Prepare`, `Preview`, `Device`,
`Project`, `Calibration`, and `Filament Manager`.

Process module inspection confirmed that Bambu Studio loaded:

```text
%APPDATA%\BambuStudio\plugins\bambu_networking.dll
```

At the time of inspection, that file still had the Pandar DLL hash
`B7756734C18DC496B801413CFDFABC6C061BBC3D800D1B3DFCFBACCB04CB446E`, which means
Bambu Studio did not replace it with the official plugin during startup.

## Cleanup Verification

After the test:

- Bambu Studio was stopped.
- `%APPDATA%\BambuStudio\BambuStudio.conf` was restored from the pre-test backup.
- `%APPDATA%\BambuStudio\plugins\bambu_networking.dll` was restored to the
  official Bambu DLL.
- `%APPDATA%\BambuStudio\plugins\backup\bambu_networking.dll` was restored to
  the official Bambu DLL.
- `%APPDATA%\BambuStudio\ota\plugins\network_plugins.json` was removed.
- No `bambu-studio` process remained.
- The repository worktree was clean.

Restored official DLL hash:

```text
87AE4BD4AD142AEBEF7ADFC1087D1F7E145A07F0E113577011B915797FDC0098
```

## Conclusion

The Windows debug DLL was functionally healthy for this smoke scope:

- it exported the expected Bambu Studio network plugin ABI symbols;
- it could be loaded and called directly by Windows;
- Bambu Studio could load it from the plugin path without startup replacement or
  visible plugin-load failure;
- the main Bambu Studio UI opened normally.

Remaining compatibility work is live end-to-end behavior: sign-in through Studio,
hub-backed printer/job listing, and print submission against a running Pandar hub.
