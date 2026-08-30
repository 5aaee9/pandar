import type { CommandStatus } from "./generated/hub-api";

export const DISCOVERY_COMMAND_KIND = "discover_printers";

const TERMINAL_COMMAND_STATUSES = new Set<CommandStatus>([
  "succeeded",
  "failed",
  "cancelled",
]);

export function isTerminalCommandStatus(status: CommandStatus) {
  return TERMINAL_COMMAND_STATUSES.has(status);
}
