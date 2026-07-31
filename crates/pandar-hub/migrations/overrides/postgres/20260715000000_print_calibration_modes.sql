UPDATE commands
SET payload_json = (
    payload_json::jsonb || jsonb_build_object('bed_leveling', false)
)::text
WHERE kind = 'print_project_file'
  AND NOT (payload_json::jsonb ? 'bed_leveling');

UPDATE commands
SET payload_json = (
    payload_json::jsonb || jsonb_build_object('auto_bed_leveling', 0)
)::text
WHERE kind = 'print_project_file'
  AND NOT (payload_json::jsonb ? 'auto_bed_leveling');

UPDATE commands
SET payload_json = (
    payload_json::jsonb || jsonb_build_object(
        'auto_flow_cali',
        CASE
            WHEN COALESCE((payload_json::jsonb ->> 'flow_cali')::boolean, false) THEN 1
            ELSE 0
        END
    )
)::text
WHERE kind = 'print_project_file'
  AND NOT (payload_json::jsonb ? 'auto_flow_cali');

UPDATE commands
SET payload_json = (
    payload_json::jsonb || jsonb_build_object('auto_offset_cali', 0)
)::text
WHERE kind = 'print_project_file'
  AND NOT (payload_json::jsonb ? 'auto_offset_cali');
