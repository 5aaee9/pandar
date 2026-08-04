export const DISCOVERY_COMMAND_KIND = "discover_printers";

const TERMINAL_COMMAND_STATUSES = new Set([
  "succeeded",
  "failed",
  "cancelled",
]);

export function isTerminalCommandStatus(status: string) {
  return TERMINAL_COMMAND_STATUSES.has(status);
}
