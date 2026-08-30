import { describe, expect, it } from "vitest";

import {
  decodeHubPayload,
  decodeHubResponse,
  HubContractError,
} from "./hub-contract";

describe("decodeHubResponse", () => {
  it("returns a typed response and permits additive fields", () => {
    const decoded = decodeHubResponse("AgentList", {
      agents: [
        {
          id: "agent-1",
          tenant_id: "tenant-1",
          name: "Shop",
          status: "online",
          created_at: "created",
          future_field: true,
        },
      ],
      future_envelope: "kept",
    });

    expect(decoded.agents[0].id).toBe("agent-1");
  });

  it("rejects malformed known fields with their wire path", () => {
    expect(() =>
      decodeHubResponse("Command", {
        id: "command-1",
        tenant_id: "tenant-1",
        agent_id: "agent-1",
        printer_id: "printer-1",
        kind: "printer_operation",
        status: "completed",
        payload_json: "{}",
        error: null,
        result_json: null,
        created_at: "created",
        updated_at: "updated",
      }),
    ).toThrowError(
      new HubContractError(
        "$.status",
        "queued | sent | acknowledged | succeeded | failed | cancelled",
      ),
    );
  });

  it("models every field in the Hub job projection", () => {
    const decoded = decodeHubResponse("JobList", {
      jobs: [
        {
          id: "job-1",
          tenant_id: "tenant-1",
          printer_id: "printer-1",
          agent_id: "agent-1",
          artifact_id: "artifact-1",
          command_id: "command-1",
          status: "acknowledged",
          error: null,
          created_at: "created",
          updated_at: "updated",
          print: {
            status: "running",
            printer_state: null,
            progress_percent: null,
            remaining_time_minutes: null,
            current_layer: null,
            total_layers: null,
            active_file: null,
            last_progress_percent: null,
            last_layer: null,
            error: null,
            started_at: null,
            finished_at: null,
            updated_at: null,
          },
          command: {
            id: "command-1",
            kind: "print_project_file",
            status: "acknowledged",
          },
          artifact: {
            id: "artifact-1",
            tenant_id: "tenant-1",
            filename: "part.3mf",
            content_type: "model/3mf",
            size_bytes: 1,
            metadata: {
              source: "3mf",
              display_name: "Part",
              default_plate_id: 1,
              plate_count: 1,
              plates: [],
              warnings: [],
            },
            created_at: "created",
          },
          material: {
            ams_mapping: null,
            ams_mapping2: null,
            ams_mapping_info: [
              {
                ams: 0,
                targetColor: "#000000FF",
                filamentId: "GFA00",
                filamentType: "PLA",
                nozzleId: null,
                sourceColor: null,
              },
            ],
            filament_usage: [],
          },
        },
      ],
    });

    expect(decoded.jobs[0].tenant_id).toBe("tenant-1");
    expect(decoded.jobs[0].artifact.metadata?.plate_count).toBe(1);
    expect(decoded.jobs[0].material.ams_mapping_info?.[0].filamentType).toBe(
      "PLA",
    );
  });

  it("matches the closed printer-control request boundary", () => {
    expect(
      decodeHubPayload("PrinterControlRequest", {
        action: "handle_print_error",
        error_action: "resume",
        error_generation: 9,
      }),
    ).toEqual({
      action: "handle_print_error",
      error_action: "resume",
      error_generation: 9,
    });
    expect(
      decodeHubPayload("PrinterControlRequest", {
        action: "home",
        axes: [],
        required_device_features: ["bambu_mqtt_homing"],
      }),
    ).toEqual({
      action: "home",
      axes: [],
      required_device_features: ["bambu_mqtt_homing"],
    });
    expect(() =>
      decodeHubPayload("PrinterControlRequest", {
        action: "pause",
        unexpected: true,
      }),
    ).toThrow("Invalid Hub payload at $.unexpected: expected declared field");
  });

  it("rejects missing required fields instead of casting response json", () => {
    expect(() => decodeHubResponse("PrinterList", {})).toThrow(
      "Invalid Hub payload at $.printers: expected present field",
    );
  });

  it("requires the canonical printer compatibility projection", () => {
    expect(() =>
      decodeHubPayload("Printer", {
        id: "printer-1",
        tenant_id: "tenant-1",
        agent_id: "agent-1",
        serial_number: "SERIAL1",
        name: "Printer",
        model: null,
        status: "online",
        last_seen_at: "now",
        created_at: "then",
        materials: null,
      }),
    ).toThrow("Invalid Hub payload at $.compatibility: expected present field");
  });
});
