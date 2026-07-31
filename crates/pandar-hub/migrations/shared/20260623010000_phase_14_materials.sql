ALTER TABLE jobs ADD COLUMN ams_mapping_json TEXT;
ALTER TABLE jobs ADD COLUMN ams_mapping2_json TEXT;

CREATE TABLE printer_material_snapshots (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    printer_id TEXT NOT NULL REFERENCES printers(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    serial_number TEXT NOT NULL,
    ams_json TEXT NOT NULL,
    external_spools_json TEXT NOT NULL,
    active_tray_json TEXT,
    observed_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, printer_id)
);

CREATE INDEX idx_printer_material_snapshots_tenant_printer
    ON printer_material_snapshots(tenant_id, printer_id);
CREATE INDEX idx_printer_material_snapshots_tenant_serial
    ON printer_material_snapshots(tenant_id, serial_number);

CREATE TABLE job_filament_usages (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    slot_index INTEGER NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('ams_mapping2', 'ams_mapping')),
    ams_id TEXT,
    tray_id TEXT,
    global_tray_id INTEGER,
    external_id TEXT,
    filament_id TEXT,
    setting_id TEXT,
    filament_type TEXT,
    color TEXT,
    used_mm TEXT,
    used_grams TEXT,
    confidence TEXT NOT NULL CHECK (confidence IN ('mapped_no_quantity', 'report_estimate')),
    created_at TEXT NOT NULL,
    UNIQUE (tenant_id, job_id, slot_index, source)
);

CREATE INDEX idx_job_filament_usages_tenant_job ON job_filament_usages(tenant_id, job_id);
CREATE INDEX idx_job_filament_usages_tenant_job_slot
    ON job_filament_usages(tenant_id, job_id, slot_index);
