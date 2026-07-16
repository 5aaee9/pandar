import type { PrinterPrintState } from "./printer-live-types";

export type Summary = {
  tenants: number;
  agents: number;
  printers: number;
  commands: number;
};

export type Tenant = {
  id: string;
  slug: string;
  display_name: string;
  created_at: string;
};

export type Printer = {
  id: string;
  tenant_id: string;
  agent_id: string;
  serial_number: string;
  name: string;
  model: string | null;
  status: string;
  last_seen_at: string;
  created_at: string;
  nozzle_temperatures?: Array<{
    label?: string | null;
    current_celsius?: string | null;
    target_celsius?: string | null;
    diameter_mm?: string | null;
    nozzle_type?: string | null;
  }>;
  active_nozzle?: string | null;
  bed_temperature_celsius?: string | null;
  bed_target_temperature_celsius?: string | null;
  chamber_temperature_celsius?: string | null;
  chamber_light_on?: boolean | null;
  materials: PrinterMaterials | null;
  state_revision?: number;
  print?: PrinterPrintState | null;
};

export type PrinterMaterials = {
  filament_switch_installed?: boolean | null;
  ams_units: Array<{
    unit_id?: string;
    unit_kind?: string | null;
    humidity?: number | string | null;
    humidity_level?: number | string | null;
    temperature_celsius?: number | string | null;
    toolhead?: string | null;
    trays?: Array<{
      tray_id?: string;
      type?: string | null;
      color?: string | null;
      multi_color?: string[] | null;
      filament_id?: string | null;
      setting_id?: string | null;
      name?: string | null;
      remaining_estimate?: string | number | null;
      k_value?: string | number | null;
      toolhead?: string | null;
      global_tray_id?: number | null;
      exists?: boolean | null;
    }>;
  }>;
  external_spools: Array<{
    external_id?: string;
    tray_id?: string;
    type?: string | null;
    color?: string | null;
    multi_color?: string[] | null;
    filament_id?: string | null;
    setting_id?: string | null;
    name?: string | null;
    remaining_estimate?: string | number | null;
    k_value?: string | number | null;
    toolhead?: string | null;
    global_tray_id?: number | null;
    exists?: boolean | null;
  }>;
  active_tray: {
    kind?: string;
    ams_id?: string | null;
    tray_id?: string | null;
    global_tray_id?: number | null;
    external_id?: string | null;
  } | null;
  observed_at: string;
};

export type Agent = {
  id: string;
  tenant_id: string;
  name: string;
  status: string;
  created_at: string;
};

export type User = {
  id: string;
  tenant_id: string;
  email: string;
  display_name: string;
  role: "tenant_admin" | "operator" | "viewer";
  created_at: string;
};

export type UserIdentity = {
  id: string;
  tenant_id: string;
  user_id: string;
  provider: string;
  subject: string;
  created_at: string;
};

export type TenantToken = {
  id: string;
  tenant_id: string;
  name: string;
  scopes: string[];
  created_by_user_id: string | null;
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
  revoked_at: string | null;
};

export type JoinLink = {
  id: string;
  tenant_id: string;
  role: "tenant_admin" | "operator" | "viewer";
  email_constraint: string | null;
  expires_at: string;
  max_uses: number;
  used_count: number;
  created_by_user_id: string | null;
  revoked_at: string | null;
  created_at: string;
};

export type JoinLinkList = {
  join_links: JoinLink[];
};

export type MeResponse = {
  identity: {
    provider: string;
    subject: string;
    email: string | null;
    email_verified: boolean | null;
    display_name: string;
  };
  tenants: Array<{
    tenant_id: string;
    tenant_slug: string;
    display_name: string;
    role: "tenant_admin" | "operator" | "viewer";
  }>;
  can_self_create_tenant: boolean;
};

export type AuditEvent = {
  id: string;
  tenant_id: string;
  actor_type: string;
  user_id: string | null;
  action: string;
  target_type: string;
  target_id: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
};

export type TenantList = {
  tenants: Tenant[];
};

export type PrinterList = {
  printers: Printer[];
};

export type AgentList = {
  agents: Agent[];
};

export type UserList = {
  users: User[];
};

export type UserIdentityList = {
  identities: UserIdentity[];
};

export type TenantTokenList = {
  tenant_tokens: TenantToken[];
};

export type AuditEventList = {
  audit_events: AuditEvent[];
};

export type AuthMetadata = {
  source: "request_cookie" | "app_auth_bearer_token" | "app_api_token" | "none";
  cookieName: string;
  provider: "clerk" | "logto" | "betterauth" | "none";
  signInUrl: string | null;
  signOutUrl: string | null;
};

export type Command = {
  id: string;
  tenant_id: string;
  agent_id: string;
  printer_id: string | null;
  kind: string;
  status: string;
  payload_json: string;
  error: string | null;
  result_json: string | null;
  created_at: string;
  updated_at: string;
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
    ftps_clear_data_fallback?: boolean;
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

export type ArtifactMetadata = {
  display_name: string;
  default_plate_id: number | null;
  plates: Array<{
    plate_id: number;
    name: string;
    estimated_time_seconds: number | null;
    filament_weight_grams: number | null;
    object_count: number;
    objects: string[];
    filaments: Array<{
      filament_id: string | null;
      tray_info_idx?: string | null;
      nozzle_id?: number | null;
      filament_type: string | null;
      color: string | null;
      used_grams: number | null;
      used_meters: number | null;
    }>;
    has_thumbnail: boolean;
  }>;
  warnings: string[];
};

export type Job = {
  id: string;
  printer_id: string;
  agent_id: string;
  artifact_id: string;
  command_id: string;
  status: string;
  error: string | null;
  created_at: string;
  updated_at: string;
  print: {
    status: string;
    printer_state: string | null;
    progress_percent: number | null;
    remaining_time_minutes: number | null;
    current_layer: number | null;
    total_layers: number | null;
    active_file: string | null;
    last_progress_percent: number | null;
    last_layer: number | null;
    error: string | null;
    started_at: string | null;
    finished_at: string | null;
    updated_at: string | null;
  };
  command: {
    id: string;
    kind: string;
    status: string;
  };
  artifact: {
    id: string;
    tenant_id: string;
    filename: string;
    content_type: string;
    size_bytes: number;
    metadata: ArtifactMetadata | null;
    created_at: string;
  };
  material: {
    ams_mapping: number[] | null;
    ams_mapping2: Array<{
      ams_id: number;
      slot_id: number;
    }> | null;
    filament_usage: Array<{
      slot_index: number;
      source: string;
      ams_id: string | null;
      tray_id: string | null;
      global_tray_id: number | null;
      external_id: string | null;
      filament_id: string | null;
      setting_id: string | null;
      filament_type: string | null;
      color: string | null;
      used_mm: string | null;
      used_grams: string | null;
      confidence: string;
    }>;
  };
};

export type JobList = {
  jobs: Job[];
};

export type PrinterEvent =
  | {
      type: "printer_snapshot";
      printer: Printer;
    }
  | {
      type: "job_progress";
      job: Job;
    }
  | {
      type: "command_result";
      command: Command;
    };

export type PrinterEventTicket = {
  ticket: string;
  expires_at: string;
};

export type FetchResult<T> =
  | { data: T; error: null; status?: number }
  | { data: null; error: null; status?: number }
  | { data: null; error: string; status?: number };
