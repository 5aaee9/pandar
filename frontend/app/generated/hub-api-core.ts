// Generated from contracts/hub-client.openapi.json. Do not edit.

export type UserRole = "tenant_admin" | "operator" | "viewer";

export type CommandStatus =
  | "queued"
  | "sent"
  | "acknowledged"
  | "succeeded"
  | "failed"
  | "cancelled";

export type JobStatus =
  | "queued"
  | "sent"
  | "acknowledged"
  | "succeeded"
  | "failed"
  | "cancelled";

export type PrintStatus =
  | "pending"
  | "stalled"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

export type Tenant = {
  id: string;
  slug: string;
  display_name: string;
  created_at: string;
};

export type TenantList = {
  tenants: Array<Tenant>;
};

export type MeIdentity = {
  provider: string;
  subject: string;
  email: string | null;
  email_verified: boolean | null;
  display_name: string;
};

export type TenantMembership = {
  tenant_id: string;
  tenant_slug: string;
  display_name: string;
  role: UserRole;
};

export type MeResponse = {
  identity: MeIdentity;
  tenants: Array<TenantMembership>;
  can_self_create_tenant: boolean;
};

export type ExternalAuthStatus = {
  enabled: boolean;
  ready: boolean;
};

export type AuthStatusResponse = {
  external_auth: ExternalAuthStatus;
};

export type ErrorResponse = {
  error: string;
};

export type TicketResponse = {
  ticket: string;
  redirect_url: string;
  expires_at: string;
};

export type Agent = {
  id: string;
  tenant_id: string;
  name: string;
  status: string;
  created_at: string;
};

export type AgentList = {
  agents: Array<Agent>;
};

export type AgentPairingResponse = {
  agent: Agent;
  agent_env: string;
};

export type Command = {
  id: string;
  tenant_id: string;
  agent_id: string;
  printer_id: string | null;
  kind: string;
  status: CommandStatus;
  payload_json: string;
  error: string | null;
  result_json: string | null;
  created_at: string;
  updated_at: string;
};

export type CreatedTenantResponse = {
  tenant: Tenant;
  membership: TenantMembership;
};

export type AcceptedMembership = {
  user_id: string;
  role: UserRole;
  created: boolean;
};

export type AcceptedJoinLinkResponse = {
  tenant: Tenant;
  membership: AcceptedMembership;
  created: boolean;
};

export type MobileTicketExchangeRequest = {
  ticket: string;
  code_verifier: string;
};

export type MobileAuthProfile = {
  user_id: string;
  user_name: string;
  tenant_id: string;
  tenant_name: string;
};

export type MobileTicketExchangeResponse = {
  token: string;
  expires_at: string;
  profile: MobileAuthProfile;
};

export type PrinterAxis = "x" | "y" | "z";

export type PrinterAxisMovement = {
  axis: PrinterAxis;
  delta_mm: number;
};

export type PrinterControlRequest = {
  action: string;
  light_on?: boolean | null;
  axes?: Array<PrinterAxis> | null;
  movements?: Array<PrinterAxisMovement> | null;
  feedrate_mm_per_min?: number | null;
  speed_mode?: number | null;
  fan_index?: number | null;
  speed_percent?: number | null;
  airduct?: boolean | null;
  temperature_celsius?: number | null;
  wait?: boolean | null;
  ams_id?: number | null;
  slot_id?: number | null;
  global_tray_id?: number | null;
  external_id?: string | null;
  duration_hours?: number | null;
  filament?: string | null;
  rotate_tray?: boolean | null;
  holder_action?: number | null;
  nozzle_id?: number | null;
  extruder_id?: number | null;
  error_action?: PrintErrorAction | null;
  error_generation?: number | null;
  required_device_features?: Array<RequiredDeviceFeature> | null;
};

export type PrintErrorAction = "resume" | "ignore" | "stop";

export type RequiredDeviceFeature =
  | "bambu_mqtt_homing"
  | "bambu_mqtt_axis_control";
