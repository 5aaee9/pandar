ALTER TABLE plugin_login_tickets ADD COLUMN kind TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE plugin_login_tickets ADD COLUMN code_challenge TEXT;

UPDATE plugin_login_tickets
SET revoked_at = COALESCE(revoked_at, created_at)
WHERE used_at IS NULL;

UPDATE tenant_tokens
SET revoked_at = COALESCE(revoked_at, created_at)
WHERE name = 'Android app' AND scopes_json = '["*"]';

CREATE INDEX idx_plugin_login_tickets_kind ON plugin_login_tickets(kind);
