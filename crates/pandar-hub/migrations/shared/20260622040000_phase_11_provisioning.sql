ALTER TABLE api_tokens ADD COLUMN revoked_at TEXT;
CREATE INDEX idx_api_tokens_revoked_at ON api_tokens(revoked_at);
