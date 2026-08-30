// Generated from contracts/hub-client.openapi.json. Do not edit.
import type { CommandStatus, JobStatus, PrintStatus } from "./hub-api-core";

export type ArtifactFilament = {
  filament_id: string | null;
  tray_info_idx?: string | null;
  nozzle_id?: number | null;
  filament_type: string | null;
  color: string | null;
  used_grams: number | null;
  used_meters: number | null;
};

export type ArtifactPlate = {
  plate_id: number;
  name: string;
  estimated_time_seconds: number | null;
  filament_weight_grams: number | null;
  object_count: number;
  objects: Array<string>;
  filaments: Array<ArtifactFilament>;
  has_thumbnail: boolean;
};

export type ArtifactMetadata = {
  source: string;
  display_name: string;
  default_plate_id: number | null;
  plate_count: number;
  plates: Array<ArtifactPlate>;
  warnings: Array<string>;
};

export type ArtifactMetadataPreviewResponse = {
  metadata: ArtifactMetadata | null;
};

export type JobPrint = {
  status: PrintStatus;
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

export type JobCommand = {
  id: string;
  kind: string;
  status: CommandStatus;
};

export type JobArtifact = {
  id: string;
  tenant_id: string;
  filename: string;
  content_type: string;
  size_bytes: number;
  metadata: ArtifactMetadata | null;
  created_at: string;
};

export type AmsMapping2 = {
  ams_id: number;
  slot_id: number;
};

export type AmsMappingInfo = {
  ams: number;
  targetColor: string;
  filamentId: string;
  filamentType: string;
  nozzleId: number | null;
  sourceColor: string | null;
};

export type FilamentUsage = {
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
};

export type JobMaterial = {
  ams_mapping: Array<number> | null;
  ams_mapping2: Array<AmsMapping2> | null;
  ams_mapping_info: Array<AmsMappingInfo> | null;
  filament_usage: Array<FilamentUsage>;
};

export type Job = {
  id: string;
  tenant_id: string;
  printer_id: string;
  agent_id: string;
  artifact_id: string;
  command_id: string;
  status: JobStatus;
  error: string | null;
  created_at: string;
  updated_at: string;
  print: JobPrint;
  command: JobCommand;
  artifact: JobArtifact;
  material: JobMaterial;
};

export type JobList = {
  jobs: Array<Job>;
};

export type RecoveryReasonRequest = {
  reason?: string | null;
};

export type ReprintJobRequest = {
  reason?: string | null;
  printer_id?: string | null;
  plate_id?: number | null;
  use_ams?: boolean | null;
  bed_leveling?: boolean | null;
  auto_bed_leveling?: 0 | 1 | 2 | null;
  flow_cali?: boolean | null;
  auto_flow_cali?: 0 | 1 | 2 | null;
  auto_offset_cali?: 0 | 1 | 2 | null;
  timelapse?: boolean | null;
  ams_mapping?: Array<number> | null;
  ams_mapping2?: Array<AmsMapping2> | null;
  ams_mapping_info?: Array<AmsMappingInfo> | null;
};
