use chrono::Local;
use sqlx::{query, query_as, Row};

use super::super::models::{
    GitHubSyncRemoteDevice, GitHubSyncSettings, GitHubSyncSettingsInput, GitHubSyncShardState,
    GitHubSyncShardStateInput, GITHUB_SYNC_DATA_MODE_AGGREGATE_V3, GITHUB_SYNC_DATA_MODE_DETAIL_V2,
};
use super::{
    github_sync_shard_id, normalize_github_sync_data_mode, normalize_non_empty, redact_secret,
    TokenScopeRepository,
};

impl TokenScopeRepository {
    pub async fn get_github_sync_settings(&self) -> Result<GitHubSyncSettings, sqlx::Error> {
        let enabled = self
            .app_setting_value("github_sync_enabled")
            .await?
            .map(|value| value == "true")
            .unwrap_or(false);
        let owner = self
            .app_setting_value("github_sync_owner")
            .await?
            .unwrap_or_default();
        let repo = self
            .app_setting_value("github_sync_repo")
            .await?
            .unwrap_or_default();
        let branch = self
            .app_setting_value("github_sync_branch")
            .await?
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "main".to_string());
        let path_prefix = self
            .app_setting_value("github_sync_path_prefix")
            .await?
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "tokenscope-sync".to_string());
        let token = self.app_setting_value("github_sync_token").await?;
        let sync_password = self.app_setting_value("github_sync_password").await?;
        let bootstrap_uploaded = self
            .app_setting_value("github_sync_bootstrap_uploaded")
            .await?
            .map(|value| value == "true")
            .unwrap_or(false);
        let data_mode = self
            .app_setting_value("github_sync_data_mode")
            .await?
            .map(|value| normalize_github_sync_data_mode(&value).to_string())
            .unwrap_or_else(|| {
                if bootstrap_uploaded {
                    GITHUB_SYNC_DATA_MODE_DETAIL_V2.to_string()
                } else {
                    GITHUB_SYNC_DATA_MODE_AGGREGATE_V3.to_string()
                }
            });
        let last_status = self.app_setting_value("github_sync_last_status").await?;
        let last_message = self.app_setting_value("github_sync_last_message").await?;

        Ok(GitHubSyncSettings {
            enabled,
            owner,
            repo,
            branch,
            path_prefix,
            data_mode,
            token_redacted: token.as_deref().map(redact_secret),
            token_configured: token
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false),
            sync_password_configured: sync_password
                .as_deref()
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false),
            bootstrap_uploaded,
            last_upload_at: self.app_setting_value("github_sync_last_upload_at").await?,
            last_import_at: self.app_setting_value("github_sync_last_import_at").await?,
            last_message: last_message.clone(),
            last_error: if last_status.as_deref() == Some("error") {
                last_message.clone()
            } else {
                None
            },
            last_status,
        })
    }

    pub async fn list_github_sync_remote_devices(
        &self,
    ) -> Result<Vec<GitHubSyncRemoteDevice>, sqlx::Error> {
        query_as::<_, GitHubSyncRemoteDevice>(
            r#"
      WITH shard_summary AS (
        SELECT
          device_id,
          SUM(CASE WHEN shard_kind = 'bootstrap' THEN 1 ELSE 0 END) AS bootstrap_shards,
          SUM(CASE WHEN shard_kind = 'day' THEN 1 ELSE 0 END) AS day_shards,
          MAX(imported_at) AS last_import_at
        FROM github_sync_shard
        WHERE imported_at IS NOT NULL
        GROUP BY device_id
      )
      SELECT
        s.device_id,
        d.device_name,
        s.bootstrap_shards,
        s.day_shards,
        s.last_import_at,
        COALESCE(d.calls, 0) AS calls,
        COALESCE(d.total_tokens, 0) AS total_tokens,
        d.sync_data_mode
      FROM shard_summary s
      LEFT JOIN external_dataset d ON d.device_id = s.device_id
      ORDER BY s.last_import_at DESC, s.device_id ASC
      "#,
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn save_github_sync_settings(
        &self,
        input: &GitHubSyncSettingsInput,
    ) -> Result<GitHubSyncSettings, sqlx::Error> {
        let now = Local::now().to_rfc3339();
        let previous = self.get_github_sync_settings().await?;
        let previous_sync_password = self.app_setting_value("github_sync_password").await?;
        let owner = input.owner.trim().to_string();
        let repo = input.repo.trim().to_string();
        let branch = normalize_non_empty(&input.branch, "main").to_string();
        let path_prefix = normalize_non_empty(&input.path_prefix, "tokenscope-sync").to_string();
        let data_mode =
            normalize_github_sync_data_mode(input.data_mode.as_deref().unwrap_or("")).to_string();
        let next_sync_password = input
            .sync_password
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let sync_namespace_changed = previous.owner != owner
            || previous.repo != repo
            || previous.branch != branch
            || previous.path_prefix != path_prefix
            || previous.data_mode != data_mode
            || next_sync_password
                .map(|password| previous_sync_password.as_deref() != Some(password))
                .unwrap_or(false);

        self.upsert_app_setting_value(
            "github_sync_enabled",
            if input.enabled { "true" } else { "false" },
            &now,
        )
        .await?;
        self.upsert_app_setting_value("github_sync_owner", &owner, &now)
            .await?;
        self.upsert_app_setting_value("github_sync_repo", &repo, &now)
            .await?;
        self.upsert_app_setting_value("github_sync_branch", &branch, &now)
            .await?;
        self.upsert_app_setting_value("github_sync_path_prefix", &path_prefix, &now)
            .await?;
        self.upsert_app_setting_value("github_sync_data_mode", &data_mode, &now)
            .await?;
        if let Some(token) = input
            .token
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            self.upsert_app_setting_value("github_sync_token", token.trim(), &now)
                .await?;
        }
        if let Some(sync_password) = input
            .sync_password
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            self.upsert_app_setting_value("github_sync_password", sync_password, &now)
                .await?;
        }
        if sync_namespace_changed {
            self.reset_github_sync_state_after_config_change(&now)
                .await?;
        }

        self.get_github_sync_settings().await
    }

    pub async fn github_sync_secret(&self, secret: &str) -> Result<Option<String>, sqlx::Error> {
        let key = match secret {
            "token" => "github_sync_token",
            "password" | "sync_password" => "github_sync_password",
            _ => return Ok(None),
        };
        self.app_setting_value(key).await
    }

    pub async fn record_github_sync_shard(
        &self,
        input: &GitHubSyncShardStateInput,
    ) -> Result<GitHubSyncShardState, sqlx::Error> {
        let id = github_sync_shard_id(
            &input.device_id,
            &input.shard_kind,
            input.shard_date.as_deref(),
        );
        let now = Local::now().to_rfc3339();
        query(
            r#"
      INSERT INTO github_sync_shard (
        id,
        device_id,
        shard_kind,
        shard_date,
        content_hash,
        github_blob_sha,
        github_path,
        imported_at,
        updated_at
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
      ON CONFLICT(id) DO UPDATE SET
        device_id = excluded.device_id,
        shard_kind = excluded.shard_kind,
        shard_date = excluded.shard_date,
        content_hash = excluded.content_hash,
        github_blob_sha = excluded.github_blob_sha,
        github_path = excluded.github_path,
        imported_at = excluded.imported_at,
        updated_at = excluded.updated_at
      "#,
        )
        .bind(&id)
        .bind(&input.device_id)
        .bind(&input.shard_kind)
        .bind(&input.shard_date)
        .bind(&input.content_hash)
        .bind(&input.github_blob_sha)
        .bind(&input.github_path)
        .bind(&input.imported_at)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        self.github_sync_shard(
            &input.device_id,
            &input.shard_kind,
            input.shard_date.as_deref(),
        )
        .await?
        .ok_or(sqlx::Error::RowNotFound)
    }

    pub async fn github_sync_shard(
        &self,
        device_id: &str,
        shard_kind: &str,
        shard_date: Option<&str>,
    ) -> Result<Option<GitHubSyncShardState>, sqlx::Error> {
        query_as::<_, GitHubSyncShardState>(
            r#"
      SELECT
        id,
        device_id,
        shard_kind,
        shard_date,
        content_hash,
        github_blob_sha,
        github_path,
        imported_at,
        updated_at
      FROM github_sync_shard
      WHERE device_id = ?1
        AND shard_kind = ?2
        AND (
          (?3 IS NULL AND shard_date IS NULL)
          OR shard_date = ?3
        )
      "#,
        )
        .bind(device_id)
        .bind(shard_kind)
        .bind(shard_date)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn github_sync_uploaded_day_dates(
        &self,
        device_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        query(
            r#"
      SELECT shard_date
      FROM github_sync_shard
      WHERE device_id = ?1
        AND shard_kind = 'day'
        AND shard_date IS NOT NULL
        AND imported_at IS NULL
      ORDER BY shard_date ASC
      "#,
        )
        .bind(device_id)
        .fetch_all(&self.pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .filter_map(|row| row.try_get::<String, _>("shard_date").ok())
                .collect()
        })
    }

    pub async fn set_github_sync_bootstrap_uploaded(
        &self,
        uploaded: bool,
    ) -> Result<(), sqlx::Error> {
        self.upsert_app_setting_value(
            "github_sync_bootstrap_uploaded",
            if uploaded { "true" } else { "false" },
            &Local::now().to_rfc3339(),
        )
        .await
    }

    pub async fn record_github_sync_run(
        &self,
        status: &str,
        message: &str,
        upload_at: Option<&str>,
        import_at: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = Local::now().to_rfc3339();
        self.upsert_app_setting_value("github_sync_last_status", status, &now)
            .await?;
        self.upsert_app_setting_value("github_sync_last_message", message, &now)
            .await?;
        if let Some(upload_at) = upload_at {
            self.upsert_app_setting_value("github_sync_last_upload_at", upload_at, &now)
                .await?;
        }
        if let Some(import_at) = import_at {
            self.upsert_app_setting_value("github_sync_last_import_at", import_at, &now)
                .await?;
        }
        Ok(())
    }

    async fn reset_github_sync_state_after_config_change(
        &self,
        now: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        for (key, value) in [
            ("github_sync_bootstrap_uploaded", "false"),
            ("github_sync_last_status", "reset"),
            (
                "github_sync_last_message",
                "GitHub 同步配置已变更，需要重新同步。",
            ),
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
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
        query(
            r#"
        DELETE FROM app_setting
        WHERE key IN ('github_sync_last_upload_at', 'github_sync_last_import_at')
        "#,
        )
        .execute(&mut *tx)
        .await?;
        query("DELETE FROM github_sync_shard")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(())
    }
}
