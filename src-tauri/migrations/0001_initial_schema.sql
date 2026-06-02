CREATE TABLE IF NOT EXISTS provider_config (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  display_name TEXT NOT NULL,
  base_url TEXT NOT NULL,
  api_key_ref TEXT,
  is_default INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS llm_call (
  id TEXT PRIMARY KEY,
  started_at TEXT NOT NULL,
  ended_at TEXT,
  date_local TEXT NOT NULL,

  provider TEXT NOT NULL,
  provider_config_id TEXT,
  api_type TEXT,
  model_requested TEXT,
  model_response TEXT,

  agent_id TEXT,
  agent_name TEXT,
  agent_run_id TEXT,
  workflow_id TEXT,
  workflow_step TEXT,
  session_id TEXT,
  trace_id TEXT,
  span_id TEXT,
  parent_span_id TEXT,

  project_id TEXT,
  user_id TEXT,
  environment TEXT,
  feature TEXT,

  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cached_input_tokens INTEGER NOT NULL DEFAULT 0,
  cache_write_input_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
  audio_input_tokens INTEGER NOT NULL DEFAULT 0,
  audio_output_tokens INTEGER NOT NULL DEFAULT 0,
  image_input_tokens INTEGER NOT NULL DEFAULT 0,
  image_output_tokens INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0,
  total_billable_tokens INTEGER NOT NULL DEFAULT 0,

  request_count INTEGER NOT NULL DEFAULT 1,
  tool_call_count INTEGER NOT NULL DEFAULT 0,
  retry_count INTEGER NOT NULL DEFAULT 0,

  latency_ms INTEGER,
  http_status INTEGER,
  status TEXT NOT NULL,
  error_type TEXT,
  error_message TEXT,

  estimated_cost_usd REAL NOT NULL DEFAULT 0,
  provider_reported_cost_usd REAL,
  reconciled_cost_usd REAL,
  cost_source TEXT,

  usage_source TEXT,
  raw_usage_json TEXT,
  raw_response_json TEXT,
  request_hash TEXT,
  response_hash TEXT,
  prompt_template_id TEXT,

  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS pricing_rule (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  input_usd_per_1m REAL NOT NULL,
  cached_input_usd_per_1m REAL NOT NULL DEFAULT 0,
  output_usd_per_1m REAL NOT NULL,
  reasoning_output_usd_per_1m REAL,
  effective_from TEXT NOT NULL,
  effective_to TEXT,
  source TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS daily_usage_agg (
  date_local TEXT NOT NULL,
  provider TEXT,
  model TEXT,
  agent_id TEXT,
  workflow_id TEXT,
  project_id TEXT,

  calls INTEGER NOT NULL DEFAULT 0,
  success_calls INTEGER NOT NULL DEFAULT 0,
  error_calls INTEGER NOT NULL DEFAULT 0,

  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cached_input_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0,

  estimated_cost_usd REAL NOT NULL DEFAULT 0,
  reconciled_cost_usd REAL,
  avg_latency_ms REAL,
  p95_latency_ms REAL,

  PRIMARY KEY (date_local, provider, model, agent_id, workflow_id, project_id)
);

CREATE INDEX IF NOT EXISTS idx_llm_call_date_local ON llm_call (date_local);
CREATE INDEX IF NOT EXISTS idx_llm_call_started_at ON llm_call (started_at);
CREATE INDEX IF NOT EXISTS idx_llm_call_agent ON llm_call (agent_id);
CREATE INDEX IF NOT EXISTS idx_llm_call_model ON llm_call (model_response, model_requested);
CREATE INDEX IF NOT EXISTS idx_llm_call_workflow ON llm_call (workflow_id);
CREATE INDEX IF NOT EXISTS idx_llm_call_status ON llm_call (status);
CREATE INDEX IF NOT EXISTS idx_pricing_rule_provider_model ON pricing_rule (provider, model, effective_from, effective_to);
