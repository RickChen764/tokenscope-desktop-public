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
            .headers(self.json_headers())
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
        let content = match body.content_bytes()? {
            Some(content) => content,
            None => self.get_raw_file(&body.path).await?,
        };
        Ok(Some(body.into_content_file(content)))
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
            .headers(self.json_headers())
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
            .headers(self.json_headers())
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
            .headers(self.json_headers())
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

    async fn get_raw_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let response = self
            .http
            .get(self.contents_url(path))
            .query(&[("ref", self.branch.as_str())])
            .headers(self.raw_headers())
            .send()
            .await
            .map_err(|err| format!("GitHub raw 文件读取失败：{err}"))?;
        if !response.status().is_success() {
            return Err(api_error(response).await);
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|err| format!("GitHub raw 文件响应读取失败：{err}"))
    }

    fn json_headers(&self) -> header::HeaderMap {
        self.headers_with_accept("application/vnd.github+json")
    }

    fn raw_headers(&self) -> header::HeaderMap {
        self.headers_with_accept("application/vnd.github.raw+json")
    }

    fn headers_with_accept(&self, accept: &'static str) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::ACCEPT, header::HeaderValue::from_static(accept));
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
    fn content_bytes(&self) -> Result<Option<Vec<u8>>, String> {
        match (self.content.as_deref(), self.encoding.as_deref()) {
            (Some(content), Some("base64")) => STANDARD
                .decode(content.replace('\n', ""))
                .map(Some)
                .map_err(|err| format!("GitHub 文件 base64 解码失败：{err}")),
            (Some(content), Some("none")) if content.trim().is_empty() => Ok(None),
            (None, _) => Ok(None),
            (_, Some(encoding)) => Err(format!("不支持的 GitHub 文件编码：{encoding}")),
            _ => Ok(None),
        }
    }

    fn into_content_file(self, content: Vec<u8>) -> GitHubContentFile {
        GitHubContentFile {
            name: self.name,
            path: self.path,
            sha: self.sha,
            content,
        }
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

    #[test]
    fn github_content_response_marks_large_object_content_for_raw_download() {
        let response = GitHubGetFileResponse {
            name: "bootstrap.tokenscope.zst.enc".to_string(),
            path: "tokenscope-sync/v1/devices/device-a/bootstrap.tokenscope.zst.enc".to_string(),
            sha: "file-sha".to_string(),
            content: Some("".to_string()),
            encoding: Some("none".to_string()),
        };

        assert!(response.content_bytes().expect("content parses").is_none());
    }

    #[test]
    fn github_content_response_decodes_base64_content() {
        let response = GitHubGetFileResponse {
            name: "manifest.enc".to_string(),
            path: "tokenscope-sync/v1/devices/device-a/manifest.enc".to_string(),
            sha: "file-sha".to_string(),
            content: Some("aGVsbG8=".to_string()),
            encoding: Some("base64".to_string()),
        };

        assert_eq!(
            response.content_bytes().expect("content parses"),
            Some(b"hello".to_vec())
        );
    }
}
