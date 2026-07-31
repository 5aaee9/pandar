ALTER TABLE jobs ADD COLUMN studio_submission_id INTEGER NOT NULL DEFAULT 1
    CHECK (studio_submission_id BETWEEN 1 AND 2147483647);
ALTER TABLE jobs ADD COLUMN plate_index INTEGER NOT NULL DEFAULT 1
    CHECK (plate_index BETWEEN 1 AND 2147483647);
ALTER TABLE jobs ADD COLUMN studio_metadata_json TEXT;

UPDATE jobs
SET plate_index = COALESCE(
    (
        SELECT CASE
            WHEN json_type(commands.payload_json, '$.plate_id') = 'integer'
                 AND json_extract(commands.payload_json, '$.plate_id') BETWEEN 1 AND 2147483647
            THEN json_extract(commands.payload_json, '$.plate_id')
            ELSE 1
        END
        FROM commands
        WHERE commands.id = jobs.command_id
          AND commands.kind = 'print_project_file'
    ),
    1
);

WITH ranked AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY tenant_id
               ORDER BY julianday(created_at) ASC, id ASC
           ) AS studio_submission_id
    FROM jobs
)
UPDATE jobs
SET studio_submission_id = (
    SELECT ranked.studio_submission_id
    FROM ranked
    WHERE ranked.id = jobs.id
);

UPDATE commands
SET payload_json = json_set(
    payload_json,
    '$.studio_submission_id',
    COALESCE(
        (SELECT jobs.studio_submission_id FROM jobs WHERE jobs.command_id = commands.id),
        1
    ),
    '$.studio_metadata',
    NULL
)
WHERE kind = 'print_project_file';

CREATE TABLE studio_submission_sequences (
    tenant_id TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    last_id INTEGER NOT NULL CHECK (last_id BETWEEN 0 AND 2147483647)
);

INSERT INTO studio_submission_sequences (tenant_id, last_id)
SELECT tenant_id, MAX(studio_submission_id)
FROM jobs
GROUP BY tenant_id;

CREATE UNIQUE INDEX idx_jobs_tenant_studio_submission
    ON jobs(tenant_id, studio_submission_id);
CREATE INDEX idx_jobs_studio_task_list
    ON jobs(tenant_id, printer_id, print_status, status, studio_submission_id DESC);
