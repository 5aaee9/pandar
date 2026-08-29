CREATE TABLE artifact_quota_reservations (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    artifact_id TEXT NOT NULL UNIQUE,
    storage_path TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_artifact_quota_reservations_tenant_expiry
    ON artifact_quota_reservations(tenant_id, expires_at);
CREATE INDEX idx_artifact_quota_reservations_expiry
    ON artifact_quota_reservations(expires_at);

CREATE TABLE artifact_deletions (
    id TEXT PRIMARY KEY,
    storage_path TEXT NOT NULL UNIQUE,
    attempts BIGINT NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_artifact_deletions_created_at ON artifact_deletions(created_at);
