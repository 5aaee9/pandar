import type {
  AgentList,
  AuditEventList,
  Command,
  JobList,
  JoinLinkList,
  PrinterList,
  TenantTokenList,
  UserList,
} from "./dashboard-types";
import { parseCommandResult } from "./command-result-parser";

const apiUrl = process.env.NEXT_PUBLIC_APP_API_URL ?? "http://localhost:8080";

async function fetchApi<T>(path: string): Promise<T> {
  const response = await fetch(`${apiUrl}${path}`, {
    credentials: "include",
  });
  if (!response.ok) {
    throw new Error(`API error: ${response.status}`);
  }
  return response.json() as Promise<T>;
}

export const apiClient = {
  tenants: {
    list: () => fetchApi<{ tenants: Array<{ id: string; slug: string; display_name: string; created_at: string }> }>("/api/v1/tenants"),
  },
  printers: {
    list: (tenantId: string) => fetchApi<PrinterList>(`/api/v1/tenants/${tenantId}/printers`),
  },
  agents: {
    list: (tenantId: string) => fetchApi<AgentList>(`/api/v1/tenants/${tenantId}/agents`),
  },
  jobs: {
    list: (tenantId: string) => fetchApi<JobList>(`/api/v1/tenants/${tenantId}/jobs`),
    delete: (tenantId: string, jobId: string) =>
      fetchApi<void>(`/api/v1/tenants/${tenantId}/jobs/${jobId}`),
    clear: (tenantId: string) =>
      fetchApi<void>(`/api/v1/tenants/${tenantId}/jobs`),
  },
  users: {
    list: (tenantId: string) => fetchApi<UserList>(`/api/v1/tenants/${tenantId}/users`),
    joinLinks: (tenantId: string) => fetchApi<JoinLinkList>(`/api/v1/tenants/${tenantId}/join-links`),
  },
  settings: {
    tenantTokens: (tenantId: string) => fetchApi<TenantTokenList>(`/api/v1/tenants/${tenantId}/tenant-tokens`),
    auditEvents: (tenantId: string) => fetchApi<AuditEventList>(`/api/v1/tenants/${tenantId}/audit-events?limit=20`),
  },
  commands: {
    get: (tenantId: string, commandId: string) => fetchApi<Command>(`/api/v1/tenants/${tenantId}/commands/${commandId}`),
  },
};

export type { Command };
export { parseCommandResult };
