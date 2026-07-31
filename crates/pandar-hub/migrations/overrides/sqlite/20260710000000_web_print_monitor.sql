ALTER TABLE printers ADD COLUMN state_revision INTEGER NOT NULL DEFAULT 1 CHECK (state_revision >= 1);
ALTER TABLE printers ADD COLUMN print_task_generation INTEGER NOT NULL DEFAULT 0 CHECK (print_task_generation >= 0);
ALTER TABLE printers ADD COLUMN print_error_generation INTEGER NOT NULL DEFAULT 0 CHECK (print_error_generation >= 0);
ALTER TABLE printers ADD COLUMN print_job_attr INTEGER;
ALTER TABLE printers ADD COLUMN print_error_task_generation INTEGER;
ALTER TABLE printers ADD COLUMN print_error_session_id TEXT;
ALTER TABLE printers ADD COLUMN print_error_received_at TEXT;
ALTER TABLE agents ADD COLUMN current_session_id TEXT;

UPDATE printers
SET print_task_generation = 1
WHERE print_task_id IS NOT NULL
   OR print_subtask_id IS NOT NULL
   OR print_progress_percent IS NOT NULL
   OR print_remaining_time_minutes IS NOT NULL
   OR print_current_layer IS NOT NULL
   OR print_total_layers IS NOT NULL
   OR print_gcode_file IS NOT NULL
   OR print_subtask_name IS NOT NULL
   OR print_job_id IS NOT NULL
   OR print_error > 0
   OR print_gcode_state IN ('PREPARE', 'SLICING', 'RUNNING', 'PAUSE', 'FINISH', 'FAILED');

UPDATE printers
SET print_error_generation = 1,
    print_error_task_generation = print_task_generation
WHERE print_error > 0;
