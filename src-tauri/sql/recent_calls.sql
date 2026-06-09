SELECT
  c.id,
  c.started_at,
  c.provider,
  c.model_requested,
  c.model_response,
  c.agent_id,
  c.workflow_id,
  c.project_id,
  c.input_tokens,
  c.output_tokens,
  c.cached_input_tokens,
  c.reasoning_output_tokens,
  c.total_tokens,
  c.estimated_cost_usd,
  c.cost_currency,
  c.latency_ms,
  c.status
FROM llm_call c
LEFT JOIN external_dataset d ON d.id = c.origin_dataset_id
WHERE c.origin_dataset_id IS NULL
  OR d.sync_data_mode = 'detail_v2'
ORDER BY started_at DESC
LIMIT ?1;
