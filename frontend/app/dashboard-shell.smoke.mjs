import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

const shellModuleUrl = pathToFileURL(
  new URL("./dashboard-shell.ts", import.meta.url).pathname,
);

const {
  DASHBOARD_VIEWS,
  dashboardRootRedirectTarget,
  dashboardSidebarHref,
  dashboardTenantHref,
  dashboardViewTitleKey,
  logoutHref,
} = await import(shellModuleUrl.href);

assert.deepEqual(DASHBOARD_VIEWS, ["devices", "agents", "users", "settings"]);
assert.equal(dashboardViewTitleKey("devices"), "devices");
assert.equal(dashboardViewTitleKey("agents"), "agents");
assert.equal(dashboardViewTitleKey("users"), "users");
assert.equal(dashboardViewTitleKey("settings"), "settings");

assert.equal(dashboardRootRedirectTarget({}), "/devices");
assert.equal(
  dashboardRootRedirectTarget({ tenant: "tenant 1", status: "job_created" }),
  "/devices?tenant=tenant+1&status=job_created",
);
assert.equal(
  dashboardRootRedirectTarget({
    tenant: "t1",
    command: "cmd1",
    status: "refresh_queued",
  }),
  "/agents?tenant=t1&command=cmd1&status=refresh_queued",
);

assert.equal(
  dashboardSidebarHref("agents", {
    tenant: "t1",
    command: "cmd1",
    status: "done",
  }),
  "/agents?tenant=t1",
);
assert.equal(dashboardSidebarHref("users", {}), "/users");

assert.equal(
  dashboardTenantHref("agents", "t2", {
    tenant: "t1",
    command: "cmd1",
    status: "done",
  }),
  "/agents?tenant=t2&command=cmd1&status=done",
);
assert.equal(
  dashboardTenantHref("devices", "t2", {
    tenant: "t1",
    command: "cmd1",
    status: "done",
  }),
  "/devices?tenant=t2&status=done",
);

assert.equal(logoutHref({ signOutUrl: null }), null);
assert.equal(
  logoutHref({ signOutUrl: "https://auth.example/sign-out" }),
  "https://auth.example/sign-out",
);
