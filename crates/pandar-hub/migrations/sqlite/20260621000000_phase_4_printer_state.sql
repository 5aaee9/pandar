ALTER TABLE printers ADD COLUMN last_seen_at TEXT;

UPDATE printers SET last_seen_at = created_at WHERE last_seen_at IS NULL;

CREATE INDEX idx_printers_tenant_agent ON printers(tenant_id, agent_id);
