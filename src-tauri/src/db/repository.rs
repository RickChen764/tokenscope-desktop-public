use std::path::Path;

use chrono::{DateTime, Duration, Local};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{query, query_as, QueryBuilder, Row, Sqlite, SqlitePool};
use uuid::Uuid;

use crate::pricing::{
    estimate_cost, normalize_currency, CostRecalculationResult, PricingRule,
    PricingRuleImportResult, PricingRuleInput,
};
use crate::usage::{NormalizedUsage, UsageSource};

use super::models::{
    AgentSourceStats, AppSettings, AppSettingsInput, CallFilterOptions, CustomImporterProfile,
    CustomImporterProfileInput, CustomImporterRunResult, DailyUsagePoint, DashboardSummary,
    DataHealthIssueRow, DataHealthIssueSummary, DataHealthSummary, ExternalDataset,
    ExternalDatasetImportCall, ExternalDatasetInput, LlmCallFilters, LlmCallPage, LlmCallRow,
    NewLlmCall, ProviderConfig, ProviderConfigInput, SyncSettings, SyncSettingsInput,
    TopDimensionRow, UnknownPricingModel,
};

const DASHBOARD_SUMMARY_SQL: &str = include_str!("../../sql/dashboard_summary.sql");
const DAILY_USAGE_SERIES_TOTAL_SQL: &str = include_str!("../../sql/daily_usage_series_total.sql");
const TOP_AGENTS_SQL: &str = include_str!("../../sql/top_agents.sql");
const TOP_MODELS_SQL: &str = include_str!("../../sql/top_models.sql");
const TOP_PROVIDERS_SQL: &str = include_str!("../../sql/top_providers.sql");
const TOP_WORKFLOWS_SQL: &str = include_str!("../../sql/top_workflows.sql");
const TOP_PROJECTS_SQL: &str = include_str!("../../sql/top_projects.sql");
const TOP_SESSIONS_SQL: &str = include_str!("../../sql/top_sessions.sql");
const RECENT_CALLS_SQL: &str = include_str!("../../sql/recent_calls.sql");
const AGENT_SOURCE_STATS_SQL: &str = include_str!("../../sql/agent_source_stats.sql");
const SEED_DEMO_SQL: &str = include_str!("../../sql/seed_demo.sql");

#[derive(Clone)]
pub struct TokenScopeRepository {
    pool: SqlitePool,
}

