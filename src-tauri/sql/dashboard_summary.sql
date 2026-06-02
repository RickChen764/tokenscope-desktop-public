WITH filtered AS (
  SELECT
    *
  FROM llm_call
  WHERE date_local BETWEEN ?1 AND ?2
),
summary AS (
  SELECT
    COALESCE(SUM(total_tokens), 0) AS total_tokens,
    COALESCE(SUM(input_tokens), 0) AS input_tokens,
    COALESCE(SUM(output_tokens), 0) AS output_tokens,
    COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
    COALESCE(SUM(reasoning_output_tokens), 0) AS reasoning_output_tokens,
    COALESCE(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd,
    CASE
      WHEN COUNT(*) = 0 THEN 'USD'
      WHEN COUNT(DISTINCT COALESCE(cost_currency, 'USD')) = 1 THEN COALESCE(MAX(cost_currency), 'USD')
      ELSE 'MIXED'
    END AS cost_currency,
    COUNT(*) AS calls,
    COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) AS success_calls,
    COALESCE(SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END), 0) AS error_calls,
    AVG(latency_ms) AS avg_latency_ms
  FROM filtered
),
top_agent AS (
  SELECT agent_id
  FROM filtered
  WHERE agent_id IS NOT NULL AND agent_id <> ''
  GROUP BY agent_id
  ORDER BY SUM(total_tokens) DESC, COUNT(*) DESC, agent_id ASC
  LIMIT 1
),
top_model AS (
  SELECT COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) AS model
  FROM filtered
  WHERE COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) IS NOT NULL
  GROUP BY model
  ORDER BY SUM(total_tokens) DESC, COUNT(*) DESC, model ASC
  LIMIT 1
)
SELECT
  summary.total_tokens,
  summary.input_tokens,
  summary.output_tokens,
  summary.cached_input_tokens,
  summary.reasoning_output_tokens,
  summary.estimated_cost_usd,
  summary.cost_currency,
  summary.calls,
  summary.success_calls,
  summary.error_calls,
  CASE
    WHEN summary.calls = 0 THEN 0.0
    ELSE CAST(summary.error_calls AS REAL) / CAST(summary.calls AS REAL)
  END AS error_rate,
  summary.avg_latency_ms,
  (SELECT agent_id FROM top_agent) AS top_agent_id,
  (SELECT model FROM top_model) AS top_model
FROM summary;
