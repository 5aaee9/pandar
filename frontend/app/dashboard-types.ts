import type {
  Command,
  Job,
  NozzleSystem,
  Printer,
  UserIdentity,
} from "./generated/hub-api";

export type * from "./generated/hub-api";

export type PrinterNozzleSystem = NozzleSystem;

export type Summary = {
  tenants: number;
  agents: number;
  printers: number;
  commands: number;
};

export type UserIdentityList = {
  identities: UserIdentity[];
};

export type AuthMetadata = {
  source: "request_cookie" | "app_auth_bearer_token" | "app_api_token" | "none";
  cookieName: string;
  provider: "clerk" | "logto" | "betterauth" | "none";
  signInUrl: string | null;
  signOutUrl: string | null;
};

export type DiscoveryResultData = {
  type: "printer_discovery";
  printers: Array<{
    serial_number?: string;
    host: string;
    name?: string;
    model?: string;
    source?: string;
  }>;
};

export type DiscoveredPrinter = DiscoveryResultData["printers"][number];

export type DiagnosticResultData = {
  type: "printer_diagnostic";
  serial_number: string;
  host?: string;
  model?: string;
  overall: string;
  checks: Array<{
    id: string;
    status: string;
    message: string;
    details?: string;
  }>;
  compatibility?: {
    normalized_model?: string | null;
    external_storage?: string;
    ftps_tls_1_2_cap?: boolean;
    features?: Record<string, string>;
  };
};

export type PrinterLinkResultData = {
  type: "printer_link";
  serial_number: string;
  host: string;
  name?: string;
  model?: string;
  status: string;
};

export type CommandResultData =
  | DiscoveryResultData
  | DiagnosticResultData
  | PrinterLinkResultData;

export type PrinterEvent =
  | { type: "printer_snapshot"; printer: Printer }
  | { type: "job_progress"; job: Job }
  | { type: "command_result"; command: Command };

export type FetchResult<T> =
  | { data: T; error: null; status?: number }
  | { data: null; error: null; status?: number }
  | { data: null; error: string; status?: number };
