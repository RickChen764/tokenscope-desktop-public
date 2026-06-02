SELECT
  provider AS dimension,
  COUNT(*) AS calls,
  COALESCE(SUM(total_tokens), 0) AS total_tokens,
  COALESCE(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd,
  CASE
    WHEN COUNT(DISTINCT COALESCE(cost_currency, 'USD')) = 1 THEN COALESCE(MAX(cost_currency), 'USD')
    ELSE 'MIXED'
  END AS cost_currency,
  AVG(latency_ms) AS avg_latency_ms
FROM llm_call
WHERE date_local BETWEEN ?1 AND ?2
  AND provider IS NOT NULL
  AND provider <> ''
GROUP BY provider
ORDER BY total_tokens DESC, calls DESC, dimension ASC
LIMIT ?3;
