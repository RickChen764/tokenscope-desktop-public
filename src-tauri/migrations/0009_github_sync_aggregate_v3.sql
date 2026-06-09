ALTER TABLE external_dataset
  ADD COLUMN sync_data_mode TEXT NOT NULL DEFAULT 'detail_v2';

CREATE TABLE IF NOT EXISTS external_daily_usage (
  dataset_id TEXT NOT NULL,
  date_local TEXT NOT NULL,
  calls INTEGER NOT NULL DEFAULT 0,
  success_calls INTEGER NOT NULL DEFAULT 0,
  error_calls INTEGER NOT NULL DEFAULT 0,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cached_input_tokens INTEGER NOT NULL DEFAULT 0,
  cache_write_input_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0,
  estimated_cost_usd REAL NOT NULL DEFAULT 0,
  cost_currency TEXT NOT NULL DEFAULT 'USD',
  latency_sum_ms INTEGER NOT NULL DEFAULT 0,
  latency_count INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (dataset_id, date_local)
);

CREATE INDEX IF NOT EXISTS idx_external_daily_usage_date
  ON external_daily_usage (date_local);

CREATE TABLE IF NOT EXISTS external_dimension_usage (
  dataset_id TEXT NOT NULL,
  date_local TEXT NOT NULL,
  dimension_type TEXT NOT NULL,
  dimension_value TEXT NOT NULL,
  dimension_label TEXT,
  calls INTEGER NOT NULL DEFAULT 0,
  success_calls INTEGER NOT NULL DEFAULT 0,
  error_calls INTEGER NOT NULL DEFAULT 0,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cached_input_tokens INTEGER NOT NULL DEFAULT 0,
  cache_write_input_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0,
  estimated_cost_usd REAL NOT NULL DEFAULT 0,
  cost_currency TEXT NOT NULL DEFAULT 'USD',
  latency_sum_ms INTEGER NOT NULL DEFAULT 0,
  latency_count INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (dataset_id, date_local, dimension_type, dimension_value)
);

CREATE INDEX IF NOT EXISTS idx_external_dimension_usage_type_date
  ON external_dimension_usage (dimension_type, date_local);
