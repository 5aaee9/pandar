ALTER TABLE printers ADD COLUMN firmware_modules_json TEXT;
ALTER TABLE printers ADD COLUMN firmware_upgrade_state_json TEXT;
ALTER TABLE printers ADD COLUMN firmware_cfg TEXT;
ALTER TABLE printers ADD COLUMN firmware_session_id TEXT;
ALTER TABLE printers ADD COLUMN firmware_generation INTEGER;
ALTER TABLE printers ADD COLUMN firmware_module_revision INTEGER NOT NULL DEFAULT 0 CHECK (firmware_module_revision >= 0);
ALTER TABLE printers ADD COLUMN firmware_status_revision INTEGER NOT NULL DEFAULT 0 CHECK (firmware_status_revision >= 0);
