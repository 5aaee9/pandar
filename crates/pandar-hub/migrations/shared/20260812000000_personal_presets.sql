CREATE TABLE personal_preset_clocks (
    tenant_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    last_updated_time BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, owner_user_id),
    FOREIGN KEY (tenant_id, owner_user_id) REFERENCES users(tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE personal_presets (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    owner_user_id TEXT NOT NULL,
    preset_type TEXT NOT NULL CHECK (preset_type IN ('print', 'filament', 'printer')),
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    base_id TEXT NOT NULL,
    inherits TEXT,
    filament_id TEXT,
    options_json TEXT NOT NULL,
    updated_time BIGINT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (tenant_id, owner_user_id, name),
    FOREIGN KEY (tenant_id, owner_user_id) REFERENCES users(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX personal_presets_owner_updated_idx
    ON personal_presets (tenant_id, owner_user_id, updated_time, id);
