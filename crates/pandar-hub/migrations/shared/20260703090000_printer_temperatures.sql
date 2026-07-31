ALTER TABLE printers ADD COLUMN nozzle_temperatures_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE printers ADD COLUMN bed_temperature_celsius TEXT;
ALTER TABLE printers ADD COLUMN bed_target_temperature_celsius TEXT;
ALTER TABLE printers ADD COLUMN chamber_temperature_celsius TEXT;
