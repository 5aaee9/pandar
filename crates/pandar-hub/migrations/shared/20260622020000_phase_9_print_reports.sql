ALTER TABLE jobs ADD COLUMN print_status TEXT NOT NULL DEFAULT 'pending'
    CHECK (print_status IN ('pending', 'running', 'completed', 'failed', 'cancelled'));
ALTER TABLE jobs ADD COLUMN printer_state TEXT;
ALTER TABLE jobs ADD COLUMN progress_percent INTEGER
    CHECK (progress_percent IS NULL OR progress_percent BETWEEN 0 AND 100);
ALTER TABLE jobs ADD COLUMN remaining_time_minutes INTEGER
    CHECK (remaining_time_minutes IS NULL OR remaining_time_minutes BETWEEN 0 AND 4320);
ALTER TABLE jobs ADD COLUMN current_layer INTEGER
    CHECK (current_layer IS NULL OR current_layer BETWEEN 0 AND 100000);
ALTER TABLE jobs ADD COLUMN total_layers INTEGER
    CHECK (total_layers IS NULL OR total_layers BETWEEN 0 AND 100000);
ALTER TABLE jobs ADD COLUMN active_file TEXT;
ALTER TABLE jobs ADD COLUMN last_progress_percent INTEGER
    CHECK (last_progress_percent IS NULL OR last_progress_percent BETWEEN 0 AND 100);
ALTER TABLE jobs ADD COLUMN last_layer INTEGER
    CHECK (last_layer IS NULL OR last_layer BETWEEN 0 AND 100000);
ALTER TABLE jobs ADD COLUMN print_error TEXT;
ALTER TABLE jobs ADD COLUMN print_started_at TEXT;
ALTER TABLE jobs ADD COLUMN print_finished_at TEXT;
ALTER TABLE jobs ADD COLUMN print_updated_at TEXT;

CREATE TABLE machine_events (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    printer_id TEXT NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
    job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    event_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('print_progress', 'print_terminal', 'print_error', 'hms')),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'error')),
    message TEXT NOT NULL,
    code TEXT,
    payload_json TEXT NOT NULL,
    observed_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (tenant_id, event_key)
);
CREATE INDEX idx_machine_events_tenant_id ON machine_events(tenant_id);
CREATE INDEX idx_machine_events_printer_id ON machine_events(printer_id);
CREATE INDEX idx_machine_events_job_id ON machine_events(job_id);
