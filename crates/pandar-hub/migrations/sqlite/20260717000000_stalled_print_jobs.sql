ALTER TABLE jobs ADD COLUMN print_status_v2 TEXT NOT NULL DEFAULT 'pending'
    CHECK (print_status_v2 IN ('pending', 'stalled', 'running', 'completed', 'failed', 'cancelled'));

UPDATE jobs SET print_status_v2 = print_status;

ALTER TABLE jobs DROP COLUMN print_status;

ALTER TABLE jobs RENAME COLUMN print_status_v2 TO print_status;

CREATE INDEX idx_jobs_print_status_status ON jobs(print_status, status);
