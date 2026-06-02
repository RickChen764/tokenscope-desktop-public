SELECT
  id,
  started_at,
  provider,
  model_requested,
  model_response,
  agent_id,
  workflow_id,
  project_id,
  input_tokens,
  output_tokens,
  cached_input_tokens,
  reasoning_output_tokens,
  total_tokens,
  estimated_cost_usd,
  cost_currency,
  latency_ms,
  status
FROM llm_call
ORDER BY started_at DESC
LIMIT ?1;
