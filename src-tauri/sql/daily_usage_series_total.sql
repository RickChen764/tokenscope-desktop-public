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
combined AS (
  SELECT
    date_local,
    NULL AS dimension,
    COUNT(*) AS calls,
    COALESCE(SUM(input_tokens), 0) AS input_tokens,
    COALESCE(SUM(output_tokens), 0) AS output_tokens,
    COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
    COALESCE(SUM(reasoning_output_tokens), 0) AS reasoning_output_tokens,
    COALESCE(SUM(total_tokens), 0) AS total_tokens,
    COALESCE(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd,
    COALESCE(cost_currency, 'USD') AS cost_currency
  FROM detail_filtered
  GROUP BY date_local, COALESCE(cost_currency, 'USD')
  UNION ALL
  SELECT
    a.date_local,
    NULL AS dimension,
    COALESCE(SUM(a.calls), 0) AS calls,
    COALESCE(SUM(a.input_tokens), 0) AS input_tokens,
    COALESCE(SUM(a.output_tokens), 0) AS output_tokens,
    COALESCE(SUM(a.cached_input_tokens), 0) AS cached_input_tokens,
    COALESCE(SUM(a.reasoning_output_tokens), 0) AS reasoning_output_tokens,
    COALESCE(SUM(a.total_tokens), 0) AS total_tokens,
    COALESCE(SUM(a.estimated_cost_usd), 0.0) AS estimated_cost_usd,
    COALESCE(a.cost_currency, 'USD') AS cost_currency
  FROM external_daily_usage a
  JOIN external_dataset d ON d.id = a.dataset_id
  WHERE a.date_local BETWEEN ?1 AND ?2
    AND d.sync_data_mode = 'aggregate_v3'
  GROUP BY a.date_local, COALESCE(a.cost_currency, 'USD')
)
SELECT
  date_local,
  NULL AS dimension,
  COALESCE(SUM(calls), 0) AS calls,
  COALESCE(SUM(input_tokens), 0) AS input_tokens,
  COALESCE(SUM(output_tokens), 0) AS output_tokens,
  COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
  COALESCE(SUM(reasoning_output_tokens), 0) AS reasoning_output_tokens,
  COALESCE(SUM(total_tokens), 0) AS total_tokens,
  COALESCE(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd,
  CASE
    WHEN COUNT(DISTINCT cost_currency) = 1 THEN COALESCE(MAX(cost_currency), 'USD')
    ELSE 'MIXED'
  END AS cost_currency
FROM combined
GROUP BY date_local
ORDER BY date_local ASC;
