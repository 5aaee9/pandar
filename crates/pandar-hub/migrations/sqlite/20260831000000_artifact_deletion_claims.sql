ALTER TABLE artifact_deletions ADD COLUMN lease_owner TEXT;
ALTER TABLE artifact_deletions ADD COLUMN lease_expires_at TEXT;

CREATE INDEX idx_artifact_deletions_claim
    ON artifact_deletions(lease_expires_at, created_at);
