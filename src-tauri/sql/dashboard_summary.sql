WITH detail_filtered AS (
  SELECT c.*
  FROM llm_call c
  LEFT JOIN external_dataset d ON d.id = c.origin_dataset_id
  WHERE c.date_local BETWEEN ?1 AND ?2
    AND (
      c.origin_dataset_id IS NULL
      OR d.sync_data_mode = 'detail_v2'
    )
),
aggregate_filtered AS (
  SELECT a.*
  FROM external_daily_usage a
  JOIN external_dataset d ON d.id = a.dataset_id
  WHERE a.date_local BETWEEN ?1 AND ?2
    AND d.sync_data_mode = 'aggregate_v3'
),
detail_summary AS (
  SELECT
    COALESCE(SUM(total_tokens), 0) AS total_tokens,
    COALESCE(SUM(input_tokens), 0) AS input_tokens,
    COALESCE(SUM(output_tokens), 0) AS output_tokens,
    COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
    COALESCE(SUM(reasoning_output_tokens), 0) AS reasoning_output_tokens,
    COALESCE(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd,
    COUNT(*) AS calls,
    COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) AS success_calls,
    COALESCE(SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END), 0) AS error_calls,
    COALESCE(SUM(CASE WHEN latency_ms IS NOT NULL THEN latency_ms ELSE 0 END), 0) AS latency_sum_ms,
    COALESCE(SUM(CASE WHEN latency_ms IS NOT NULL THEN 1 ELSE 0 END), 0) AS latency_count
  FROM detail_filtered
),
aggregate_summary AS (
  SELECT
    COALESCE(SUM(total_tokens), 0) AS total_tokens,
    COALESCE(SUM(input_tokens), 0) AS input_tokens,
    COALESCE(SUM(output_tokens), 0) AS output_tokens,
    COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
    COALESCE(SUM(reasoning_output_tokens), 0) AS reasoning_output_tokens,
    COALESCE(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd,
    COALESCE(SUM(calls), 0) AS calls,
    COALESCE(SUM(success_calls), 0) AS success_calls,
    COALESCE(SUM(error_calls), 0) AS error_calls,
    COALESCE(SUM(latency_sum_ms), 0) AS latency_sum_ms,
    COALESCE(SUM(latency_count), 0) AS latency_count
  FROM aggregate_filtered
),
cost_currencies AS (
  SELECT COALESCE(cost_currency, 'USD') AS cost_currency
  FROM detail_filtered
  UNION ALL
  SELECT COALESCE(cost_currency, 'USD') AS cost_currency
  FROM aggregate_filtered
  WHERE calls > 0
),
top_agent_candidates AS (
  SELECT
    agent_id AS dimension,
    COUNT(*) AS calls,
    COALESCE(SUM(total_tokens), 0) AS total_tokens
  FROM detail_filtered
  WHERE agent_id IS NOT NULL AND agent_id <> ''
  GROUP BY agent_id
  UNION ALL
  SELECT
    dimension_value AS dimension,
    COALESCE(SUM(a.calls), 0) AS calls,
    COALESCE(SUM(a.total_tokens), 0) AS total_tokens
  FROM external_dimension_usage a
  JOIN external_dataset d ON d.id = a.dataset_id
  WHERE a.date_local BETWEEN ?1 AND ?2
    AND d.sync_data_mode = 'aggregate_v3'
    AND a.dimension_type = 'agent'
  GROUP BY dimension_value
),
top_model_candidates AS (
  SELECT
    COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) AS dimension,
    COUNT(*) AS calls,
    COALESCE(SUM(total_tokens), 0) AS total_tokens
  FROM detail_filtered
  WHERE COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) IS NOT NULL
  GROUP BY dimension
  UNION ALL
  SELECT
    dimension_value AS dimension,
    COALESCE(SUM(a.calls), 0) AS calls,
    COALESCE(SUM(a.total_tokens), 0) AS total_tokens
  FROM external_dimension_usage a
  JOIN external_dataset d ON d.id = a.dataset_id
  WHERE a.date_local BETWEEN ?1 AND ?2
    AND d.sync_data_mode = 'aggregate_v3'
    AND a.dimension_type = 'model'
  GROUP BY dimension_value
),
summary AS (
  SELECT
    detail_summary.total_tokens + aggregate_summary.total_tokens AS total_tokens,
    detail_summary.input_tokens + aggregate_summary.input_tokens AS input_tokens,
    detail_summary.output_tokens + aggregate_summary.output_tokens AS output_tokens,
    detail_summary.cached_input_tokens + aggregate_summary.cached_input_tokens AS cached_input_tokens,
    detail_summary.reasoning_output_tokens + aggregate_summary.reasoning_output_tokens AS reasoning_output_tokens,
    detail_summary.estimated_cost_usd + aggregate_summary.estimated_cost_usd AS estimated_cost_usd,
    detail_summary.calls + aggregate_summary.calls AS calls,
    detail_summary.success_calls + aggregate_summary.success_calls AS success_calls,
    detail_summary.error_calls + aggregate_summary.error_calls AS error_calls,
    detail_summary.latency_sum_ms + aggregate_summary.latency_sum_ms AS latency_sum_ms,
    detail_summary.latency_count + aggregate_summary.latency_count AS latency_count
  FROM detail_summary, aggregate_summary
)
SELECT
  summary.total_tokens,
  summary.input_tokens,
  summary.output_tokens,
  summary.cached_input_tokens,
  summary.reasoning_output_tokens,
  summary.estimated_cost_usd,
  CASE
    WHEN (SELECT COUNT(*) FROM cost_currencies) = 0 THEN 'USD'
    WHEN (SELECT COUNT(DISTINCT cost_currency) FROM cost_currencies) = 1 THEN (SELECT MAX(cost_currency) FROM cost_currencies)
    ELSE 'MIXED'
  END AS cost_currency,
  summary.calls,
  summary.success_calls,
  summary.error_calls,
  CASE
    WHEN summary.calls = 0 THEN 0.0
    ELSE CAST(summary.error_calls AS REAL) / CAST(summary.calls AS REAL)
  END AS error_rate,
  CASE
    WHEN summary.latency_count = 0 THEN NULL
    ELSE CAST(summary.latency_sum_ms AS REAL) / CAST(summary.latency_count AS REAL)
  END AS avg_latency_ms,
  (
    SELECT dimension
    FROM top_agent_candidates
    GROUP BY dimension
    ORDER BY SUM(total_tokens) DESC, SUM(calls) DESC, dimension ASC
    LIMIT 1
  ) AS top_agent_id,
  (
    SELECT dimension
    FROM top_model_candidates
    GROUP BY dimension
    ORDER BY SUM(total_tokens) DESC, SUM(calls) DESC, dimension ASC
    LIMIT 1
  ) AS top_model
FROM summary;
