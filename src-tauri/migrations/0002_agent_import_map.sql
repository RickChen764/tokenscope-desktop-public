CREATE TABLE IF NOT EXISTS agent_import_map (
  source TEXT NOT NULL,
  external_id TEXT NOT NULL,
  llm_call_id TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  PRIMARY KEY (source, external_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_import_map_call ON agent_import_map (llm_call_id);
