UPDATE commands
SET payload_json = json_set(payload_json, '$.bed_leveling', json('false'))
WHERE kind = 'print_project_file'
  AND json_valid(payload_json)
  AND json_type(payload_json, '$.bed_leveling') IS NULL;

UPDATE commands
SET payload_json = json_set(payload_json, '$.auto_bed_leveling', 0)
WHERE kind = 'print_project_file'
  AND json_valid(payload_json)
  AND json_type(payload_json, '$.auto_bed_leveling') IS NULL;

UPDATE commands
SET payload_json = json_set(
    payload_json,
    '$.auto_flow_cali',
    CASE WHEN json_extract(payload_json, '$.flow_cali') = 1 THEN 1 ELSE 0 END
)
WHERE kind = 'print_project_file'
  AND json_valid(payload_json)
  AND json_type(payload_json, '$.auto_flow_cali') IS NULL;

UPDATE commands
SET payload_json = json_set(payload_json, '$.auto_offset_cali', 0)
WHERE kind = 'print_project_file'
  AND json_valid(payload_json)
  AND json_type(payload_json, '$.auto_offset_cali') IS NULL;
