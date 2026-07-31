CREATE TABLE printer_event_tickets (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    ticket_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used_at TEXT
);
CREATE INDEX idx_printer_event_tickets_tenant_id ON printer_event_tickets(tenant_id);
CREATE INDEX idx_printer_event_tickets_hash ON printer_event_tickets(ticket_hash);
CREATE INDEX idx_printer_event_tickets_expires_at ON printer_event_tickets(expires_at);
