# Studio Plugin Device List Design

## Problem

Bambu Studio starts with the Pandar networking plugin installed, and the plugin can reach the Pandar Hub. The Studio Device tab still shows `No printer`, and the printer selector has an empty `My Device` list, even though the same local Hub has one printer in the tenant.

The plugin shim returns the body from `GET /api/v1/plugin/printers` directly from `bambu_network_get_user_print_info`. The existing ABI probe success fixture uses the Bambu Studio shape:

```json
{ "devices": [{ "dev_id": "printer-1", "name": "Probe Printer" }] }
```

The Hub plugin route originally returned:

```json
{ "printers": [{ "dev_id": "...", "name": "..." }] }
```

After changing the top-level field to `devices`, runtime validation still showed an empty `My Device` list in Bambu Studio. The Bambu Studio parser in `reference/BambuStudio/src/slic3r/GUI/DeviceCore/DevManager.cpp` reads Studio-native device fields (`dev_name`, `dev_online`, `dev_model_name`, and `task_status`) rather than Pandar's plugin aliases (`name`, `online`, `model`, and `state`) when constructing `MachineObject` entries.

Runtime validation also showed that the local plugin sign-in page blocked trusted no-auth development runs before it could create a plugin ticket: external auth is disabled in no-auth mode, and the server action still required browser auth before calling the no-auth Hub.

## Scope

Change only the Studio/plugin-facing printer list response. The regular tenant printer API must keep returning its existing `printers` response.

## Design

`GET /api/v1/plugin/printers` will return a `devices` array containing Bambu Studio-native fields plus the existing Pandar plugin aliases:

- `dev_id`
- `dev_name`
- `name`
- `dev_model_name`
- `model`
- `dev_online`
- `online`
- `task_status`
- `state`
- `pandar_printer_id`

`dev_online` and `online` will both treat active printer states such as `IDLE` as online. Only persisted offline/unknown-style states are exposed as offline.

The plugin shim should continue passing the Hub response through unchanged. Auth behavior is unchanged: the route still requires a valid plugin Studio tenant token, and existing error mapping remains unchanged.

The frontend plugin sign-in page should allow the trusted local no-auth case when the frontend has no auth provider, no auth token, external auth readiness is disabled, and tenant lookup succeeds. Creating a plugin ticket should let Hub enforce authorization instead of adding an extra browser-auth precondition.

## Acceptance Criteria

- A plugin-authenticated `GET /api/v1/plugin/printers` returns `{"devices":[...]}`.
- The response no longer includes a top-level `printers` field.
- Device entries include the Bambu Studio fields used by `parse_user_print_info`: `dev_name`, `dev_online`, `dev_model_name`, and `task_status`.
- A printer whose Pandar status is `IDLE` is exposed to Studio as online.
- Existing plugin auth behavior is unchanged.
- Local no-auth plugin sign-in can create a plugin ticket and redirect back to Bambu Studio's callback URL.
- The Studio ABI probe remains compatible with the response shape it already expects.
- `docs/roadmap.md` records the completed fix.

## Verification

- Targeted Hub plugin route test for the response shape.
- Existing network plugin ABI probe or targeted plugin tests.
- `cargo fmt`
- `cargo clippy --workspace`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
