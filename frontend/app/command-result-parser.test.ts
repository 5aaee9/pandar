import { describe, expect, it } from "vitest";

import { parseCommandResult } from "./command-result-parser";

const baseCommand = {
  id: "cmd1",
  tenant_id: "tenant1",
  agent_id: "agent1",
  printer_id: null,
  status: "succeeded",
  payload_json: "{}",
  error: null,
  created_at: "2026-07-01T00:00:00Z",
  updated_at: "2026-07-01T00:00:00Z",
};

describe("parseCommandResult", () => {
  it("parses printer link command results", () => {
    const parsed = parseCommandResult({
      ...baseCommand,
      kind: "link_printer",
      result_json: JSON.stringify({
        type: "printer_link",
        serial_number: "SERIAL123",
        host: "192.0.2.10",
        name: "Office X1C",
        model: "X1 Carbon",
        status: "READY",
      }),
    });

    expect(parsed).toEqual({
      type: "printer_link",
      serial_number: "SERIAL123",
      host: "192.0.2.10",
      name: "Office X1C",
      model: "X1 Carbon",
      status: "READY",
    });
  });
});
