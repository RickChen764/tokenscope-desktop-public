CREATE TABLE IF NOT EXISTS external_dataset (
  id TEXT PRIMARY KEY,
  device_id TEXT NOT NULL,
  device_name TEXT NOT NULL,
  package_version INTEGER NOT NULL,
  source_path TEXT,
  imported_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  calls INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0,
  estimated_cost_usd REAL NOT NULL DEFAULT 0
);

ALTER TABLE llm_call ADD COLUMN origin_dataset_id TEXT;

ALTER TABLE agent_import_map ADD COLUMN dataset_id TEXT;

CREATE INDEX IF NOT EXISTS idx_llm_call_origin_dataset
  ON llm_call (origin_dataset_id);

CREATE INDEX IF NOT EXISTS idx_agent_import_map_dataset
  ON agent_import_map (dataset_id);
