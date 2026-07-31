CREATE TABLE tenant_tokens (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    scopes_json TEXT NOT NULL,
    created_by_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    expires_at TEXT,
    revoked_at TEXT
);
CREATE INDEX idx_tenant_tokens_tenant_id ON tenant_tokens(tenant_id);
CREATE INDEX idx_tenant_tokens_hash ON tenant_tokens(token_hash);
CREATE INDEX idx_tenant_tokens_revoked_at ON tenant_tokens(revoked_at);

CREATE TABLE plugin_login_tickets (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    ticket_hash TEXT NOT NULL UNIQUE,
    redirect_url TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT,
    revoked_at TEXT
);
CREATE INDEX idx_plugin_login_tickets_tenant_id ON plugin_login_tickets(tenant_id);
CREATE INDEX idx_plugin_login_tickets_hash ON plugin_login_tickets(ticket_hash);

ALTER TABLE agents ADD COLUMN credential_hash TEXT;
ALTER TABLE agents ADD COLUMN credential_rotated_at TEXT;
ALTER TABLE agents ADD COLUMN credential_revoked_at TEXT;
CREATE INDEX idx_agents_credential_hash ON agents(credential_hash);
