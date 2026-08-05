import type { Agent, JoinLink, TenantToken } from "./dashboard-types";

export type MutationActionState =
  | { ok: true }
  | { ok: false; error: string }
  | null;

export type LinkPrinterActionState =
  | { ok: true; commandId: string }
  | { ok: false; error: string }
  | null;

export type SecretActionState =
  | {
      ok: true;
      kind: "tenant_token";
      operation: "created" | "rotated";
      token: string;
      tenantToken: TenantToken;
    }
  | {
      ok: true;
      kind: "agent_pairing";
      agentEnv: string;
      agent: Agent;
      message: string;
    }
  | {
      ok: true;
      kind: "join_link";
      joinLink: JoinLink;
      token: string;
      message: string;
    }
  | {
      ok: false;
      error: string;
    }
  | null;
