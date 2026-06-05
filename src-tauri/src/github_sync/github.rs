use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::{header, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct GitHubSyncLayout {
    prefix: String,
}

impl GitHubSyncLayout {
    pub fn new(prefix: String) -> Self {
        let prefix = normalize_prefix(&prefix);
        Self { prefix }
    }

    #[allow(dead_code)]
    pub fn space_path(&self) -> String {
        self.path("space.json")
    }

    pub fn devices_path(&self) -> String {
        self.path("devices")
    }

    pub fn manifest_path(&self, device_id: &str) -> String {
        self.path(&format!(
            "devices/{}/manifest.enc",
            safe_path_segment(device_id)
        ))
    }

    pub fn bootstrap_path(&self, device_id: &str) -> String {
        self.path(&format!(
            "devices/{}/bootstrap.tokenscope.zst.enc",
            safe_path_segment(device_id)
        ))
    }

    pub fn day_path(&self, device_id: &str, date_local: &str) -> String {
        self.path(&format!(
            "devices/{}/days/{}.tokenscope.zst.enc",
            safe_path_segment(device_id),
            safe_path_segment(date_local)
        ))
    }

    pub fn days_path(&self, device_id: &str) -> String {
        self.path(&format!("devices/{}/days", safe_path_segment(device_id)))
    }

    fn path(&self, suffix: &str) -> String {
        format!("{}/v1/{}", self.prefix, suffix.trim_matches('/'))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GitHubPutFileRequest {
    pub message: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

impl GitHubPutFileRequest {
    pub fn new(message: String, content: Vec<u8>, sha: Option<String>) -> Self {
        Self {
            message,
            content: STANDARD.encode(content),
            sha,
            branch: None,
        }
    }

    fn with_branch(mut self, branch: &str) -> Self {
        self.branch = Some(branch.to_string());
        self
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GitHubContentFile {
    pub name: String,
    pub path: String,
    pub sha: String,
    pub content: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GitHubContentsClient {
    owner: String,
    repo: String,
    branch: String,
    token: String,
    http: reqwest::Client,
}

#[allow(dead_code)]
impl GitHubContentsClient {
    pub fn new(owner: String, repo: String, branch: String, token: String) -> Self {
        Self {
            owner,
            repo,
            branch,
            token,
            http: reqwest::Client::new(),
        }
    }

    pub async fn get_file(&self, path: &str) -> Result<Option<GitHubContentFile>, String> {
        let response = self
            .http
            .get(self.contents_url(path))
            .query(&[("ref", self.branch.as_str())])
            .headers(self.headers())
            .send()
            .await
            .map_err(|err| format!("GitHub 文件读取失败：{err}"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }

        let body = response
            .json::<GitHubGetFileResponse>()
            .await
            .map_err(|err| format!("GitHub 文件响应解析失败：{err}"))?;
        Ok(Some(body.into_content_file()?))
    }

    pub async fn put_file(
        &self,
        path: &str,
        content: Vec<u8>,
        sha: Option<String>,
        message: &str,
    ) -> Result<GitHubContentFile, String> {
        let request =
            GitHubPutFileRequest::new(message.to_string(), content, sha).with_branch(&self.branch);
        let response = self
            .http
            .put(self.contents_url(path))
            .headers(self.headers())
            .json(&request)
            .send()
            .await
            .map_err(|err| format!("GitHub 文件上传失败：{err}"))?;
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }

        let body = response
            .json::<GitHubPutFileResponse>()
            .await
            .map_err(|err| format!("GitHub 上传响应解析失败：{err}"))?;
        Ok(GitHubContentFile {
            name: body.content.name,
            path: body.content.path,
            sha: body.content.sha,
            content: Vec::new(),
        })
    }

    pub async fn list_device_dirs(&self, layout: &GitHubSyncLayout) -> Result<Vec<String>, String> {
        let response = self
            .http
            .get(self.contents_url(&layout.devices_path()))
            .query(&[("ref", self.branch.as_str())])
            .headers(self.headers())
            .send()
            .await
            .map_err(|err| format!("GitHub 设备目录读取失败：{err}"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }

        let entries = response
            .json::<Vec<GitHubListEntry>>()
            .await
            .map_err(|err| format!("GitHub 设备目录响应解析失败：{err}"))?;
        Ok(entries
            .into_iter()
            .filter(|entry| entry.entry_type == "dir")
            .map(|entry| entry.name)
            .collect())
    }

    pub async fn list_day_files(
        &self,
        layout: &GitHubSyncLayout,
        device_id: &str,
    ) -> Result<Vec<GitHubContentFile>, String> {
        let response = self
            .http
            .get(self.contents_url(&layout.days_path(device_id)))
            .query(&[("ref", self.branch.as_str())])
            .headers(self.headers())
            .send()
            .await
            .map_err(|err| format!("GitHub 日期分片目录读取失败：{err}"))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }

        let entries = response
            .json::<Vec<GitHubListEntry>>()
            .await
            .map_err(|err| format!("GitHub 日期分片目录响应解析失败：{err}"))?;
        Ok(entries
            .into_iter()
            .filter(|entry| entry.entry_type == "file")
            .filter_map(|entry| {
                Some(GitHubContentFile {
                    name: entry.name,
                    path: entry.path?,
                    sha: entry.sha?,
                    content: Vec::new(),
                })
            })
            .collect())
    }

    fn contents_url(&self, path: &str) -> String {
        format!(
            "https://api.github.com/repos/{}/{}/contents/{}",
            self.owner.trim_matches('/'),
            self.repo.trim_matches('/'),
            path.trim_matches('/')
        )
    }

    fn headers(&self) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static("TokenScope Desktop"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            header::HeaderValue::from_static("2022-11-28"),
        );
        if let Ok(value) = header::HeaderValue::from_str(&format!("Bearer {}", self.token)) {
            headers.insert(header::AUTHORIZATION, value);
        }
        headers
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubGetFileResponse {
    name: String,
    path: String,
    sha: String,
    content: Option<String>,
    encoding: Option<String>,
}

impl GitHubGetFileResponse {
    fn into_content_file(self) -> Result<GitHubContentFile, String> {
        let content = match (self.content, self.encoding.as_deref()) {
            (Some(content), Some("base64")) => STANDARD
                .decode(content.replace('\n', ""))
                .map_err(|err| format!("GitHub 文件 base64 解码失败：{err}"))?,
            _ => Vec::new(),
        };

        Ok(GitHubContentFile {
            name: self.name,
            path: self.path,
            sha: self.sha,
            content,
        })
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubPutFileResponse {
    content: GitHubContentNode,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubContentNode {
    name: String,
    path: String,
    sha: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitHubListEntry {
    name: String,
    path: Option<String>,
    sha: Option<String>,
    #[serde(rename = "type")]
    entry_type: String,
}

#[allow(dead_code)]
async fn api_error(response: reqwest::Response) -> String {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if text.trim().is_empty() {
        format!("GitHub API 请求失败：{status}")
    } else {
        format!("GitHub API 请求失败：{status} {text}")
    }
}

fn normalize_prefix(prefix: &str) -> String {
    let normalized = prefix
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        "tokenscope-sync".to_string()
    } else {
        normalized
    }
}

fn safe_path_segment(segment: &str) -> String {
    segment
        .trim_matches('/')
        .replace('\\', "-")
        .replace('/', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_sync_path_normalizes_prefix_and_device_files() {
        let layout = GitHubSyncLayout::new("/tokenscope-sync//".to_string());

        assert_eq!(layout.space_path(), "tokenscope-sync/v1/space.json");
        assert_eq!(layout.devices_path(), "tokenscope-sync/v1/devices");
        assert_eq!(
            layout.manifest_path("device-a"),
            "tokenscope-sync/v1/devices/device-a/manifest.enc"
        );
        assert_eq!(
            layout.bootstrap_path("device-a"),
            "tokenscope-sync/v1/devices/device-a/bootstrap.tokenscope.zst.enc"
        );
        assert_eq!(
            layout.day_path("device-a", "2026-06-05"),
            "tokenscope-sync/v1/devices/device-a/days/2026-06-05.tokenscope.zst.enc"
        );
        assert_eq!(
            layout.days_path("device-a"),
            "tokenscope-sync/v1/devices/device-a/days"
        );
    }

    #[test]
    fn github_content_put_request_uses_expected_sha_for_updates() {
        let request = GitHubPutFileRequest::new(
            "sync".to_string(),
            b"hello".to_vec(),
            Some("old-sha".to_string()),
        );

        assert_eq!(request.message, "sync");
        assert_eq!(request.sha.as_deref(), Some("old-sha"));
        assert_eq!(request.content, "aGVsbG8=");

        let request = request.with_branch("main");
        assert_eq!(request.branch.as_deref(), Some("main"));
    }
}