impl TokenScopeRepository {
    pub async fn connect(path: &Path) -> Result<Self, sqlx::Error> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        Ok(Self { pool })
    }

    #[cfg(test)]
    pub async fn connect_in_memory() -> Result<Self, sqlx::Error> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!("./migrations").run(&self.pool).await
    }

    pub async fn insert_llm_call(&self, call: &NewLlmCall) -> Result<(), sqlx::Error> {
        query(
            r#"
      INSERT OR REPLACE INTO llm_call (
        id,
        started_at,
        ended_at,
        date_local,
        provider,
        provider_config_id,
        api_type,
        model_requested,
        model_response,
        agent_id,
        agent_name,
        agent_run_id,
        workflow_id,
        workflow_step,
        session_id,
        trace_id,
        span_id,
        parent_span_id,
        project_id,
        user_id,
        environment,
        feature,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        audio_input_tokens,
        audio_output_tokens,
        image_input_tokens,
        image_output_tokens,
        total_tokens,
        total_billable_tokens,
        request_count,
        tool_call_count,
        retry_count,
        latency_ms,
        http_status,
        status,
        error_type,
        error_message,
        estimated_cost_usd,
        cost_currency,
        provider_reported_cost_usd,
        reconciled_cost_usd,
        cost_source,
        usage_source,
        raw_usage_json,
        raw_response_json,
        request_hash,
        response_hash,
        prompt_template_id,
        created_at
      ) VALUES (
        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
        ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
        ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
        ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40,
        ?41, ?42, ?43, ?44, ?45, ?46, ?47, ?48, ?49, ?50,
        ?51, ?52, ?53
      )
      "#,
        )
        .bind(&call.id)
        .bind(&call.started_at)
        .bind(&call.ended_at)
        .bind(&call.date_local)
        .bind(&call.provider)
        .bind(&call.provider_config_id)
        .bind(&call.api_type)
        .bind(&call.model_requested)
        .bind(&call.model_response)
        .bind(&call.agent_id)
        .bind(&call.agent_name)
        .bind(&call.agent_run_id)
        .bind(&call.workflow_id)
        .bind(&call.workflow_step)
        .bind(&call.session_id)
        .bind(&call.trace_id)
        .bind(&call.span_id)
        .bind(&call.parent_span_id)
        .bind(&call.project_id)
        .bind(&call.user_id)
        .bind(&call.environment)
        .bind(&call.feature)
        .bind(call.input_tokens)
        .bind(call.output_tokens)
        .bind(call.cached_input_tokens)
        .bind(call.cache_write_input_tokens)
        .bind(call.reasoning_output_tokens)
        .bind(call.audio_input_tokens)
        .bind(call.audio_output_tokens)
        .bind(call.image_input_tokens)
        .bind(call.image_output_tokens)
        .bind(call.total_tokens)
        .bind(call.total_billable_tokens)
        .bind(call.request_count)
        .bind(call.tool_call_count)
        .bind(call.retry_count)
        .bind(call.latency_ms)
        .bind(call.http_status)
        .bind(&call.status)
        .bind(&call.error_type)
        .bind(&call.error_message)
        .bind(call.estimated_cost_usd)
        .bind(&call.cost_currency)
        .bind(call.provider_reported_cost_usd)
        .bind(call.reconciled_cost_usd)
        .bind(&call.cost_source)
        .bind(&call.usage_source)
        .bind(&call.raw_usage_json)
        .bind(&call.raw_response_json)
        .bind(&call.request_hash)
        .bind(&call.response_hash)
        .bind(&call.prompt_template_id)
        .bind(&call.created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_or_create_local_device_id(&self) -> Result<String, sqlx::Error> {
        if let Some(device_id) = self.app_setting_value("device_id").await? {
            if !device_id.trim().is_empty() {
                return Ok(device_id);
            }
        }

        let device_id = Uuid::new_v4().to_string();
        self.upsert_app_setting_value("device_id", &device_id, &Local::now().to_rfc3339())
            .await?;
        Ok(device_id)
    }

    pub async fn list_external_datasets(&self) -> Result<Vec<ExternalDataset>, sqlx::Error> {
        query_as::<_, ExternalDataset>(
            r#"
      SELECT
        id,
        device_id,
        device_name,
        package_version,
        source_path,
        imported_at,
        updated_at,
        calls,
        total_tokens,
        estimated_cost_usd,
        cost_currency
      FROM external_dataset
      ORDER BY updated_at DESC, device_name ASC
      "#,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn upsert_external_dataset(
        &self,
        input: &ExternalDatasetInput,
    ) -> Result<ExternalDataset, sqlx::Error> {
        query(
            r#"
      INSERT INTO external_dataset (
        id,
        device_id,
        device_name,
        package_version,
        source_path,
        imported_at,
        updated_at,
        calls,
        total_tokens,
        estimated_cost_usd,
        cost_currency
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
      ON CONFLICT(id) DO UPDATE SET
        device_id = excluded.device_id,
        device_name = excluded.device_name,
        package_version = excluded.package_version,
        source_path = excluded.source_path,
        imported_at = excluded.imported_at,
        updated_at = excluded.updated_at,
        calls = excluded.calls,
        total_tokens = excluded.total_tokens,
        estimated_cost_usd = excluded.estimated_cost_usd,
        cost_currency = excluded.cost_currency
      "#,
        )
        .bind(&input.id)
        .bind(&input.device_id)
        .bind(&input.device_name)
        .bind(input.package_version)
        .bind(&input.source_path)
        .bind(&input.imported_at)
        .bind(&input.updated_at)
        .bind(input.calls)
        .bind(input.total_tokens)
        .bind(input.estimated_cost_usd)
        .bind(&input.cost_currency)
        .execute(&self.pool)
        .await?;

        self.get_external_dataset(&input.id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn replace_external_dataset(
        &self,
        input: &ExternalDatasetInput,
        calls: &[ExternalDatasetImportCall],
    ) -> Result<ExternalDataset, sqlx::Error> {
        self.remove_external_dataset(&input.id).await?;
        let dataset = self.upsert_external_dataset(input).await?;
        for call in calls {
            self.insert_external_dataset_call(&input.id, call).await?;
        }

        Ok(dataset)
    }

    pub async fn remove_external_dataset(&self, dataset_id: &str) -> Result<i64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let deleted_calls = query("DELETE FROM llm_call WHERE origin_dataset_id = ?1")
            .bind(dataset_id)
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64;
        query("DELETE FROM agent_import_map WHERE dataset_id = ?1 OR source LIKE ?2")
            .bind(dataset_id)
            .bind(format!("external:{dataset_id}:%"))
            .execute(&mut *tx)
            .await?;
        query("DELETE FROM external_dataset WHERE id = ?1")
            .bind(dataset_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(deleted_calls)
    }

    async fn get_external_dataset(
        &self,
        dataset_id: &str,
    ) -> Result<Option<ExternalDataset>, sqlx::Error> {
        query_as::<_, ExternalDataset>(
            r#"
      SELECT
        id,
        device_id,
        device_name,
        package_version,
        source_path,
        imported_at,
        updated_at,
        calls,
        total_tokens,
        estimated_cost_usd,
        cost_currency
      FROM external_dataset
      WHERE id = ?1
      "#,
        )
        .bind(dataset_id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn insert_external_dataset_call(
        &self,
        dataset_id: &str,
        import_call: &ExternalDatasetImportCall,
    ) -> Result<(), sqlx::Error> {
        let source = external_dataset_source(dataset_id, &import_call.source_key);
        let existing_call_id: Option<String> = query(
            r#"
      SELECT llm_call_id
      FROM agent_import_map
      WHERE source = ?1 AND external_id = ?2
      "#,
        )
        .bind(&source)
        .bind(&import_call.external_id)
        .fetch_optional(&self.pool)
        .await?
        .and_then(|row| row.try_get("llm_call_id").ok());

        let mut call = import_call.call.clone();
        if let Some(existing_call_id) = existing_call_id {
            call.id = existing_call_id;
        }

        self.insert_llm_call(&call).await?;
        query("UPDATE llm_call SET origin_dataset_id = ?1 WHERE id = ?2")
            .bind(dataset_id)
            .bind(&call.id)
            .execute(&self.pool)
            .await?;
        query(
            r#"
      INSERT INTO agent_import_map (
        source,
        external_id,
        llm_call_id,
        imported_at,
        dataset_id
      ) VALUES (?1, ?2, ?3, ?4, ?5)
      ON CONFLICT(source, external_id) DO UPDATE SET
        llm_call_id = excluded.llm_call_id,
        imported_at = excluded.imported_at,
        dataset_id = excluded.dataset_id
      "#,
        )
        .bind(&source)
        .bind(&import_call.external_id)
        .bind(&call.id)
        .bind(Local::now().to_rfc3339())
        .bind(dataset_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn dashboard_summary(
        &self,
        from: &str,
        to: &str,
    ) -> Result<DashboardSummary, sqlx::Error> {
        let row = query(DASHBOARD_SUMMARY_SQL)
            .bind(from)
            .bind(to)
            .fetch_one(&self.pool)
            .await?;

        Ok(DashboardSummary {
            total_tokens: row.try_get("total_tokens")?,
            input_tokens: row.try_get("input_tokens")?,
            output_tokens: row.try_get("output_tokens")?,
            cached_input_tokens: row.try_get("cached_input_tokens")?,
            reasoning_output_tokens: row.try_get("reasoning_output_tokens")?,
            estimated_cost_usd: row.try_get("estimated_cost_usd")?,
            cost_currency: row.try_get("cost_currency")?,
            calls: row.try_get("calls")?,
            success_calls: row.try_get("success_calls")?,
            error_calls: row.try_get("error_calls")?,
            error_rate: row.try_get("error_rate")?,
            avg_latency_ms: row.try_get("avg_latency_ms")?,
            top_agent_id: row.try_get("top_agent_id")?,
            top_model: row.try_get("top_model")?,
        })
    }

    pub async fn dimension_summary(
        &self,
        from: &str,
        to: &str,
        dimension: &str,
        value: &str,
    ) -> Result<DashboardSummary, String> {
        let expression = dimension_expression(dimension)?;
        let sql = format!(
            r#"
      WITH filtered AS (
        SELECT *
        FROM llm_call
        WHERE date_local BETWEEN ?1 AND ?2
          AND {expression} = ?3
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
        CASE
          WHEN COUNT(*) = 0 THEN 0.0
          ELSE COALESCE(SUM(CASE WHEN status = 'error' THEN 1 ELSE 0 END), 0) * 1.0 / COUNT(*)
        END AS error_rate,
        AVG(latency_ms) AS avg_latency_ms,
        (SELECT agent_id FROM top_agent) AS top_agent_id,
        (SELECT model FROM top_model) AS top_model
      FROM filtered
      "#
        );

        let row = query(&sql)
            .bind(from)
            .bind(to)
            .bind(value)
            .fetch_one(&self.pool)
            .await
            .map_err(|err| err.to_string())?;

        Ok(DashboardSummary {
            total_tokens: row.try_get("total_tokens").map_err(|err| err.to_string())?,
            input_tokens: row.try_get("input_tokens").map_err(|err| err.to_string())?,
            output_tokens: row
                .try_get("output_tokens")
                .map_err(|err| err.to_string())?,
            cached_input_tokens: row
                .try_get("cached_input_tokens")
                .map_err(|err| err.to_string())?,
            reasoning_output_tokens: row
                .try_get("reasoning_output_tokens")
                .map_err(|err| err.to_string())?,
            estimated_cost_usd: row
                .try_get("estimated_cost_usd")
                .map_err(|err| err.to_string())?,
            cost_currency: row
                .try_get("cost_currency")
                .map_err(|err| err.to_string())?,
            calls: row.try_get("calls").map_err(|err| err.to_string())?,
            success_calls: row
                .try_get("success_calls")
                .map_err(|err| err.to_string())?,
            error_calls: row.try_get("error_calls").map_err(|err| err.to_string())?,
            error_rate: row.try_get("error_rate").map_err(|err| err.to_string())?,
            avg_latency_ms: row
                .try_get("avg_latency_ms")
                .map_err(|err| err.to_string())?,
            top_agent_id: row.try_get("top_agent_id").map_err(|err| err.to_string())?,
            top_model: row.try_get("top_model").map_err(|err| err.to_string())?,
        })
    }

    pub async fn daily_usage_series(
        &self,
        from: &str,
        to: &str,
        group_by: Option<&str>,
    ) -> Result<Vec<DailyUsagePoint>, String> {
        let sql = match group_by {
            None => DAILY_USAGE_SERIES_TOTAL_SQL.to_string(),
            Some("provider") => daily_grouped_sql("provider"),
            Some("model") => daily_grouped_sql(
                "COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, ''), 'unknown')",
            ),
            Some("agent") => daily_grouped_sql("COALESCE(NULLIF(agent_id, ''), 'unknown')"),
            Some("workflow") => daily_grouped_sql("COALESCE(NULLIF(workflow_id, ''), 'unknown')"),
            Some("project") => daily_grouped_sql("COALESCE(NULLIF(project_id, ''), 'unknown')"),
            Some("session") => daily_grouped_sql("COALESCE(NULLIF(session_id, ''), 'unknown')"),
            Some(other) => return Err(format!("unsupported daily series group_by: {other}")),
        };

        query_as::<_, DailyUsagePointRow>(&sql)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pool)
            .await
            .map(|rows| rows.into_iter().map(Into::into).collect())
            .map_err(|err| err.to_string())
    }

    pub async fn dimension_daily_series(
        &self,
        from: &str,
        to: &str,
        dimension: &str,
        value: &str,
    ) -> Result<Vec<DailyUsagePoint>, String> {
        let expression = dimension_expression(dimension)?;
        let sql = format!(
            r#"
    SELECT
      date_local,
      {expression} AS dimension,
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
      AND {expression} = ?3
    GROUP BY date_local, dimension
    ORDER BY date_local ASC, dimension ASC
    "#
        );

        query_as::<_, DailyUsagePointRow>(&sql)
            .bind(from)
            .bind(to)
            .bind(value)
            .fetch_all(&self.pool)
            .await
            .map(|rows| rows.into_iter().map(Into::into).collect())
            .map_err(|err| err.to_string())
    }

    pub async fn top_agents(
        &self,
        from: &str,
        to: &str,
        limit: i64,
    ) -> Result<Vec<TopDimensionRow>, sqlx::Error> {
        self.top_dimensions(TOP_AGENTS_SQL, from, to, limit).await
    }

    pub async fn top_models(
        &self,
        from: &str,
        to: &str,
        limit: i64,
    ) -> Result<Vec<TopDimensionRow>, sqlx::Error> {
        self.top_dimensions(TOP_MODELS_SQL, from, to, limit).await
    }

    pub async fn top_providers(
        &self,
        from: &str,
        to: &str,
        limit: i64,
    ) -> Result<Vec<TopDimensionRow>, sqlx::Error> {
        self.top_dimensions(TOP_PROVIDERS_SQL, from, to, limit)
            .await
    }

    pub async fn top_workflows(
        &self,
        from: &str,
        to: &str,
        limit: i64,
    ) -> Result<Vec<TopDimensionRow>, sqlx::Error> {
        self.top_dimensions(TOP_WORKFLOWS_SQL, from, to, limit)
            .await
    }

    pub async fn top_projects(
        &self,
        from: &str,
        to: &str,
        limit: i64,
    ) -> Result<Vec<TopDimensionRow>, sqlx::Error> {
        self.top_dimensions(TOP_PROJECTS_SQL, from, to, limit).await
    }

    pub async fn top_sessions(
        &self,
        from: &str,
        to: &str,
        limit: i64,
    ) -> Result<Vec<TopDimensionRow>, sqlx::Error> {
        self.top_dimensions(TOP_SESSIONS_SQL, from, to, limit).await
    }

    pub async fn recent_calls(&self, limit: i64) -> Result<Vec<LlmCallRow>, sqlx::Error> {
        query_as::<_, LlmCallRowSql>(RECENT_CALLS_SQL)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map(|rows| rows.into_iter().map(Into::into).collect())
    }

    pub async fn list_llm_calls(
        &self,
        filters: &LlmCallFilters,
    ) -> Result<LlmCallPage, sqlx::Error> {
        let limit = normalize_page_limit(filters.limit);
        let offset = filters.offset.max(0);

        let mut count_builder = QueryBuilder::<Sqlite>::new(
            r#"
      SELECT COUNT(*) AS total
      FROM llm_call
      "#,
        );
        push_call_filters(&mut count_builder, filters);
        let total: i64 = count_builder
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await?;

        let mut rows_builder = QueryBuilder::<Sqlite>::new(
            r#"
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
      "#,
        );
        push_call_filters(&mut rows_builder, filters);
        rows_builder
            .push(" ORDER BY started_at DESC, id DESC LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);

        let rows = rows_builder
            .build_query_as::<LlmCallRowSql>()
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(Into::into)
            .collect();

        Ok(LlmCallPage { rows, total })
    }

    pub async fn call_filter_options(&self) -> Result<CallFilterOptions, sqlx::Error> {
        Ok(CallFilterOptions {
            providers: distinct_non_empty_values(&self.pool, "provider").await?,
            agents: distinct_non_empty_values(&self.pool, "agent_id").await?,
            models: distinct_non_empty_models(&self.pool).await?,
            statuses: distinct_non_empty_values(&self.pool, "status").await?,
        })
    }

    pub async fn agent_source_stats(&self) -> Result<Vec<AgentSourceStats>, sqlx::Error> {
        query_as::<_, AgentSourceStatsSql>(AGENT_SOURCE_STATS_SQL)
            .fetch_all(&self.pool)
            .await
            .map(|rows| rows.into_iter().map(Into::into).collect())
    }

    pub async fn list_custom_importer_profiles(
        &self,
    ) -> Result<Vec<CustomImporterProfile>, sqlx::Error> {
        query_as::<_, CustomImporterProfile>(
            r#"
      WITH source_stats AS (
        SELECT
          m.source AS source_key,
          COUNT(*) AS imported_calls,
          COALESCE(SUM(c.total_tokens), 0) AS total_tokens,
          COALESCE(SUM(c.estimated_cost_usd), 0.0) AS estimated_cost_usd,
          CASE
            WHEN COUNT(DISTINCT COALESCE(c.cost_currency, 'USD')) = 1 THEN COALESCE(MAX(c.cost_currency), 'USD')
            ELSE 'MIXED'
          END AS cost_currency,
          MAX(m.imported_at) AS last_imported_at,
          MAX(c.started_at) AS last_call_at
        FROM agent_import_map m
        JOIN llm_call c ON c.id = m.llm_call_id
        GROUP BY m.source
      ),
      latest_run AS (
        SELECT
          run.profile_id,
          run.status,
          run.error
        FROM custom_importer_run run
        JOIN (
          SELECT profile_id, MAX(started_at) AS started_at
          FROM custom_importer_run
          GROUP BY profile_id
        ) latest
          ON latest.profile_id = run.profile_id
         AND latest.started_at = run.started_at
      )
      SELECT
        p.id,
        p.name,
        p.enabled,
        p.source_key,
        p.database_path,
        p.import_sql,
        p.mappings_json,
        p.created_at,
        p.updated_at,
        COALESCE(s.imported_calls, 0) AS imported_calls,
        COALESCE(s.total_tokens, 0) AS total_tokens,
        COALESCE(s.estimated_cost_usd, 0.0) AS estimated_cost_usd,
        COALESCE(s.cost_currency, 'USD') AS cost_currency,
        s.last_imported_at,
        s.last_call_at,
        latest_run.status AS last_run_status,
        latest_run.error AS last_run_error
      FROM custom_importer_profile p
      LEFT JOIN source_stats s ON s.source_key = p.source_key
      LEFT JOIN latest_run ON latest_run.profile_id = p.id
      ORDER BY p.name ASC, p.id ASC
      "#,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_custom_importer_profile(
        &self,
        id: &str,
    ) -> Result<Option<CustomImporterProfile>, sqlx::Error> {
        let mut profiles = query_as::<_, CustomImporterProfile>(
            r#"
      WITH source_stats AS (
        SELECT
          m.source AS source_key,
          COUNT(*) AS imported_calls,
          COALESCE(SUM(c.total_tokens), 0) AS total_tokens,
          COALESCE(SUM(c.estimated_cost_usd), 0.0) AS estimated_cost_usd,
          CASE
            WHEN COUNT(DISTINCT COALESCE(c.cost_currency, 'USD')) = 1 THEN COALESCE(MAX(c.cost_currency), 'USD')
            ELSE 'MIXED'
          END AS cost_currency,
          MAX(m.imported_at) AS last_imported_at,
          MAX(c.started_at) AS last_call_at
        FROM agent_import_map m
        JOIN llm_call c ON c.id = m.llm_call_id
        GROUP BY m.source
      ),
      latest_run AS (
        SELECT
          run.profile_id,
          run.status,
          run.error
        FROM custom_importer_run run
        JOIN (
          SELECT profile_id, MAX(started_at) AS started_at
          FROM custom_importer_run
          GROUP BY profile_id
        ) latest
          ON latest.profile_id = run.profile_id
         AND latest.started_at = run.started_at
      )
      SELECT
        p.id,
        p.name,
        p.enabled,
        p.source_key,
        p.database_path,
        p.import_sql,
        p.mappings_json,
        p.created_at,
        p.updated_at,
        COALESCE(s.imported_calls, 0) AS imported_calls,
        COALESCE(s.total_tokens, 0) AS total_tokens,
        COALESCE(s.estimated_cost_usd, 0.0) AS estimated_cost_usd,
        COALESCE(s.cost_currency, 'USD') AS cost_currency,
        s.last_imported_at,
        s.last_call_at,
        latest_run.status AS last_run_status,
        latest_run.error AS last_run_error
      FROM custom_importer_profile p
      LEFT JOIN source_stats s ON s.source_key = p.source_key
      LEFT JOIN latest_run ON latest_run.profile_id = p.id
      WHERE p.id = ?1
      LIMIT 1
      "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        Ok(profiles.pop())
    }

    pub async fn upsert_custom_importer_profile(
        &self,
        input: &CustomImporterProfileInput,
    ) -> Result<CustomImporterProfile, sqlx::Error> {
        let id = input
            .id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("custom-importer-{}", Uuid::new_v4()));
        let now = Local::now().to_rfc3339();

        query(
            r#"
      INSERT INTO custom_importer_profile (
        id,
        name,
        enabled,
        source_key,
        database_path,
        import_sql,
        mappings_json,
        created_at,
        updated_at
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
      ON CONFLICT(id) DO UPDATE SET
        name = excluded.name,
        enabled = excluded.enabled,
        source_key = excluded.source_key,
        database_path = excluded.database_path,
        import_sql = excluded.import_sql,
        mappings_json = excluded.mappings_json,
        updated_at = excluded.updated_at
      "#,
        )
        .bind(&id)
        .bind(input.name.trim())
        .bind(input.enabled)
        .bind(input.source_key.trim())
        .bind(input.database_path.trim())
        .bind(input.import_sql.trim())
        .bind(input.mappings_json.trim())
        .bind(now)
        .execute(&self.pool)
        .await?;

        self.get_custom_importer_profile(&id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn delete_custom_importer_profile(&self, id: &str) -> Result<bool, sqlx::Error> {
        let result = query("DELETE FROM custom_importer_profile WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn record_custom_importer_run(
        &self,
        profile_id: &str,
        status: &str,
        imported: i64,
        skipped: i64,
        error: Option<&str>,
        started_at: &str,
    ) -> Result<CustomImporterRunResult, sqlx::Error> {
        let finished_at = Local::now().to_rfc3339();
        query(
            r#"
      INSERT INTO custom_importer_run (
        id,
        profile_id,
        status,
        imported,
        skipped,
        error,
        started_at,
        finished_at
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
      "#,
        )
        .bind(format!("custom-importer-run-{}", Uuid::new_v4()))
        .bind(profile_id)
        .bind(status)
        .bind(imported)
        .bind(skipped)
        .bind(error)
        .bind(started_at)
        .bind(finished_at)
        .execute(&self.pool)
        .await?;

        Ok(CustomImporterRunResult {
            profile_id: profile_id.to_string(),
            status: status.to_string(),
            imported,
            skipped,
            error: error.map(ToString::to_string),
        })
    }

    pub async fn data_health_summary(&self) -> Result<DataHealthSummary, sqlx::Error> {
        let total_calls: i64 = query("SELECT COUNT(*) FROM llm_call")
            .fetch_one(&self.pool)
            .await?
            .try_get(0)?;
        let issue_calls_sql = data_health_issue_calls_sql("COUNT(DISTINCT call_id)");
        let issue_calls: i64 = query(&issue_calls_sql)
            .fetch_one(&self.pool)
            .await?
            .try_get(0)?;
        let issue_counts_sql = data_health_issue_counts_sql();
        let issues = query_as::<_, DataHealthIssueSummarySql>(&issue_counts_sql)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(Into::into)
            .collect();

        Ok(DataHealthSummary {
            total_calls,
            issue_calls,
            issues,
        })
    }

    pub async fn list_data_health_issues(
        &self,
        filters: &LlmCallFilters,
    ) -> Result<Vec<DataHealthIssueRow>, sqlx::Error> {
        let limit = normalize_page_limit(filters.limit);
        let offset = filters.offset.max(0);
        let mut builder = QueryBuilder::<Sqlite>::new(
            r#"
      WITH filtered AS (
        SELECT *
        FROM llm_call
      "#,
        );
        push_call_filters(&mut builder, filters);
        builder.push(
            r#"
      ),
      issues AS (
      "#,
        );
        push_data_health_issue_selects(&mut builder, "filtered");
        builder.push(
            r#"
      )
      SELECT
        call_id,
        issue_type,
        started_at,
        date_local,
        provider,
        model,
        agent_id,
        workflow_id,
        project_id,
        session_id,
        status,
        total_tokens,
        estimated_cost_usd,
        cost_currency,
        cost_source
      FROM issues
      ORDER BY started_at DESC, call_id ASC, issue_type ASC
      LIMIT
      "#,
        );
        builder.push_bind(limit);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        builder
            .build_query_as::<DataHealthIssueRowSql>()
            .fetch_all(&self.pool)
            .await
            .map(|rows| rows.into_iter().map(Into::into).collect())
    }

    pub async fn seed_demo_data(&self) -> Result<(), sqlx::Error> {
        sqlx::raw_sql(SEED_DEMO_SQL).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn clear_demo_data(&self) -> Result<i64, sqlx::Error> {
        let deleted_calls = query(
            r#"
      DELETE FROM llm_call
      WHERE id LIKE 'demo-call-%'
         OR usage_source = 'demo_seed'
      "#,
        )
        .execute(&self.pool)
        .await?
        .rows_affected() as i64;

        query("DELETE FROM provider_config WHERE id = 'demo-openai-compatible'")
            .execute(&self.pool)
            .await?;
        query("DELETE FROM pricing_rule WHERE id LIKE 'demo-pricing-%' OR source = 'demo'")
            .execute(&self.pool)
            .await?;

        Ok(deleted_calls)
    }

    pub async fn list_provider_configs(&self) -> Result<Vec<ProviderConfig>, sqlx::Error> {
        query_as::<_, ProviderConfigSql>(
            r#"
      SELECT
        id,
        provider,
        display_name,
        base_url,
        api_key_ref,
        is_default,
        created_at,
        updated_at
      FROM provider_config
      ORDER BY is_default DESC, display_name ASC, provider ASC
      "#,
        )
        .fetch_all(&self.pool)
        .await
        .map(|rows| rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_provider_config(
        &self,
        id: &str,
    ) -> Result<Option<ProviderConfig>, sqlx::Error> {
        query_as::<_, ProviderConfigSql>(
            r#"
      SELECT
        id,
        provider,
        display_name,
        base_url,
        api_key_ref,
        is_default,
        created_at,
        updated_at
      FROM provider_config
      WHERE id = ?1
      "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map(|row| row.map(Into::into))
    }

    pub async fn upsert_provider_config(
        &self,
        input: &ProviderConfigInput,
    ) -> Result<ProviderConfig, sqlx::Error> {
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = Local::now().to_rfc3339();

        let existing = query(
            r#"
      SELECT created_at, api_key_ref
      FROM provider_config
      WHERE id = ?1
      "#,
        )
        .bind(&id)
        .fetch_optional(&self.pool)
        .await?;

        let created_at = existing
            .as_ref()
            .and_then(|row| row.try_get::<String, _>("created_at").ok())
            .unwrap_or_else(|| now.clone());
        let api_key_ref = input
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(redact_secret)
            .or_else(|| {
                existing
                    .as_ref()
                    .and_then(|row| row.try_get::<Option<String>, _>("api_key_ref").ok())
                    .flatten()
            });

        if input.is_default {
            query("UPDATE provider_config SET is_default = 0, updated_at = ?1 WHERE id <> ?2")
                .bind(&now)
                .bind(&id)
                .execute(&self.pool)
                .await?;
        }

        query(
            r#"
      INSERT INTO provider_config (
        id,
        provider,
        display_name,
        base_url,
        api_key_ref,
        is_default,
        created_at,
        updated_at
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
      ON CONFLICT(id) DO UPDATE SET
        provider = excluded.provider,
        display_name = excluded.display_name,
        base_url = excluded.base_url,
        api_key_ref = excluded.api_key_ref,
        is_default = excluded.is_default,
        updated_at = excluded.updated_at
      "#,
        )
        .bind(&id)
        .bind(&input.provider)
        .bind(&input.display_name)
        .bind(&input.base_url)
        .bind(&api_key_ref)
        .bind(if input.is_default { 1_i64 } else { 0_i64 })
        .bind(&created_at)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        self.get_provider_config(&id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn delete_provider_config(&self, id: &str) -> Result<bool, sqlx::Error> {
        let result = query("DELETE FROM provider_config WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn get_app_settings(&self) -> Result<AppSettings, sqlx::Error> {
        let proxy_port = query("SELECT value FROM app_setting WHERE key = 'proxy_port'")
            .fetch_optional(&self.pool)
            .await?
            .and_then(|row| row.try_get::<String, _>("value").ok())
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(14567);
        let debug_capture_enabled =
            query("SELECT value FROM app_setting WHERE key = 'debug_capture_enabled'")
                .fetch_optional(&self.pool)
                .await?
                .and_then(|row| row.try_get::<String, _>("value").ok())
                .map(|value| value == "true")
                .unwrap_or(false);

        Ok(AppSettings {
            proxy_port,
            debug_capture_enabled,
        })
    }

    pub async fn upsert_app_settings(
        &self,
        input: &AppSettingsInput,
    ) -> Result<AppSettings, sqlx::Error> {
        let now = Local::now().to_rfc3339();
        self.upsert_app_setting_value("proxy_port", &input.proxy_port.to_string(), &now)
            .await?;
        self.upsert_app_setting_value(
            "debug_capture_enabled",
            if input.debug_capture_enabled {
                "true"
            } else {
                "false"
            },
            &now,
        )
        .await?;

        self.get_app_settings().await
    }

    pub async fn get_sync_settings(&self) -> Result<SyncSettings, sqlx::Error> {
        let enabled = self
            .app_setting_value("background_sync_enabled")
            .await?
            .map(|value| value == "true")
            .unwrap_or(true);
        let interval_minutes = self
            .app_setting_value("background_sync_interval_minutes")
            .await?
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(30)
            .clamp(5, 1440);
        let sync_on_startup = self
            .app_setting_value("background_sync_on_launch")
            .await?
            .map(|value| value == "true")
            .unwrap_or(true);
        let last_sync_at = self
            .app_setting_value("background_sync_last_finished_at")
            .await?;
        let last_status = self
            .app_setting_value("background_sync_last_status")
            .await?;
        let last_message = self
            .app_setting_value("background_sync_last_message")
            .await?;
        let next_run_at = if enabled {
            next_sync_run_at(last_sync_at.as_deref(), interval_minutes)
        } else {
            None
        };
        let last_error = if last_status.as_deref() == Some("error") {
            last_message.clone()
        } else {
            None
        };
        let last_result = match last_status.as_deref() {
            Some("success") | Some("busy") => last_message.clone(),
            Some("error") => Some("同步失败。".to_string()),
            _ => None,
        };

        Ok(SyncSettings {
            enabled,
            interval_minutes,
            sync_on_startup,
            last_sync_at,
            next_sync_at: next_run_at,
            last_result,
            last_error,
        })
    }

    pub async fn save_sync_settings(
        &self,
        input: &SyncSettingsInput,
    ) -> Result<SyncSettings, sqlx::Error> {
        let now = Local::now().to_rfc3339();
        let interval_minutes = input.interval_minutes.clamp(5, 1440);
        self.upsert_app_setting_value(
            "background_sync_enabled",
            if input.enabled { "true" } else { "false" },
            &now,
        )
        .await?;
        self.upsert_app_setting_value(
            "background_sync_interval_minutes",
            &interval_minutes.to_string(),
            &now,
        )
        .await?;
        self.upsert_app_setting_value(
            "background_sync_on_launch",
            if input.sync_on_startup {
                "true"
            } else {
                "false"
            },
            &now,
        )
        .await?;

        self.get_sync_settings().await
    }

    pub async fn record_sync_run(
        &self,
        started_at: &str,
        finished_at: &str,
        status: &str,
        message: &str,
        imported: i64,
        skipped: i64,
    ) -> Result<(), sqlx::Error> {
        let now = Local::now().to_rfc3339();
        let mut tx = self.pool.begin().await?;
        for (key, value) in [
            ("background_sync_last_started_at", started_at.to_string()),
            ("background_sync_last_finished_at", finished_at.to_string()),
            ("background_sync_last_status", status.to_string()),
            ("background_sync_last_message", message.to_string()),
            ("background_sync_last_imported", imported.to_string()),
            ("background_sync_last_skipped", skipped.to_string()),
        ] {
            query(
                r#"
          INSERT INTO app_setting (key, value, updated_at)
          VALUES (?1, ?2, ?3)
          ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
          "#,
            )
            .bind(key)
            .bind(value)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        Ok(())
    }

    pub async fn import_cursor(&self, source_id: &str) -> Result<Option<String>, sqlx::Error> {
        self.app_setting_value(&import_cursor_key(source_id)).await
    }

    pub async fn save_import_cursor(
        &self,
        source_id: &str,
        cursor_at: &str,
    ) -> Result<(), sqlx::Error> {
        self.upsert_app_setting_value(
            &import_cursor_key(source_id),
            cursor_at,
            &Local::now().to_rfc3339(),
        )
        .await
    }

    pub async fn export_llm_calls_csv(
        &self,
        filters: &LlmCallFilters,
    ) -> Result<String, sqlx::Error> {
        let mut builder = QueryBuilder::<Sqlite>::new(
            r#"
      SELECT
        id,
        started_at,
        ended_at,
        date_local,
        provider,
        provider_config_id,
        api_type,
        model_requested,
        model_response,
        agent_id,
        agent_name,
        agent_run_id,
        workflow_id,
        workflow_step,
        session_id,
        trace_id,
        span_id,
        parent_span_id,
        project_id,
        user_id,
        environment,
        feature,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        audio_input_tokens,
        audio_output_tokens,
        image_input_tokens,
        image_output_tokens,
        total_tokens,
        total_billable_tokens,
        request_count,
        tool_call_count,
        retry_count,
        latency_ms,
        http_status,
        status,
        error_type,
        error_message,
        usage_source,
        request_hash,
        response_hash,
        prompt_template_id,
        created_at
      FROM llm_call
      "#,
        );
        push_call_filters(&mut builder, filters);
        builder.push(" ORDER BY started_at DESC, id DESC");

        let rows = builder
            .build_query_as::<ExportLlmCallRow>()
            .fetch_all(&self.pool)
            .await?;

        Ok(render_llm_calls_csv(&rows))
    }

    pub async fn find_pricing_rule(
        &self,
        provider: &str,
        model: &str,
        at_date: &str,
    ) -> Result<Option<PricingRule>, sqlx::Error> {
        query_as::<_, PricingRule>(
            r#"
      SELECT
        id,
        provider,
        model,
        currency,
        input_usd_per_1m,
        cached_input_usd_per_1m,
        output_usd_per_1m,
        reasoning_output_usd_per_1m,
        effective_from,
        effective_to,
        source
      FROM pricing_rule
      WHERE provider = ?1
        AND model = ?2
        AND effective_from <= ?3
        AND (effective_to IS NULL OR effective_to > ?3)
      ORDER BY effective_from DESC, updated_at DESC, id ASC
      LIMIT 1
      "#,
        )
        .bind(provider)
        .bind(model)
        .bind(at_date)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn list_pricing_rules(&self) -> Result<Vec<PricingRule>, sqlx::Error> {
        query_as::<_, PricingRule>(
            r#"
      SELECT
        id,
        provider,
        model,
        currency,
        input_usd_per_1m,
        cached_input_usd_per_1m,
        output_usd_per_1m,
        reasoning_output_usd_per_1m,
        effective_from,
        effective_to,
        source
      FROM pricing_rule
      ORDER BY provider ASC, model ASC, effective_from DESC, id ASC
      "#,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_pricing_rule(&self, id: &str) -> Result<Option<PricingRule>, sqlx::Error> {
        query_as::<_, PricingRule>(
            r#"
      SELECT
        id,
        provider,
        model,
        currency,
        input_usd_per_1m,
        cached_input_usd_per_1m,
        output_usd_per_1m,
        reasoning_output_usd_per_1m,
        effective_from,
        effective_to,
        source
      FROM pricing_rule
      WHERE id = ?1
      "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn pricing_rule_exists(&self, id: &str) -> Result<bool, sqlx::Error> {
        query("SELECT 1 FROM pricing_rule WHERE id = ?1 LIMIT 1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map(|row| row.is_some())
    }

    pub async fn upsert_pricing_rule(
        &self,
        input: &PricingRuleInput,
    ) -> Result<PricingRule, sqlx::Error> {
        let id = input
            .id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let now = Local::now().to_rfc3339();
        let existing = query("SELECT created_at FROM pricing_rule WHERE id = ?1")
            .bind(&id)
            .fetch_optional(&self.pool)
            .await?;
        let created_at = existing
            .as_ref()
            .and_then(|row| row.try_get::<String, _>("created_at").ok())
            .unwrap_or_else(|| now.clone());

        query(
            r#"
      INSERT INTO pricing_rule (
        id,
        provider,
        model,
        currency,
        input_usd_per_1m,
        cached_input_usd_per_1m,
        output_usd_per_1m,
        reasoning_output_usd_per_1m,
        effective_from,
        effective_to,
        source,
        created_at,
        updated_at
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
      ON CONFLICT(id) DO UPDATE SET
        provider = excluded.provider,
        model = excluded.model,
        currency = excluded.currency,
        input_usd_per_1m = excluded.input_usd_per_1m,
        cached_input_usd_per_1m = excluded.cached_input_usd_per_1m,
        output_usd_per_1m = excluded.output_usd_per_1m,
        reasoning_output_usd_per_1m = excluded.reasoning_output_usd_per_1m,
        effective_from = excluded.effective_from,
        effective_to = excluded.effective_to,
        source = excluded.source,
        updated_at = excluded.updated_at
      "#,
        )
        .bind(&id)
        .bind(input.provider.trim())
        .bind(input.model.trim())
        .bind(normalize_currency(&input.currency))
        .bind(input.input_usd_per_1m)
        .bind(input.cached_input_usd_per_1m)
        .bind(input.output_usd_per_1m)
        .bind(input.reasoning_output_usd_per_1m)
        .bind(input.effective_from.trim())
        .bind(input.effective_to.as_deref().map(str::trim))
        .bind(input.source.as_deref().map(str::trim))
        .bind(&created_at)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        self.get_pricing_rule(&id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn import_pricing_rules(
        &self,
        rules: &[PricingRuleInput],
    ) -> Result<PricingRuleImportResult, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let now = Local::now().to_rfc3339();
        let mut imported = 0;
        let mut updated = 0;

        for input in rules {
            let id = input
                .id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let existing = query("SELECT created_at FROM pricing_rule WHERE id = ?1")
                .bind(&id)
                .fetch_optional(&mut *tx)
                .await?;
            let created_at = existing
                .as_ref()
                .and_then(|row| row.try_get::<String, _>("created_at").ok())
                .unwrap_or_else(|| now.clone());

            query(
                r#"
      INSERT INTO pricing_rule (
        id,
        provider,
        model,
        currency,
        input_usd_per_1m,
        cached_input_usd_per_1m,
        output_usd_per_1m,
        reasoning_output_usd_per_1m,
        effective_from,
        effective_to,
        source,
        created_at,
        updated_at
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
      ON CONFLICT(id) DO UPDATE SET
        provider = excluded.provider,
        model = excluded.model,
        currency = excluded.currency,
        input_usd_per_1m = excluded.input_usd_per_1m,
        cached_input_usd_per_1m = excluded.cached_input_usd_per_1m,
        output_usd_per_1m = excluded.output_usd_per_1m,
        reasoning_output_usd_per_1m = excluded.reasoning_output_usd_per_1m,
        effective_from = excluded.effective_from,
        effective_to = excluded.effective_to,
        source = excluded.source,
        updated_at = excluded.updated_at
      "#,
            )
            .bind(&id)
            .bind(input.provider.trim())
            .bind(input.model.trim())
            .bind(normalize_currency(&input.currency))
            .bind(input.input_usd_per_1m)
            .bind(input.cached_input_usd_per_1m)
            .bind(input.output_usd_per_1m)
            .bind(input.reasoning_output_usd_per_1m)
            .bind(input.effective_from.trim())
            .bind(input.effective_to.as_deref().map(str::trim))
            .bind(input.source.as_deref().map(str::trim))
            .bind(&created_at)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

            if existing.is_some() {
                updated += 1;
            } else {
                imported += 1;
            }
        }

        tx.commit().await?;

        Ok(PricingRuleImportResult {
            imported,
            updated,
            total: rules.len() as i64,
        })
    }

    pub async fn delete_pricing_rule(&self, id: &str) -> Result<bool, sqlx::Error> {
        let result = query("DELETE FROM pricing_rule WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn recalculate_estimated_costs(
        &self,
    ) -> Result<CostRecalculationResult, sqlx::Error> {
        let rows = query_as::<_, CostRecalculationCallSql>(
            r#"
      SELECT
        id,
        provider,
        COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) AS model,
        date_local,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_write_input_tokens,
        reasoning_output_tokens,
        audio_input_tokens,
        audio_output_tokens,
        image_input_tokens,
        image_output_tokens,
        total_tokens,
        total_billable_tokens
      FROM llm_call
      WHERE COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) IS NOT NULL
      "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut updated = 0;
        let mut missing = 0;

        for row in rows {
            let Some(model) = row.model.as_deref() else {
                continue;
            };
            let usage = NormalizedUsage {
                input_tokens: row.input_tokens,
                output_tokens: row.output_tokens,
                cached_input_tokens: row.cached_input_tokens,
                cache_write_input_tokens: row.cache_write_input_tokens,
                reasoning_output_tokens: row.reasoning_output_tokens,
                audio_input_tokens: row.audio_input_tokens,
                audio_output_tokens: row.audio_output_tokens,
                image_input_tokens: row.image_input_tokens,
                image_output_tokens: row.image_output_tokens,
                total_tokens: row.total_tokens,
                total_billable_tokens: row.total_billable_tokens,
                raw_usage_json: Value::Null,
                usage_source: UsageSource::Estimated,
            };
            let rule = self
                .find_pricing_rule(&row.provider, model, &row.date_local)
                .await?;
            let estimate = estimate_cost(&usage, rule.as_ref());
            if rule.is_none() {
                missing += 1;
            }

            let result = query(
                r#"
        UPDATE llm_call
        SET estimated_cost_usd = ?1,
            cost_currency = ?2,
            cost_source = ?3
        WHERE id = ?4
        "#,
            )
            .bind(estimate.estimated_cost_usd)
            .bind(estimate.cost_currency)
            .bind(estimate.cost_source)
            .bind(&row.id)
            .execute(&self.pool)
            .await?;
            updated += result.rows_affected() as i64;
        }

        Ok(CostRecalculationResult { updated, missing })
    }

    pub async fn list_unknown_pricing_models(
        &self,
    ) -> Result<Vec<UnknownPricingModel>, sqlx::Error> {
        query_as::<_, UnknownPricingModelSql>(
            r#"
      SELECT
        provider,
        COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) AS model,
        COUNT(*) AS calls,
        COALESCE(SUM(total_tokens), 0) AS total_tokens,
        COALESCE(SUM(estimated_cost_usd), 0.0) AS estimated_cost_usd,
        CASE
          WHEN COUNT(DISTINCT COALESCE(cost_currency, 'USD')) = 1 THEN COALESCE(MAX(cost_currency), 'USD')
          ELSE 'MIXED'
        END AS cost_currency,
        MIN(started_at) AS first_seen_at,
        MAX(started_at) AS last_seen_at
      FROM llm_call c
      WHERE COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) IS NOT NULL
        AND NOT EXISTS (
          SELECT 1
          FROM pricing_rule rule
          WHERE rule.provider = c.provider
            AND rule.model = COALESCE(NULLIF(c.model_response, ''), NULLIF(c.model_requested, ''))
            AND rule.effective_from <= c.date_local
            AND (rule.effective_to IS NULL OR rule.effective_to > c.date_local)
        )
      GROUP BY provider, model
      ORDER BY calls DESC, total_tokens DESC, provider ASC, model ASC
      "#,
        )
        .fetch_all(&self.pool)
        .await
        .map(|rows| rows.into_iter().map(Into::into).collect())
    }

    async fn top_dimensions(
        &self,
        sql: &str,
        from: &str,
        to: &str,
        limit: i64,
    ) -> Result<Vec<TopDimensionRow>, sqlx::Error> {
        query_as::<_, TopDimensionRowSql>(sql)
            .bind(from)
            .bind(to)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map(|rows| rows.into_iter().map(Into::into).collect())
    }

    async fn upsert_app_setting_value(
        &self,
        key: &str,
        value: &str,
        updated_at: &str,
    ) -> Result<(), sqlx::Error> {
        query(
            r#"
          INSERT INTO app_setting (key, value, updated_at)
          VALUES (?1, ?2, ?3)
          ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
          "#,
        )
        .bind(key)
        .bind(value)
        .bind(updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn app_setting_value(&self, key: &str) -> Result<Option<String>, sqlx::Error> {
        query("SELECT value FROM app_setting WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map(|row| row.and_then(|row| row.try_get::<String, _>("value").ok()))
    }
}

fn normalize_page_limit(limit: i64) -> i64 {
    limit.clamp(1, 100)
}

fn external_dataset_source(dataset_id: &str, source_key: &str) -> String {
    format!("external:{dataset_id}:{source_key}")
}

fn import_cursor_key(source_id: &str) -> String {
    format!("agent_import_cursor_{source_id}")
}

fn next_sync_run_at(last_finished_at: Option<&str>, interval_minutes: i64) -> Option<String> {
    let Some(last_finished_at) = last_finished_at else {
        return Some(Local::now().to_rfc3339());
    };
    let Ok(last_finished_at) = DateTime::parse_from_rfc3339(last_finished_at) else {
        return Some(Local::now().to_rfc3339());
    };

    Some(
        last_finished_at
            .with_timezone(&Local)
            .checked_add_signed(Duration::minutes(interval_minutes))
            .unwrap_or_else(Local::now)
            .to_rfc3339(),
    )
}

fn dimension_expression(dimension: &str) -> Result<&'static str, String> {
    match dimension {
        "provider" => Ok("provider"),
        "model" => Ok("COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, ''))"),
        "agent" => Ok("agent_id"),
        "workflow" => Ok("workflow_id"),
        "project" => Ok("project_id"),
        "session" => Ok("session_id"),
        other => Err(format!("unsupported dimension: {other}")),
    }
}

fn push_call_filters<'a>(builder: &mut QueryBuilder<'a, Sqlite>, filters: &'a LlmCallFilters) {
    let mut has_filter = false;

    if let Some(from) = non_empty(filters.from.as_deref()) {
        push_filter_prefix(builder, &mut has_filter);
        builder.push("date_local >= ").push_bind(from);
    }

    if let Some(to) = non_empty(filters.to.as_deref()) {
        push_filter_prefix(builder, &mut has_filter);
        builder.push("date_local <= ").push_bind(to);
    }

    if let Some(provider) = non_empty(filters.provider.as_deref()) {
        push_filter_prefix(builder, &mut has_filter);
        builder.push("provider = ").push_bind(provider);
    }

    if let Some(agent_id) = non_empty(filters.agent_id.as_deref()) {
        push_filter_prefix(builder, &mut has_filter);
        builder.push("agent_id = ").push_bind(agent_id);
    }

    if let Some(model) = non_empty(filters.model.as_deref()) {
        push_filter_prefix(builder, &mut has_filter);
        builder
            .push("COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) = ")
            .push_bind(model);
    }

    if let Some(status) = non_empty(filters.status.as_deref()) {
        push_filter_prefix(builder, &mut has_filter);
        builder.push("status = ").push_bind(status);
    }

    if let Some(workflow_id) = non_empty(filters.workflow_id.as_deref()) {
        push_filter_prefix(builder, &mut has_filter);
        builder.push("workflow_id = ").push_bind(workflow_id);
    }

    if let Some(project_id) = non_empty(filters.project_id.as_deref()) {
        push_filter_prefix(builder, &mut has_filter);
        builder.push("project_id = ").push_bind(project_id);
    }

    if let Some(session_id) = non_empty(filters.session_id.as_deref()) {
        push_filter_prefix(builder, &mut has_filter);
        builder.push("session_id = ").push_bind(session_id);
    }
}

fn push_filter_prefix(builder: &mut QueryBuilder<'_, Sqlite>, has_filter: &mut bool) {
    if *has_filter {
        builder.push(" AND ");
    } else {
        builder.push(" WHERE ");
        *has_filter = true;
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn data_health_issue_calls_sql(select_clause: &str) -> String {
    format!(
        r#"
      WITH issues AS (
        {}
      )
      SELECT {select_clause}
      FROM issues
      "#,
        data_health_issue_selects("llm_call")
    )
}

fn data_health_issue_counts_sql() -> String {
    format!(
        r#"
      WITH issues AS (
        {}
      )
      SELECT issue_type, COUNT(DISTINCT call_id) AS calls
      FROM issues
      GROUP BY issue_type
      ORDER BY issue_type ASC
      "#,
        data_health_issue_selects("llm_call")
    )
}

fn data_health_issue_selects(table: &str) -> String {
    let columns = format!(
        r#"
          id AS call_id,
          '{{issue_type}}' AS issue_type,
          started_at,
          date_local,
          provider,
          COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) AS model,
          agent_id,
          workflow_id,
          project_id,
          session_id,
          status,
          total_tokens,
          estimated_cost_usd,
          cost_currency,
          cost_source
        FROM {table}
        "#
    );
    format!(
        r#"
        SELECT {}
        WHERE COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) IS NULL
        UNION ALL
        SELECT {}
        WHERE total_tokens <= 0
        UNION ALL
        SELECT {}
        WHERE status <> 'success' OR COALESCE(http_status, 0) >= 400
      "#,
        columns.replace("{issue_type}", "missing_model"),
        columns.replace("{issue_type}", "missing_tokens"),
        columns.replace("{issue_type}", "failed_call"),
    )
}

fn push_data_health_issue_selects(builder: &mut QueryBuilder<'_, Sqlite>, table: &str) {
    let select = data_health_issue_selects(table);
    builder.push(select);
}

async fn distinct_non_empty_values(
    pool: &SqlitePool,
    column: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let sql = format!(
        r#"
      SELECT DISTINCT {column} AS value
      FROM llm_call
      WHERE {column} IS NOT NULL AND {column} <> ''
      ORDER BY value ASC
      "#
    );

    query_as::<_, DistinctValueRow>(&sql)
        .fetch_all(pool)
        .await
        .map(|rows| rows.into_iter().map(|row| row.value).collect())
}

async fn distinct_non_empty_models(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    query_as::<_, DistinctValueRow>(
        r#"
      SELECT DISTINCT COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) AS value
      FROM llm_call
      WHERE COALESCE(NULLIF(model_response, ''), NULLIF(model_requested, '')) IS NOT NULL
      ORDER BY value ASC
      "#,
    )
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(|row| row.value).collect())
}

fn daily_grouped_sql(dimension_expression: &str) -> String {
    format!(
        r#"
    SELECT
      date_local,
      {dimension_expression} AS dimension,
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
    GROUP BY date_local, dimension
    ORDER BY date_local ASC, dimension ASC
    "#
    )
}

#[derive(sqlx::FromRow)]
struct DistinctValueRow {
    value: String,
}

#[derive(sqlx::FromRow)]
struct CostRecalculationCallSql {
    id: String,
    provider: String,
    model: Option<String>,
    date_local: String,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    reasoning_output_tokens: i64,
    audio_input_tokens: i64,
    audio_output_tokens: i64,
    image_input_tokens: i64,
    image_output_tokens: i64,
    total_tokens: i64,
    total_billable_tokens: i64,
}

#[derive(sqlx::FromRow)]
struct DailyUsagePointRow {
    date_local: String,
    dimension: Option<String>,
    calls: i64,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    cost_currency: String,
}

impl From<DailyUsagePointRow> for DailyUsagePoint {
    fn from(row: DailyUsagePointRow) -> Self {
        Self {
            date_local: row.date_local,
            dimension: row.dimension,
            calls: row.calls,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            cached_input_tokens: row.cached_input_tokens,
            reasoning_output_tokens: row.reasoning_output_tokens,
            total_tokens: row.total_tokens,
            estimated_cost_usd: row.estimated_cost_usd,
            cost_currency: row.cost_currency,
        }
    }
}

#[derive(sqlx::FromRow)]
struct TopDimensionRowSql {
    dimension: String,
    calls: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    cost_currency: String,
    avg_latency_ms: Option<f64>,
}

impl From<TopDimensionRowSql> for TopDimensionRow {
    fn from(row: TopDimensionRowSql) -> Self {
        Self {
            dimension: row.dimension,
            calls: row.calls,
            total_tokens: row.total_tokens,
            estimated_cost_usd: row.estimated_cost_usd,
            cost_currency: row.cost_currency,
            avg_latency_ms: row.avg_latency_ms,
        }
    }
}

#[derive(sqlx::FromRow)]
struct LlmCallRowSql {
    id: String,
    started_at: String,
    provider: String,
    model_requested: Option<String>,
    model_response: Option<String>,
    agent_id: Option<String>,
    workflow_id: Option<String>,
    project_id: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    reasoning_output_tokens: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    cost_currency: String,
    latency_ms: Option<i64>,
    status: String,
}

impl From<LlmCallRowSql> for LlmCallRow {
    fn from(row: LlmCallRowSql) -> Self {
        Self {
            id: row.id,
            started_at: row.started_at,
            provider: row.provider,
            model_requested: row.model_requested,
            model_response: row.model_response,
            agent_id: row.agent_id,
            workflow_id: row.workflow_id,
            project_id: row.project_id,
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            cached_input_tokens: row.cached_input_tokens,
            reasoning_output_tokens: row.reasoning_output_tokens,
            total_tokens: row.total_tokens,
            estimated_cost_usd: row.estimated_cost_usd,
            cost_currency: row.cost_currency,
            latency_ms: row.latency_ms,
            status: row.status,
        }
    }
}

#[derive(sqlx::FromRow)]
struct AgentSourceStatsSql {
    source_key: String,
    imported_calls: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    cost_currency: String,
    last_imported_at: Option<String>,
    last_call_at: Option<String>,
}

#[derive(sqlx::FromRow)]
struct DataHealthIssueSummarySql {
    issue_type: String,
    calls: i64,
}

impl From<DataHealthIssueSummarySql> for DataHealthIssueSummary {
    fn from(row: DataHealthIssueSummarySql) -> Self {
        Self {
            issue_type: row.issue_type,
            calls: row.calls,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DataHealthIssueRowSql {
    call_id: String,
    issue_type: String,
    started_at: String,
    date_local: String,
    provider: String,
    model: Option<String>,
    agent_id: Option<String>,
    workflow_id: Option<String>,
    project_id: Option<String>,
    session_id: Option<String>,
    status: String,
    total_tokens: i64,
    estimated_cost_usd: f64,
    cost_currency: String,
    cost_source: Option<String>,
}

impl From<DataHealthIssueRowSql> for DataHealthIssueRow {
    fn from(row: DataHealthIssueRowSql) -> Self {
        Self {
            call_id: row.call_id,
            issue_type: row.issue_type,
            started_at: row.started_at,
            date_local: row.date_local,
            provider: row.provider,
            model: row.model,
            agent_id: row.agent_id,
            workflow_id: row.workflow_id,
            project_id: row.project_id,
            session_id: row.session_id,
            status: row.status,
            total_tokens: row.total_tokens,
            estimated_cost_usd: row.estimated_cost_usd,
            cost_currency: row.cost_currency,
            cost_source: row.cost_source,
        }
    }
}

#[derive(sqlx::FromRow)]
struct UnknownPricingModelSql {
    provider: String,
    model: String,
    calls: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
    cost_currency: String,
    first_seen_at: String,
    last_seen_at: String,
}

impl From<UnknownPricingModelSql> for UnknownPricingModel {
    fn from(row: UnknownPricingModelSql) -> Self {
        Self {
            provider: row.provider,
            model: row.model,
            calls: row.calls,
            total_tokens: row.total_tokens,
            estimated_cost_usd: row.estimated_cost_usd,
            cost_currency: row.cost_currency,
            first_seen_at: row.first_seen_at,
            last_seen_at: row.last_seen_at,
        }
    }
}

impl From<AgentSourceStatsSql> for AgentSourceStats {
    fn from(row: AgentSourceStatsSql) -> Self {
        Self {
            source_key: row.source_key,
            imported_calls: row.imported_calls,
            total_tokens: row.total_tokens,
            estimated_cost_usd: row.estimated_cost_usd,
            cost_currency: row.cost_currency,
            last_imported_at: row.last_imported_at,
            last_call_at: row.last_call_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ProviderConfigSql {
    id: String,
    provider: String,
    display_name: String,
    base_url: String,
    api_key_ref: Option<String>,
    is_default: i64,
    created_at: String,
    updated_at: String,
}

impl From<ProviderConfigSql> for ProviderConfig {
    fn from(row: ProviderConfigSql) -> Self {
        Self {
            id: row.id,
            provider: row.provider,
            display_name: row.display_name,
            base_url: row.base_url,
            api_key_redacted: row.api_key_ref,
            is_default: row.is_default != 0,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ExportLlmCallRow {
    id: String,
    started_at: String,
    ended_at: Option<String>,
    date_local: String,
    provider: String,
    provider_config_id: Option<String>,
    api_type: Option<String>,
    model_requested: Option<String>,
    model_response: Option<String>,
    agent_id: Option<String>,
    agent_name: Option<String>,
    agent_run_id: Option<String>,
    workflow_id: Option<String>,
    workflow_step: Option<String>,
    session_id: Option<String>,
    trace_id: Option<String>,
    span_id: Option<String>,
    parent_span_id: Option<String>,
    project_id: Option<String>,
    user_id: Option<String>,
    environment: Option<String>,
    feature: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    cache_write_input_tokens: i64,
    reasoning_output_tokens: i64,
    audio_input_tokens: i64,
    audio_output_tokens: i64,
    image_input_tokens: i64,
    image_output_tokens: i64,
    total_tokens: i64,
    total_billable_tokens: i64,
    request_count: i64,
    tool_call_count: i64,
    retry_count: i64,
    latency_ms: Option<i64>,
    http_status: Option<i64>,
    status: String,
    error_type: Option<String>,
    error_message: Option<String>,
    usage_source: Option<String>,
    request_hash: Option<String>,
    response_hash: Option<String>,
    prompt_template_id: Option<String>,
    created_at: String,
}

const CSV_HEADERS: &[&str] = &[
    "id",
    "started_at",
    "ended_at",
    "date_local",
    "provider",
    "provider_config_id",
    "api_type",
    "model_requested",
    "model_response",
    "agent_id",
    "agent_name",
    "agent_run_id",
    "workflow_id",
    "workflow_step",
    "session_id",
    "trace_id",
    "span_id",
    "parent_span_id",
    "project_id",
    "user_id",
    "environment",
    "feature",
    "input_tokens",
    "output_tokens",
    "cached_input_tokens",
    "cache_write_input_tokens",
    "reasoning_output_tokens",
    "audio_input_tokens",
    "audio_output_tokens",
    "image_input_tokens",
    "image_output_tokens",
    "total_tokens",
    "total_billable_tokens",
    "request_count",
    "tool_call_count",
    "retry_count",
    "latency_ms",
    "http_status",
    "status",
    "error_type",
    "error_message",
    "usage_source",
    "request_hash",
    "response_hash",
    "prompt_template_id",
    "created_at",
];

fn render_llm_calls_csv(rows: &[ExportLlmCallRow]) -> String {
    let mut csv = String::new();
    csv.push_str(&CSV_HEADERS.join(","));
    csv.push('\n');

    for row in rows {
        let values = [
            row.id.clone(),
            row.started_at.clone(),
            row.ended_at.clone().unwrap_or_default(),
            row.date_local.clone(),
            row.provider.clone(),
            row.provider_config_id.clone().unwrap_or_default(),
            row.api_type.clone().unwrap_or_default(),
            row.model_requested.clone().unwrap_or_default(),
            row.model_response.clone().unwrap_or_default(),
            row.agent_id.clone().unwrap_or_default(),
            row.agent_name.clone().unwrap_or_default(),
            row.agent_run_id.clone().unwrap_or_default(),
            row.workflow_id.clone().unwrap_or_default(),
            row.workflow_step.clone().unwrap_or_default(),
            row.session_id.clone().unwrap_or_default(),
            row.trace_id.clone().unwrap_or_default(),
            row.span_id.clone().unwrap_or_default(),
            row.parent_span_id.clone().unwrap_or_default(),
            row.project_id.clone().unwrap_or_default(),
            row.user_id.clone().unwrap_or_default(),
            row.environment.clone().unwrap_or_default(),
            row.feature.clone().unwrap_or_default(),
            row.input_tokens.to_string(),
            row.output_tokens.to_string(),
            row.cached_input_tokens.to_string(),
            row.cache_write_input_tokens.to_string(),
            row.reasoning_output_tokens.to_string(),
            row.audio_input_tokens.to_string(),
            row.audio_output_tokens.to_string(),
            row.image_input_tokens.to_string(),
            row.image_output_tokens.to_string(),
            row.total_tokens.to_string(),
            row.total_billable_tokens.to_string(),
            row.request_count.to_string(),
            row.tool_call_count.to_string(),
            row.retry_count.to_string(),
            row.latency_ms
                .map(|value| value.to_string())
                .unwrap_or_default(),
            row.http_status
                .map(|value| value.to_string())
                .unwrap_or_default(),
            row.status.clone(),
            row.error_type.clone().unwrap_or_default(),
            row.error_message.clone().unwrap_or_default(),
            row.usage_source.clone().unwrap_or_default(),
            row.request_hash.clone().unwrap_or_default(),
            row.response_hash.clone().unwrap_or_default(),
            row.prompt_template_id.clone().unwrap_or_default(),
            row.created_at.clone(),
        ];
        csv.push_str(
            &values
                .iter()
                .map(|value| csv_escape(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push('\n');
    }

    csv
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn redact_secret(secret: &str) -> String {
    if secret.len() <= 8 {
        return "********".to_string();
    }

    let prefix: String = secret.chars().take(4).collect();
    let suffix: String = secret
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::{AppSettingsInput, LlmCallFilters, ProviderConfigInput, TokenScopeRepository};
    use crate::pricing::PricingRuleInput;
    use sqlx::{query, Row};

    #[tokio::test]
    async fn dashboard_summary_decodes_empty_database_with_real_defaults() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        let summary = repository
            .dashboard_summary("1970-01-01", "1970-01-01")
            .await
            .expect("empty summary decodes");

        assert_eq!(summary.calls, 0);
        assert_eq!(summary.estimated_cost_usd, 0.0);
        assert_eq!(summary.error_rate, 0.0);
        assert_eq!(summary.avg_latency_ms, None);
    }

    #[tokio::test]
    async fn sync_cursor_round_trips_per_agent_source() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        assert_eq!(
            repository
                .import_cursor("codex")
                .await
                .expect("missing cursor reads"),
            None
        );

        repository
            .save_import_cursor("codex", "2026-06-01T10:00:00+08:00")
            .await
            .expect("codex cursor saved");
        repository
            .save_import_cursor("opencode", "2026-06-01T11:00:00+08:00")
            .await
            .expect("opencode cursor saved");

        assert_eq!(
            repository
                .import_cursor("codex")
                .await
                .expect("codex cursor reads")
                .as_deref(),
            Some("2026-06-01T10:00:00+08:00")
        );
        assert_eq!(
            repository
                .import_cursor("opencode")
                .await
                .expect("opencode cursor reads")
                .as_deref(),
            Some("2026-06-01T11:00:00+08:00")
        );
    }

    #[tokio::test]
    async fn sync_settings_default_to_launch_incremental_sync() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        let settings = repository
            .get_sync_settings()
            .await
            .expect("sync settings read");

        assert!(settings.enabled);
        assert!(settings.sync_on_startup);
    }

    #[tokio::test]
    async fn external_dataset_removal_keeps_local_calls() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        insert_minimal_call(
            &repository,
            "local-call",
            "codex",
            "2026-05-30T10:00:00+08:00",
            100,
            0.01,
        )
        .await;
        insert_external_dataset_row(&repository, "dataset-b").await;
        insert_external_dataset_call(
            &repository,
            "dataset-b-call",
            "dataset-b",
            "2026-05-30T11:00:00+08:00",
            200,
            0.02,
        )
        .await;

        let removed = repository
            .remove_external_dataset("dataset-b")
            .await
            .expect("dataset removed");

        assert_eq!(removed, 1);
        let local_calls: i64 = query("SELECT COUNT(*) FROM llm_call WHERE id = 'local-call'")
            .fetch_one(repository.pool())
            .await
            .expect("local call count")
            .try_get(0)
            .expect("count decodes");
        let external_calls: i64 =
            query("SELECT COUNT(*) FROM llm_call WHERE origin_dataset_id = 'dataset-b'")
                .fetch_one(repository.pool())
                .await
                .expect("external call count")
                .try_get(0)
                .expect("count decodes");
        let datasets: i64 = query("SELECT COUNT(*) FROM external_dataset WHERE id = 'dataset-b'")
            .fetch_one(repository.pool())
            .await
            .expect("dataset count")
            .try_get(0)
            .expect("count decodes");

        assert_eq!(local_calls, 1);
        assert_eq!(external_calls, 0);
        assert_eq!(datasets, 0);
    }

    #[tokio::test]
    async fn clear_demo_data_removes_seed_calls_without_touching_real_calls() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");
        repository.seed_demo_data().await.expect("demo data seeded");
        insert_minimal_call(
            &repository,
            "real-codex-call",
            "codex",
            "2026-05-30T10:00:00+08:00",
            100,
            0.0,
        )
        .await;

        let removed = repository
            .clear_demo_data()
            .await
            .expect("demo data cleared");

        let demo_calls: i64 = query(
            "SELECT COUNT(*) FROM llm_call WHERE id LIKE 'demo-call-%' OR usage_source = 'demo_seed'",
        )
        .fetch_one(repository.pool())
        .await
        .expect("demo call count")
        .try_get(0)
        .expect("count decodes");
        let real_calls: i64 = query("SELECT COUNT(*) FROM llm_call WHERE id = 'real-codex-call'")
            .fetch_one(repository.pool())
            .await
            .expect("real call count")
            .try_get(0)
            .expect("count decodes");

        assert_eq!(removed, 5);
        assert_eq!(demo_calls, 0);
        assert_eq!(real_calls, 1);
    }

    #[tokio::test]
    async fn find_pricing_rule_returns_latest_effective_rule() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");
        insert_pricing_rule(
            &repository,
            "rule-old",
            "openai-compatible",
            "gpt-5-mini",
            "USD",
            0.25,
            "2026-01-01",
            None,
        )
        .await;
        insert_pricing_rule(
            &repository,
            "rule-new",
            "openai-compatible",
            "gpt-5-mini",
            "USD",
            0.35,
            "2026-05-01",
            None,
        )
        .await;

        let rule = repository
            .find_pricing_rule("openai-compatible", "gpt-5-mini", "2026-05-29")
            .await
            .expect("lookup succeeds")
            .expect("effective rule exists");

        assert_eq!(rule.input_usd_per_1m, 0.35);
        assert_eq!(rule.effective_from, "2026-05-01");
    }

    #[tokio::test]
    async fn find_pricing_rule_treats_effective_to_as_exclusive() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");
        insert_pricing_rule(
            &repository,
            "rule-expired",
            "openai-compatible",
            "gpt-5-mini",
            "USD",
            0.25,
            "2026-01-01",
            Some("2026-05-29"),
        )
        .await;

        let rule = repository
            .find_pricing_rule("openai-compatible", "gpt-5-mini", "2026-05-29")
            .await
            .expect("lookup succeeds");

        assert!(rule.is_none());
    }

    #[tokio::test]
    async fn find_pricing_rule_returns_none_for_missing_model() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        let rule = repository
            .find_pricing_rule("openai-compatible", "unknown-model", "2026-05-29")
            .await
            .expect("lookup succeeds");

        assert!(rule.is_none());
    }

    #[tokio::test]
    async fn agent_source_stats_groups_import_map_sources_for_management_page() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        insert_minimal_call(
            &repository,
            "codex-call-1",
            "codex",
            "2026-05-29T10:00:00+08:00",
            1200,
            0.0,
        )
        .await;
        insert_minimal_call(
            &repository,
            "codex-call-2",
            "codex",
            "2026-05-30T11:00:00+08:00",
            800,
            0.0,
        )
        .await;
        insert_minimal_call(
            &repository,
            "hermes-call-1",
            "hermes",
            "2026-05-30T12:00:00+08:00",
            500,
            0.25,
        )
        .await;
        insert_import_map(
            &repository,
            "codex_state_threads",
            "thread-1",
            "codex-call-1",
            "2026-05-30T12:30:00+08:00",
        )
        .await;
        insert_import_map(
            &repository,
            "codex_state_threads",
            "thread-2",
            "codex-call-2",
            "2026-05-30T12:35:00+08:00",
        )
        .await;
        insert_import_map(
            &repository,
            "hermes_state_sessions",
            "session-1",
            "hermes-call-1",
            "2026-05-30T12:40:00+08:00",
        )
        .await;

        let stats = repository
            .agent_source_stats()
            .await
            .expect("source stats query succeeds");

        let codex = stats
            .iter()
            .find(|row| row.source_key == "codex_state_threads")
            .expect("codex stats exist");
        assert_eq!(codex.imported_calls, 2);
        assert_eq!(codex.total_tokens, 2000);
        assert_eq!(codex.estimated_cost_usd, 0.0);
        assert_eq!(
            codex.last_imported_at.as_deref(),
            Some("2026-05-30T12:35:00+08:00")
        );
        assert_eq!(
            codex.last_call_at.as_deref(),
            Some("2026-05-30T11:00:00+08:00")
        );

        let hermes = stats
            .iter()
            .find(|row| row.source_key == "hermes_state_sessions")
            .expect("hermes stats exist");
        assert_eq!(hermes.imported_calls, 1);
        assert_eq!(hermes.total_tokens, 500);
        assert_eq!(hermes.estimated_cost_usd, 0.25);
    }

    #[tokio::test]
    async fn list_llm_calls_filters_counts_and_paginates_results() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        insert_filterable_call(
            &repository,
            "match-newer",
            "codex",
            "worker",
            "gpt-5",
            "success",
            "2026-05-30T12:00:00+08:00",
            "2026-05-30",
        )
        .await;
        insert_filterable_call(
            &repository,
            "match-older",
            "codex",
            "worker",
            "gpt-5",
            "success",
            "2026-05-29T12:00:00+08:00",
            "2026-05-29",
        )
        .await;
        insert_filterable_call(
            &repository,
            "wrong-provider",
            "hermes",
            "worker",
            "gpt-5",
            "success",
            "2026-05-30T13:00:00+08:00",
            "2026-05-30",
        )
        .await;
        insert_filterable_call(
            &repository,
            "wrong-agent",
            "codex",
            "planner",
            "gpt-5",
            "success",
            "2026-05-30T14:00:00+08:00",
            "2026-05-30",
        )
        .await;
        insert_filterable_call(
            &repository,
            "wrong-model",
            "codex",
            "worker",
            "gpt-4",
            "success",
            "2026-05-30T15:00:00+08:00",
            "2026-05-30",
        )
        .await;
        insert_filterable_call(
            &repository,
            "wrong-status",
            "codex",
            "worker",
            "gpt-5",
            "error",
            "2026-05-30T16:00:00+08:00",
            "2026-05-30",
        )
        .await;
        insert_filterable_call(
            &repository,
            "wrong-date",
            "codex",
            "worker",
            "gpt-5",
            "success",
            "2026-04-30T12:00:00+08:00",
            "2026-04-30",
        )
        .await;

        let first_page = repository
            .list_llm_calls(&super::LlmCallFilters {
                from: Some("2026-05-01".to_string()),
                to: Some("2026-05-31".to_string()),
                provider: Some("codex".to_string()),
                agent_id: Some("worker".to_string()),
                model: Some("gpt-5".to_string()),
                status: Some("success".to_string()),
                workflow_id: None,
                project_id: None,
                session_id: None,
                limit: 1,
                offset: 0,
            })
            .await
            .expect("filtered calls query succeeds");

        assert_eq!(first_page.total, 2);
        assert_eq!(first_page.rows.len(), 1);
        assert_eq!(first_page.rows[0].id, "match-newer");

        let second_page = repository
            .list_llm_calls(&super::LlmCallFilters {
                from: Some("2026-05-01".to_string()),
                to: Some("2026-05-31".to_string()),
                provider: Some("codex".to_string()),
                agent_id: Some("worker".to_string()),
                model: Some("gpt-5".to_string()),
                status: Some("success".to_string()),
                workflow_id: None,
                project_id: None,
                session_id: None,
                limit: 1,
                offset: 1,
            })
            .await
            .expect("second page query succeeds");

        assert_eq!(second_page.total, 2);
        assert_eq!(second_page.rows.len(), 1);
        assert_eq!(second_page.rows[0].id, "match-older");
    }

    #[tokio::test]
    async fn call_filter_options_return_distinct_non_empty_values() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        insert_filterable_call(
            &repository,
            "option-a",
            "codex",
            "worker",
            "gpt-5",
            "success",
            "2026-05-30T12:00:00+08:00",
            "2026-05-30",
        )
        .await;
        insert_filterable_call(
            &repository,
            "option-b",
            "hermes",
            "planner",
            "gpt-4",
            "error",
            "2026-05-30T13:00:00+08:00",
            "2026-05-30",
        )
        .await;

        let options = repository
            .call_filter_options()
            .await
            .expect("filter options query succeeds");

        assert_eq!(options.providers, vec!["codex", "hermes"]);
        assert_eq!(options.agents, vec!["planner", "worker"]);
        assert_eq!(options.models, vec!["gpt-4", "gpt-5"]);
        assert_eq!(options.statuses, vec!["error", "success"]);
    }

    #[tokio::test]
    async fn dimension_summary_filters_by_selected_dimension() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        insert_filterable_call(
            &repository,
            "model-match-success",
            "codex",
            "worker",
            "gpt-5",
            "success",
            "2026-05-30T12:00:00+08:00",
            "2026-05-30",
        )
        .await;
        insert_filterable_call(
            &repository,
            "model-match-error",
            "hermes",
            "planner",
            "gpt-5",
            "error",
            "2026-05-30T13:00:00+08:00",
            "2026-05-30",
        )
        .await;
        insert_filterable_call(
            &repository,
            "model-ignored",
            "codex",
            "worker",
            "gpt-4",
            "success",
            "2026-05-30T14:00:00+08:00",
            "2026-05-30",
        )
        .await;

        let summary = repository
            .dimension_summary("2026-05-01", "2026-05-31", "model", "gpt-5")
            .await
            .expect("dimension summary query succeeds");

        assert_eq!(summary.calls, 2);
        assert_eq!(summary.success_calls, 1);
        assert_eq!(summary.error_calls, 1);
        assert_eq!(summary.total_tokens, 200);
        assert_eq!(summary.estimated_cost_usd, 0.02);
        assert_eq!(summary.error_rate, 0.5);
        assert_eq!(summary.top_model.as_deref(), Some("gpt-5"));
    }

    #[tokio::test]
    async fn dimension_daily_series_filters_and_groups_by_date() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        insert_filterable_call(
            &repository,
            "codex-day-one",
            "codex",
            "worker",
            "gpt-5",
            "success",
            "2026-05-29T12:00:00+08:00",
            "2026-05-29",
        )
        .await;
        insert_filterable_call(
            &repository,
            "codex-day-two",
            "codex",
            "planner",
            "gpt-4",
            "success",
            "2026-05-30T12:00:00+08:00",
            "2026-05-30",
        )
        .await;
        insert_filterable_call(
            &repository,
            "ignored-hermes",
            "hermes",
            "planner",
            "gpt-5",
            "success",
            "2026-05-30T13:00:00+08:00",
            "2026-05-30",
        )
        .await;

        let points = repository
            .dimension_daily_series("2026-05-01", "2026-05-31", "provider", "codex")
            .await
            .expect("dimension series query succeeds");

        assert_eq!(points.len(), 2);
        assert_eq!(points[0].date_local, "2026-05-29");
        assert_eq!(points[0].dimension.as_deref(), Some("codex"));
        assert_eq!(points[0].calls, 1);
        assert_eq!(points[0].total_tokens, 100);
        assert_eq!(points[1].date_local, "2026-05-30");
        assert_eq!(points[1].calls, 1);
        assert_eq!(points[1].total_tokens, 100);
    }

    #[tokio::test]
    async fn data_health_reports_summary_and_issue_rows_without_raw_payloads() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        insert_health_call(
            &repository,
            "missing-model",
            None,
            100,
            0.01,
            "success",
            None,
            None,
        )
        .await;
        insert_health_call(
            &repository,
            "missing-tokens",
            Some("gpt-5"),
            0,
            0.01,
            "success",
            None,
            None,
        )
        .await;
        insert_health_call(
            &repository,
            "missing-cost",
            Some("gpt-5"),
            100,
            0.0,
            "success",
            None,
            None,
        )
        .await;
        insert_health_call(
            &repository,
            "failed-call",
            Some("gpt-5"),
            100,
            0.01,
            "error",
            Some(500),
            None,
        )
        .await;
        insert_health_call(
            &repository,
            "missing-pricing-rule",
            Some("gpt-unknown"),
            100,
            0.0,
            "success",
            None,
            Some("missing_pricing_rule"),
        )
        .await;

        let summary = repository
            .data_health_summary()
            .await
            .expect("health summary query succeeds");
        let rows = repository
            .list_data_health_issues(&LlmCallFilters {
                from: None,
                to: None,
                provider: None,
                agent_id: None,
                model: None,
                status: None,
                workflow_id: None,
                project_id: None,
                session_id: None,
                limit: 100,
                offset: 0,
            })
            .await
            .expect("health issue query succeeds");

        assert_eq!(summary.total_calls, 5);
        assert_eq!(summary.issue_calls, 3);
        assert_eq!(
            summary
                .issues
                .iter()
                .map(|issue| issue.issue_type.as_str())
                .collect::<Vec<_>>(),
            vec!["failed_call", "missing_model", "missing_tokens"]
        );
        assert!(rows
            .iter()
            .any(|row| row.call_id == "missing-model" && row.issue_type == "missing_model"));
        assert!(rows
            .iter()
            .any(|row| row.call_id == "missing-tokens" && row.issue_type == "missing_tokens"));
        assert!(rows
            .iter()
            .any(|row| row.call_id == "failed-call" && row.issue_type == "failed_call"));
        assert!(rows.iter().all(|row| row.issue_type != "missing_cost"));
        assert!(rows
            .iter()
            .all(|row| row.issue_type != "missing_pricing_rule"));
    }

    #[tokio::test]
    async fn list_unknown_pricing_models_ignores_models_with_effective_rules() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        insert_pricing_rule(
            &repository,
            "known-rule",
            "openai-compatible",
            "gpt-known",
            "USD",
            1.0,
            "2026-01-01",
            None,
        )
        .await;
        insert_health_call(
            &repository,
            "known-call",
            Some("gpt-known"),
            100,
            0.01,
            "success",
            None,
            None,
        )
        .await;
        insert_health_call(
            &repository,
            "unknown-call",
            Some("gpt-new"),
            100,
            0.0,
            "success",
            None,
            Some("missing_pricing_rule"),
        )
        .await;

        let rules = repository
            .list_pricing_rules()
            .await
            .expect("pricing rules list succeeds");
        let unknown_models = repository
            .list_unknown_pricing_models()
            .await
            .expect("unknown pricing models query succeeds");

        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].model, "gpt-known");
        assert_eq!(unknown_models.len(), 1);
        assert_eq!(unknown_models[0].provider, "openai-compatible");
        assert_eq!(unknown_models[0].model, "gpt-new");
        assert_eq!(unknown_models[0].calls, 1);
    }

    #[tokio::test]
    async fn upsert_and_delete_pricing_rule_round_trips_local_rules() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        let inserted = repository
            .upsert_pricing_rule(&PricingRuleInput {
                id: None,
                provider: " codex ".to_string(),
                model: " gpt-5.5 ".to_string(),
                currency: "CNY".to_string(),
                input_usd_per_1m: 1.0,
                cached_input_usd_per_1m: 0.1,
                output_usd_per_1m: 8.0,
                reasoning_output_usd_per_1m: None,
                effective_from: "2026-05-01".to_string(),
                effective_to: None,
                source: Some("local".to_string()),
            })
            .await
            .expect("pricing rule inserted");

        assert!(!inserted.id.is_empty());
        assert_eq!(inserted.provider, "codex");
        assert_eq!(inserted.model, "gpt-5.5");
        assert_eq!(inserted.currency, "CNY");
        assert_eq!(inserted.output_usd_per_1m, 8.0);

        let updated = repository
            .upsert_pricing_rule(&PricingRuleInput {
                id: Some(inserted.id.clone()),
                provider: "codex".to_string(),
                model: "gpt-5.5".to_string(),
                currency: "USD".to_string(),
                input_usd_per_1m: 1.0,
                cached_input_usd_per_1m: 0.1,
                output_usd_per_1m: 10.0,
                reasoning_output_usd_per_1m: None,
                effective_from: "2026-05-01".to_string(),
                effective_to: Some("2026-06-01".to_string()),
                source: Some("local".to_string()),
            })
            .await
            .expect("pricing rule updated");

        assert_eq!(updated.id, inserted.id);
        assert_eq!(updated.currency, "USD");
        assert_eq!(updated.output_usd_per_1m, 10.0);
        assert_eq!(updated.effective_to.as_deref(), Some("2026-06-01"));

        assert!(repository
            .delete_pricing_rule(&inserted.id)
            .await
            .expect("pricing rule deleted"));
        assert!(repository
            .list_pricing_rules()
            .await
            .expect("pricing rules listed")
            .is_empty());
    }

    #[tokio::test]
    async fn import_pricing_rules_counts_new_and_updated_rows() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        let rules = vec![
            PricingRuleInput {
                id: Some("import-rule-a".to_string()),
                provider: "codex".to_string(),
                model: "gpt-5.5".to_string(),
                currency: "USD".to_string(),
                input_usd_per_1m: 1.0,
                cached_input_usd_per_1m: 0.1,
                output_usd_per_1m: 8.0,
                reasoning_output_usd_per_1m: None,
                effective_from: "2026-06-01".to_string(),
                effective_to: None,
                source: Some("test_import".to_string()),
            },
            PricingRuleInput {
                id: Some("import-rule-b".to_string()),
                provider: "codex".to_string(),
                model: "gpt-5.3-codex".to_string(),
                currency: "USD".to_string(),
                input_usd_per_1m: 0.0,
                cached_input_usd_per_1m: 0.0,
                output_usd_per_1m: 0.0,
                reasoning_output_usd_per_1m: None,
                effective_from: "2026-06-01".to_string(),
                effective_to: None,
                source: Some("test_import".to_string()),
            },
        ];

        let first = repository
            .import_pricing_rules(&rules)
            .await
            .expect("pricing rules imported");
        assert_eq!(first.imported, 2);
        assert_eq!(first.updated, 0);
        assert_eq!(first.total, 2);

        let second = repository
            .import_pricing_rules(&rules)
            .await
            .expect("pricing rules re-imported");
        assert_eq!(second.imported, 0);
        assert_eq!(second.updated, 2);
        assert_eq!(second.total, 2);
    }

    #[tokio::test]
    async fn sync_settings_round_trip_with_defaults_and_last_run_state() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        let defaults = repository
            .get_sync_settings()
            .await
            .expect("sync settings defaults load");
        assert!(defaults.enabled);
        assert_eq!(defaults.interval_minutes, 30);
        assert!(defaults.sync_on_startup);
        assert_eq!(defaults.last_result.as_deref(), None);

        let saved = repository
            .save_sync_settings(&super::SyncSettingsInput {
                enabled: true,
                interval_minutes: 60,
                sync_on_startup: true,
            })
            .await
            .expect("sync settings save");
        assert!(saved.enabled);
        assert_eq!(saved.interval_minutes, 60);
        assert!(saved.sync_on_startup);

        repository
            .record_sync_run(
                "2026-06-01T09:00:00+08:00",
                "2026-06-01T09:00:03+08:00",
                "success",
                "同步完成",
                3,
                4,
            )
            .await
            .expect("sync run recorded");
        let after_run = repository
            .get_sync_settings()
            .await
            .expect("sync settings reload");

        assert_eq!(
            after_run.last_sync_at.as_deref(),
            Some("2026-06-01T09:00:03+08:00")
        );
        assert_eq!(after_run.last_result.as_deref(), Some("同步完成"));
        assert_eq!(after_run.last_error.as_deref(), None);
        assert!(after_run.next_sync_at.is_some());
    }

    #[tokio::test]
    async fn recalculate_estimated_costs_applies_local_pricing_rules() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");
        insert_pricing_rule(
            &repository,
            "codex-gpt55",
            "codex",
            "gpt-5.5",
            "CNY",
            1.0,
            "2026-01-01",
            None,
        )
        .await;
        insert_cost_recalc_call(
            &repository,
            "known-cost-call",
            "codex",
            "gpt-5.5",
            2000,
            1000,
            500,
            50,
        )
        .await;
        insert_cost_recalc_call(
            &repository,
            "unknown-cost-call",
            "codex",
            "gpt-unknown",
            1000,
            0,
            100,
            0,
        )
        .await;

        let result = repository
            .recalculate_estimated_costs()
            .await
            .expect("cost recalculation succeeds");

        assert_eq!(result.updated, 2);
        assert_eq!(result.missing, 1);

        let known = query(
            r#"
      SELECT estimated_cost_usd, cost_currency, cost_source
      FROM llm_call
      WHERE id = 'known-cost-call'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("known call selected");
        let unknown = query(
            r#"
      SELECT estimated_cost_usd, cost_currency, cost_source
      FROM llm_call
      WHERE id = 'unknown-cost-call'
      "#,
        )
        .fetch_one(repository.pool())
        .await
        .expect("unknown call selected");

        assert!((known.get::<f64, _>("estimated_cost_usd") - 0.002025).abs() < f64::EPSILON);
        assert_eq!(known.get::<String, _>("cost_currency"), "CNY");
        assert_eq!(known.get::<String, _>("cost_source"), "pricing_rule");
        assert_eq!(unknown.get::<f64, _>("estimated_cost_usd"), 0.0);
        assert_eq!(unknown.get::<String, _>("cost_currency"), "USD");
        assert_eq!(
            unknown.get::<String, _>("cost_source"),
            "missing_pricing_rule"
        );
    }

    #[tokio::test]
    async fn dimension_details_support_workflow_project_and_session() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        insert_dimension_call(
            &repository,
            "matching",
            "workflow-a",
            "project-a",
            "session-a",
            "2026-05-30",
            200,
        )
        .await;
        insert_dimension_call(
            &repository,
            "ignored",
            "workflow-b",
            "project-b",
            "session-b",
            "2026-05-30",
            100,
        )
        .await;

        let workflow_summary = repository
            .dimension_summary("2026-05-01", "2026-05-31", "workflow", "workflow-a")
            .await
            .expect("workflow dimension summary succeeds");
        let project_series = repository
            .dimension_daily_series("2026-05-01", "2026-05-31", "project", "project-a")
            .await
            .expect("project dimension series succeeds");
        let session_summary = repository
            .dimension_summary("2026-05-01", "2026-05-31", "session", "session-a")
            .await
            .expect("session dimension summary succeeds");
        let projects = repository
            .top_projects("2026-05-01", "2026-05-31", 10)
            .await
            .expect("top projects succeeds");
        let sessions = repository
            .top_sessions("2026-05-01", "2026-05-31", 10)
            .await
            .expect("top sessions succeeds");

        assert_eq!(workflow_summary.calls, 1);
        assert_eq!(workflow_summary.total_tokens, 200);
        assert_eq!(project_series.len(), 1);
        assert_eq!(project_series[0].dimension.as_deref(), Some("project-a"));
        assert_eq!(session_summary.calls, 1);
        assert_eq!(session_summary.total_tokens, 200);
        assert_eq!(projects[0].dimension, "project-a");
        assert_eq!(sessions[0].dimension, "session-a");
    }

    #[tokio::test]
    async fn csv_export_reuses_call_filters() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        insert_filterable_call(
            &repository,
            "csv-match",
            "codex",
            "worker",
            "gpt-5",
            "success",
            "2026-05-30T12:00:00+08:00",
            "2026-05-30",
        )
        .await;
        insert_filterable_call(
            &repository,
            "csv-ignored",
            "hermes",
            "worker",
            "gpt-5",
            "success",
            "2026-05-30T13:00:00+08:00",
            "2026-05-30",
        )
        .await;

        let csv = repository
            .export_llm_calls_csv(&LlmCallFilters {
                from: Some("2026-05-30".to_string()),
                to: Some("2026-05-30".to_string()),
                provider: Some("codex".to_string()),
                agent_id: None,
                model: None,
                status: None,
                workflow_id: None,
                project_id: None,
                session_id: None,
                limit: 100,
                offset: 0,
            })
            .await
            .expect("csv exported");

        assert!(csv.contains("csv-match"));
        assert!(!csv.contains("csv-ignored"));
    }

    #[tokio::test]
    async fn provider_config_returns_only_redacted_api_key() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        let saved = repository
            .upsert_provider_config(&ProviderConfigInput {
                id: Some("provider-openai".to_string()),
                provider: "openai-compatible".to_string(),
                display_name: "OpenAI".to_string(),
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: Some("sk-test-very-secret-key".to_string()),
                is_default: true,
            })
            .await
            .expect("provider config saved");

        assert_eq!(saved.api_key_redacted.as_deref(), Some("sk-t...-key"));
        assert_ne!(
            saved.api_key_redacted.as_deref(),
            Some("sk-test-very-secret-key")
        );

        let listed = repository
            .list_provider_configs()
            .await
            .expect("provider configs listed");

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "provider-openai");
        assert_eq!(listed[0].api_key_redacted.as_deref(), Some("sk-t...-key"));
        assert_ne!(
            listed[0].api_key_redacted.as_deref(),
            Some("sk-test-very-secret-key")
        );
    }

    #[tokio::test]
    async fn app_settings_upsert_and_read_proxy_debug_values() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        let initial = repository
            .get_app_settings()
            .await
            .expect("settings read succeeds");

        assert_eq!(initial.proxy_port, 14567);
        assert!(!initial.debug_capture_enabled);

        let saved = repository
            .upsert_app_settings(&AppSettingsInput {
                proxy_port: 4317,
                debug_capture_enabled: true,
            })
            .await
            .expect("settings saved");

        assert_eq!(saved.proxy_port, 4317);
        assert!(saved.debug_capture_enabled);

        let loaded = repository
            .get_app_settings()
            .await
            .expect("settings loaded");

        assert_eq!(loaded.proxy_port, 4317);
        assert!(loaded.debug_capture_enabled);
    }

    #[tokio::test]
    async fn csv_export_excludes_sensitive_raw_fields() {
        let repository = TokenScopeRepository::connect_in_memory()
            .await
            .expect("in-memory database connects");
        repository.migrate().await.expect("migrations run");

        query(
            r#"
      INSERT INTO llm_call (
        id,
        started_at,
        ended_at,
        date_local,
        provider,
        provider_config_id,
        api_type,
        model_requested,
        model_response,
        agent_id,
        agent_name,
        workflow_id,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        reasoning_output_tokens,
        total_tokens,
        total_billable_tokens,
        request_count,
        latency_ms,
        http_status,
        status,
        estimated_cost_usd,
        provider_reported_cost_usd,
        reconciled_cost_usd,
        cost_source,
        usage_source,
        raw_usage_json,
        raw_response_json,
        request_hash,
        response_hash,
        prompt_template_id,
        created_at
      ) VALUES (
        'csv-call',
        '2026-05-30T12:00:00+08:00',
        '2026-05-30T12:00:01+08:00',
        '2026-05-30',
        'openai-compatible',
        'provider-openai',
        'responses',
        'gpt-5-mini',
        'gpt-5-mini-2026',
        'agent-1',
        'Codex',
        'workflow-1',
        11,
        7,
        3,
        2,
        20,
        17,
        1,
        950,
        200,
        'success',
        0.0123,
        0.0456,
        0.0345,
        'estimated',
        'provider',
        '{"prompt_tokens":11}',
        '{"Authorization":"Bearer sk-test-very-secret-key","output_text":"raw response"}',
        'request-hash',
        'response-hash',
        'prompt-template-1',
        '2026-05-30T12:00:02+08:00'
      )
      "#,
        )
        .execute(repository.pool())
        .await
        .expect("call with raw response inserted");

        let csv = repository
            .export_llm_calls_csv(&LlmCallFilters {
                from: Some("2026-05-30".to_string()),
                to: Some("2026-05-30".to_string()),
                provider: None,
                agent_id: None,
                model: None,
                status: None,
                workflow_id: None,
                project_id: None,
                session_id: None,
                limit: 100,
                offset: 0,
            })
            .await
            .expect("csv exported");

        assert!(csv.contains("id,started_at,ended_at,date_local,provider"));
        assert!(csv.contains("csv-call"));
        assert!(csv.contains("request-hash"));
        assert!(!csv.contains("Authorization"));
        assert!(!csv.contains("sk-test-very-secret-key"));
        assert!(!csv.contains("raw_response_json"));
        assert!(!csv.contains("raw response"));
    }

    async fn insert_pricing_rule(
        repository: &TokenScopeRepository,
        id: &str,
        provider: &str,
        model: &str,
        currency: &str,
        input_usd_per_1m: f64,
        effective_from: &str,
        effective_to: Option<&str>,
    ) {
        query(
            r#"
      INSERT INTO pricing_rule (
        id,
        provider,
        model,
        currency,
        input_usd_per_1m,
        cached_input_usd_per_1m,
        output_usd_per_1m,
        reasoning_output_usd_per_1m,
        effective_from,
        effective_to,
        source,
        created_at,
        updated_at
      ) VALUES (?1, ?2, ?3, ?4, ?5, 0.025, 2.0, NULL, ?6, ?7, 'test', '2026-01-01', '2026-01-01')
      "#,
        )
        .bind(id)
        .bind(provider)
        .bind(model)
        .bind(currency)
        .bind(input_usd_per_1m)
        .bind(effective_from)
        .bind(effective_to)
        .execute(&repository.pool)
        .await
        .expect("pricing rule inserted");
    }

    async fn insert_minimal_call(
        repository: &TokenScopeRepository,
        id: &str,
        provider: &str,
        started_at: &str,
        total_tokens: i64,
        estimated_cost_usd: f64,
    ) {
        query(
            r#"
      INSERT INTO llm_call (
        id,
        started_at,
        date_local,
        provider,
        total_tokens,
        total_billable_tokens,
        estimated_cost_usd,
        status,
        created_at
      ) VALUES (?1, ?2, '2026-05-30', ?3, ?4, ?4, ?5, 'success', ?2)
      "#,
        )
        .bind(id)
        .bind(started_at)
        .bind(provider)
        .bind(total_tokens)
        .bind(estimated_cost_usd)
        .execute(repository.pool())
        .await
        .expect("minimal call inserted");
    }

    async fn insert_import_map(
        repository: &TokenScopeRepository,
        source: &str,
        external_id: &str,
        llm_call_id: &str,
        imported_at: &str,
    ) {
        query(
            r#"
      INSERT INTO agent_import_map (
        source,
        external_id,
        llm_call_id,
        imported_at
      ) VALUES (?1, ?2, ?3, ?4)
      "#,
        )
        .bind(source)
        .bind(external_id)
        .bind(llm_call_id)
        .bind(imported_at)
        .execute(repository.pool())
        .await
        .expect("import map inserted");
    }

    async fn insert_external_dataset_row(repository: &TokenScopeRepository, id: &str) {
        query(
            r#"
      INSERT INTO external_dataset (
        id,
        device_id,
        device_name,
        package_version,
        source_path,
        imported_at,
        updated_at,
        calls,
        total_tokens,
        estimated_cost_usd
      ) VALUES (
        ?1,
        'device-b',
        'B-PC',
        1,
        'B.tokenscope',
        '2026-05-30T12:00:00+08:00',
        '2026-05-30T12:00:00+08:00',
        1,
        200,
        0.02
      )
      "#,
        )
        .bind(id)
        .execute(repository.pool())
        .await
        .expect("external dataset inserted");
    }

    async fn insert_external_dataset_call(
        repository: &TokenScopeRepository,
        id: &str,
        dataset_id: &str,
        started_at: &str,
        total_tokens: i64,
        estimated_cost_usd: f64,
    ) {
        query(
            r#"
      INSERT INTO llm_call (
        id,
        started_at,
        date_local,
        provider,
        total_tokens,
        total_billable_tokens,
        estimated_cost_usd,
        status,
        origin_dataset_id,
        created_at
      ) VALUES (?1, ?2, '2026-05-30', 'codex', ?3, ?3, ?4, 'success', ?5, ?2)
      "#,
        )
        .bind(id)
        .bind(started_at)
        .bind(total_tokens)
        .bind(estimated_cost_usd)
        .bind(dataset_id)
        .execute(repository.pool())
        .await
        .expect("external dataset call inserted");
    }

    async fn insert_filterable_call(
        repository: &TokenScopeRepository,
        id: &str,
        provider: &str,
        agent_id: &str,
        model: &str,
        status: &str,
        started_at: &str,
        date_local: &str,
    ) {
        query(
            r#"
      INSERT INTO llm_call (
        id,
        started_at,
        date_local,
        provider,
        model_requested,
        model_response,
        agent_id,
        workflow_id,
        total_tokens,
        total_billable_tokens,
        estimated_cost_usd,
        status,
        created_at
      ) VALUES (?1, ?2, ?3, ?4, ?6, ?6, ?5, 'test_workflow', 100, 100, 0.01, ?7, ?2)
      "#,
        )
        .bind(id)
        .bind(started_at)
        .bind(date_local)
        .bind(provider)
        .bind(agent_id)
        .bind(model)
        .bind(status)
        .execute(repository.pool())
        .await
        .expect("filterable call inserted");
    }

    async fn insert_health_call(
        repository: &TokenScopeRepository,
        id: &str,
        model: Option<&str>,
        total_tokens: i64,
        estimated_cost_usd: f64,
        status: &str,
        http_status: Option<i64>,
        cost_source: Option<&str>,
    ) {
        query(
            r#"
      INSERT INTO llm_call (
        id,
        started_at,
        date_local,
        provider,
        model_requested,
        model_response,
        total_tokens,
        total_billable_tokens,
        estimated_cost_usd,
        status,
        http_status,
        cost_source,
        raw_usage_json,
        raw_response_json,
        created_at
      ) VALUES (
        ?1,
        '2026-05-30T12:00:00+08:00',
        '2026-05-30',
        'openai-compatible',
        ?2,
        ?2,
        ?3,
        ?3,
        ?4,
        ?5,
        ?6,
        ?7,
        '{"prompt":"sensitive"}',
        '{"response":"sensitive"}',
        '2026-05-30T12:00:01+08:00'
      )
      "#,
        )
        .bind(id)
        .bind(model)
        .bind(total_tokens)
        .bind(estimated_cost_usd)
        .bind(status)
        .bind(http_status)
        .bind(cost_source)
        .execute(repository.pool())
        .await
        .expect("health call inserted");
    }

    async fn insert_cost_recalc_call(
        repository: &TokenScopeRepository,
        id: &str,
        provider: &str,
        model: &str,
        input_tokens: i64,
        cached_input_tokens: i64,
        output_tokens: i64,
        reasoning_output_tokens: i64,
    ) {
        let total_tokens = input_tokens + output_tokens;
        let total_billable_tokens =
            input_tokens.saturating_sub(cached_input_tokens) + output_tokens;
        query(
            r#"
      INSERT INTO llm_call (
        id,
        started_at,
        date_local,
        provider,
        model_requested,
        model_response,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        reasoning_output_tokens,
        total_tokens,
        total_billable_tokens,
        estimated_cost_usd,
        cost_source,
        status,
        created_at
      ) VALUES (
        ?1,
        '2026-05-30T12:00:00+08:00',
        '2026-05-30',
        ?2,
        ?3,
        ?3,
        ?4,
        ?5,
        ?6,
        ?7,
        ?8,
        ?9,
        0.0,
        'import_no_cost',
        'success',
        '2026-05-30T12:00:01+08:00'
      )
      "#,
        )
        .bind(id)
        .bind(provider)
        .bind(model)
        .bind(input_tokens)
        .bind(output_tokens)
        .bind(cached_input_tokens)
        .bind(reasoning_output_tokens)
        .bind(total_tokens)
        .bind(total_billable_tokens)
        .execute(repository.pool())
        .await
        .expect("cost recalculation call inserted");
    }

    async fn insert_dimension_call(
        repository: &TokenScopeRepository,
        id: &str,
        workflow_id: &str,
        project_id: &str,
        session_id: &str,
        date_local: &str,
        total_tokens: i64,
    ) {
        query(
            r#"
      INSERT INTO llm_call (
        id,
        started_at,
        date_local,
        provider,
        model_requested,
        model_response,
        workflow_id,
        project_id,
        session_id,
        total_tokens,
        total_billable_tokens,
        estimated_cost_usd,
        status,
        created_at
      ) VALUES (
        ?1,
        ?2 || 'T12:00:00+08:00',
        ?2,
        'codex',
        'gpt-5',
        'gpt-5',
        ?3,
        ?4,
        ?5,
        ?6,
        ?6,
        ?6 * 0.0001,
        'success',
        ?2 || 'T12:00:01+08:00'
      )
      "#,
        )
        .bind(id)
        .bind(date_local)
        .bind(workflow_id)
        .bind(project_id)
        .bind(session_id)
        .bind(total_tokens)
        .execute(repository.pool())
        .await
        .expect("dimension call inserted");
    }
}
