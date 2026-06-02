SELECT
  agent_import_map.source AS source_key,
  COUNT(agent_import_map.llm_call_id) AS imported_calls,
  COALESCE(SUM(llm_call.total_tokens), 0) AS total_tokens,
  COALESCE(SUM(llm_call.estimated_cost_usd), 0.0) AS estimated_cost_usd,
  CASE
    WHEN COUNT(DISTINCT COALESCE(llm_call.cost_currency, 'USD')) = 1 THEN COALESCE(MAX(llm_call.cost_currency), 'USD')
    ELSE 'MIXED'
  END AS cost_currency,
  MAX(agent_import_map.imported_at) AS last_imported_at,
  MAX(llm_call.started_at) AS last_call_at
FROM agent_import_map
LEFT JOIN llm_call ON llm_call.id = agent_import_map.llm_call_id
GROUP BY agent_import_map.source
ORDER BY agent_import_map.source ASC;
