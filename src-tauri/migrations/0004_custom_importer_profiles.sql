CREATE TABLE IF NOT EXISTS custom_importer_profile (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  source_key TEXT NOT NULL UNIQUE,
  database_path TEXT NOT NULL,
  import_sql TEXT NOT NULL,
  mappings_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS custom_importer_run (
  id TEXT PRIMARY KEY,
  profile_id TEXT NOT NULL,
  status TEXT NOT NULL,
  imported INTEGER NOT NULL DEFAULT 0,
  skipped INTEGER NOT NULL DEFAULT 0,
  error TEXT,
  started_at TEXT NOT NULL,
  finished_at TEXT NOT NULL,
  FOREIGN KEY (profile_id) REFERENCES custom_importer_profile(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS custom_importer_profile_source_key_idx
  ON custom_importer_profile(source_key);

CREATE INDEX IF NOT EXISTS custom_importer_run_profile_started_idx
  ON custom_importer_run(profile_id, started_at DESC);
