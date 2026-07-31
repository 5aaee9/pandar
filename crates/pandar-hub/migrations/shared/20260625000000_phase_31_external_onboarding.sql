CREATE TABLE join_links (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL,
    email_constraint TEXT,
    expires_at TEXT NOT NULL,
    max_uses INTEGER NOT NULL,
    used_count INTEGER NOT NULL DEFAULT 0,
    created_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    revoked_at TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX join_links_tenant_id_created_at_idx ON join_links (tenant_id, created_at);
