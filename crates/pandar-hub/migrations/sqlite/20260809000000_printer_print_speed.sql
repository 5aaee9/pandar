ALTER TABLE printers ADD COLUMN print_speed_level INTEGER
    CHECK (print_speed_level IS NULL OR print_speed_level BETWEEN 1 AND 4);
