pub(super) const JOB_SELECTION_SQL: &str = "\
SELECT jobs.id
FROM jobs
JOIN commands ON commands.id = jobs.command_id
WHERE jobs.updated_at < ?
  AND jobs.status IN ('succeeded', 'failed')
  AND jobs.print_status IN ('completed', 'failed', 'cancelled', 'pending')
  AND jobs.print_status <> 'running'
  AND commands.status NOT IN ('queued', 'sent', 'acknowledged')";

pub(super) const ARTIFACT_SELECTION_SQL: &str = "\
SELECT artifact.id
FROM job_artifacts artifact
WHERE EXISTS (
    SELECT 1
    FROM jobs selected_jobs
    JOIN commands ON commands.id = selected_jobs.command_id
    WHERE selected_jobs.artifact_id = artifact.id
      AND selected_jobs.updated_at < ?
      AND selected_jobs.status IN ('succeeded', 'failed')
      AND selected_jobs.print_status IN ('completed', 'failed', 'cancelled', 'pending')
      AND commands.status NOT IN ('queued', 'sent', 'acknowledged')
  )
  AND NOT EXISTS (
    SELECT 1
    FROM jobs retained
    WHERE retained.artifact_id = artifact.id
      AND NOT EXISTS (
        SELECT 1
        FROM commands retained_command
        WHERE retained_command.id = retained.command_id
          AND retained.updated_at < ?
          AND retained.status IN ('succeeded', 'failed')
          AND retained.print_status IN ('completed', 'failed', 'cancelled', 'pending')
          AND retained_command.status NOT IN ('queued', 'sent', 'acknowledged')
      )
  )";

pub(super) const COMMAND_SELECTION_SQL: &str = "\
SELECT commands.id
FROM commands
WHERE commands.updated_at < ?
  AND commands.status IN ('succeeded', 'failed')
  AND NOT EXISTS (
    SELECT 1
    FROM jobs retained
    WHERE retained.command_id = commands.id
      AND NOT (
        retained.updated_at < ?
        AND retained.status IN ('succeeded', 'failed')
        AND retained.print_status IN ('completed', 'failed', 'cancelled', 'pending')
        AND commands.status NOT IN ('queued', 'sent', 'acknowledged')
      )
  )";

pub(super) const AUDIT_SELECTION_SQL: &str = "\
SELECT audit_events.id
FROM audit_events
WHERE audit_events.created_at < ?
  AND NOT (
    audit_events.target_type = 'job'
    AND audit_events.target_id IN (
      SELECT retained.id
      FROM jobs retained
      JOIN commands retained_command ON retained_command.id = retained.command_id
      WHERE NOT (
        retained.updated_at < ?
        AND retained.status IN ('succeeded', 'failed')
        AND retained.print_status IN ('completed', 'failed', 'cancelled', 'pending')
        AND retained_command.status NOT IN ('queued', 'sent', 'acknowledged')
      )
    )
  )
  AND NOT (
    audit_events.target_type = 'command'
    AND audit_events.target_id IN (
      SELECT retained.id
      FROM commands retained
      WHERE NOT (
        retained.updated_at < ?
        AND retained.status IN ('succeeded', 'failed')
        AND NOT EXISTS (
          SELECT 1
          FROM jobs retained_job
          WHERE retained_job.command_id = retained.id
            AND NOT (
              retained_job.updated_at < ?
              AND retained_job.status IN ('succeeded', 'failed')
              AND retained_job.print_status IN ('completed', 'failed', 'cancelled', 'pending')
              AND retained.status NOT IN ('queued', 'sent', 'acknowledged')
            )
        )
      )
    )
  )";

pub(super) const PLUGIN_TICKET_SELECTION_SQL: &str = "\
SELECT id
FROM plugin_login_tickets
WHERE (used_at IS NOT NULL AND used_at < ?)
   OR (revoked_at IS NOT NULL AND revoked_at < ?)
   OR (expires_at < ?)";

pub(super) const TENANT_TOKEN_SELECTION_SQL: &str = "\
SELECT id
FROM tenant_tokens
WHERE (revoked_at IS NOT NULL AND revoked_at < ?)
   OR (expires_at IS NOT NULL AND expires_at < ?)";

pub(super) const DELETE_JOBS_SQL: &str = "\
DELETE FROM jobs
WHERE id IN (
  SELECT jobs.id
  FROM jobs
  JOIN commands ON commands.id = jobs.command_id
  WHERE jobs.updated_at < ?
    AND jobs.status IN ('succeeded', 'failed')
    AND jobs.print_status IN ('completed', 'failed', 'cancelled', 'pending')
    AND commands.status NOT IN ('queued', 'sent', 'acknowledged')
)";

pub(super) const DELETE_ARTIFACTS_SQL: &str = "\
DELETE FROM job_artifacts
WHERE NOT EXISTS (
  SELECT 1 FROM jobs retained WHERE retained.artifact_id = job_artifacts.id
)
AND id IN (";

pub(super) const DELETE_COMMANDS_SQL: &str = "\
DELETE FROM commands
WHERE id IN (
  SELECT commands.id
  FROM commands
  WHERE commands.updated_at < ?
    AND commands.status IN ('succeeded', 'failed')
    AND NOT EXISTS (
      SELECT 1
      FROM jobs retained
      WHERE retained.command_id = commands.id
        AND NOT (
          retained.updated_at < ?
          AND retained.status IN ('succeeded', 'failed')
          AND retained.print_status IN ('completed', 'failed', 'cancelled', 'pending')
          AND commands.status NOT IN ('queued', 'sent', 'acknowledged')
        )
    )
)";

pub(super) const DELETE_AUDIT_SQL: &str = "\
DELETE FROM audit_events
WHERE id IN (";

pub(super) const DELETE_PLUGIN_TICKETS_SQL: &str = "\
DELETE FROM plugin_login_tickets
WHERE id IN (
  SELECT id
  FROM plugin_login_tickets
  WHERE (used_at IS NOT NULL AND used_at < ?)
     OR (revoked_at IS NOT NULL AND revoked_at < ?)
     OR (expires_at < ?)
)";

pub(super) const DELETE_TENANT_TOKENS_SQL: &str = "\
DELETE FROM tenant_tokens
WHERE id IN (
  SELECT id
  FROM tenant_tokens
  WHERE (revoked_at IS NOT NULL AND revoked_at < ?)
     OR (expires_at IS NOT NULL AND expires_at < ?)
)";
