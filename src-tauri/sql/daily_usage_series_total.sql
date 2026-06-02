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
  CASE
    WHEN COUNT(DISTINCT COALESCE(cost_currency, 'USD')) = 1 THEN COALESCE(MAX(cost_currency), 'USD')
    ELSE 'MIXED'
  END AS cost_currency
FROM llm_call
WHERE date_local BETWEEN ?1 AND ?2
GROUP BY date_local
ORDER BY date_local ASC;
