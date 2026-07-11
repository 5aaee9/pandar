export type PrinterPrintState = {
  task_generation: number;
  error_generation: number;
  hms: Array<{ attr: number; code: number }>;
  job_state: number | null;
  gcode_state: string | null;
  task_id: string | null;
  subtask_id: string | null;
  subtask_name: string | null;
  gcode_file: string | null;
  progress_percent: number | null;
  remaining_time_minutes: number | null;
  current_layer: number | null;
  total_layers: number | null;
  print_error: number | null;
  printer_job_id: string | null;
};
