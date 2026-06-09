use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::{header, StatusCode};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::Duration;

const GITHUB_READ_REQUEST_RETRIES: usize = 2;
const GITHUB_READ_RETRY_BASE_DELAY_MS: u64 = 250;
const GITHUB_HTTP_TIMEOUT_SECS: u64 = 60;
const GITHUB_HTTP_CONNECT_TIMEOUT_SECS: u64 = 15;

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
    base_url: String,
    http: reqwest::Client,
}

#[allow(dead_code)]
impl GitHubContentsClient {
    pub fn new(owner: String, repo: String, branch: String, token: String) -> Self {
        Self::with_base_url(
            owner,
            repo,
            branch,
            token,
            "https://api.github.com".to_string(),
        )
    }

    #[cfg(test)]
    fn new_with_base_url(
        owner: String,
        repo: String,
        branch: String,
        token: String,
        base_url: String,
    ) -> Self {
        Self::with_base_url(owner, repo, branch, token, base_url)
    }

    fn with_base_url(
        owner: String,
        repo: String,
        branch: String,
        token: String,
        base_url: String,
    ) -> Self {
        Self {
            owner,
            repo,
            branch,
            token,
            base_url,
            http: github_http_client(),
        }
    }

    pub async fn get_file(&self, path: &str) -> Result<Option<GitHubContentFile>, String> {
        let response = send_github_request(
            self.http
                .get(self.contents_url(path))
                .query(&[("ref", self.branch.as_str())])
                .headers(self.json_headers()),
            "GitHub 文件读取失败",
            GITHUB_READ_REQUEST_RETRIES,
        )
        .await?;
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
        let response = send_github_request(
            self.http
                .put(self.contents_url(path))
                .headers(self.json_headers())
                .json(&request),
            "GitHub 文件上传失败",
            0,
        )
        .await?;
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
        let response = send_github_request(
            self.http
                .get(self.contents_url(&layout.devices_path()))
                .query(&[("ref", self.branch.as_str())])
                .headers(self.json_headers()),
            "GitHub 设备目录读取失败",
            GITHUB_READ_REQUEST_RETRIES,
        )
        .await?;
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
        let response = send_github_request(
            self.http
                .get(self.contents_url(&layout.days_path(device_id)))
                .query(&[("ref", self.branch.as_str())])
                .headers(self.json_headers()),
            "GitHub 日期分片目录读取失败",
            GITHUB_READ_REQUEST_RETRIES,
        )
        .await?;
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
            "{}/repos/{}/{}/contents/{}",
            self.base_url.trim_end_matches('/'),
            self.owner.trim_matches('/'),
            self.repo.trim_matches('/'),
            path.trim_matches('/')
        )
    }

    async fn get_raw_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let response = send_github_request(
            self.http
                .get(self.contents_url(path))
                .query(&[("ref", self.branch.as_str())])
                .headers(self.raw_headers()),
            "GitHub raw 文件读取失败",
            GITHUB_READ_REQUEST_RETRIES,
        )
        .await?;
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

fn github_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(GITHUB_HTTP_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(GITHUB_HTTP_CONNECT_TIMEOUT_SECS))
        .build()
        .expect("GitHub HTTP client builds")
}

async fn send_github_request(
    request: reqwest::RequestBuilder,
    context: &str,
    retry_count: usize,
) -> Result<reqwest::Response, String> {
    let mut request = request;
    let mut retries_used = 0;
    loop {
        let retry_request = if retries_used < retry_count {
            request.try_clone()
        } else {
            None
        };
        match request.send().await {
            Ok(response) => return Ok(response),
            Err(err) => {
                if retries_used < retry_count {
                    if let Some(next_request) = retry_request {
                        retries_used += 1;
                        tokio::time::sleep(Duration::from_millis(
                            GITHUB_READ_RETRY_BASE_DELAY_MS * retries_used as u64,
                        ))
                        .await;
                        request = next_request;
                        continue;
                    }
                }
                return Err(format_github_request_error(context, &err, retries_used));
            }
        }
    }
}

fn format_github_request_error(context: &str, err: &reqwest::Error, retries_used: usize) -> String {
    let mut qualifiers = Vec::new();
    if retries_used > 0 {
        qualifiers.push(format!("已重试 {retries_used} 次"));
    }
    if err.is_timeout() {
        qualifiers.push("请求超时".to_string());
    }
    if err.is_connect() {
        qualifiers.push("连接失败".to_string());
    }
    if err.is_request() {
        qualifiers.push("请求发送失败".to_string());
    }
    if let Some(status) = err.status() {
        qualifiers.push(format!("HTTP {status}"));
    }

    let mut message = format!("{context}：{err}");
    if !qualifiers.is_empty() {
        message.push_str(&format!("（{}）", qualifiers.join("，")));
    }

    let mut sources = Vec::new();
    let mut source = err.source();
    while let Some(current) = source {
        sources.push(current.to_string());
        source = current.source();
    }
    if !sources.is_empty() {
        message.push_str(&format!("；底层原因：{}", sources.join(" <- ")));
    }

    message
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
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

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

    #[tokio::test]
    async fn github_get_file_retries_transient_send_failures() {
        let (base_url, attempts) = spawn_github_contents_server(vec![
            TestResponse::DropConnection,
            TestResponse::Json(
                r#"{"name":"2026-06-09.tokenscope.zst.enc","path":"tokenscope-sync/v1/devices/device-a/days/2026-06-09.tokenscope.zst.enc","sha":"file-sha","content":"aGVsbG8=","encoding":"base64"}"#,
            ),
        ]);
        let client = GitHubContentsClient::new_with_base_url(
            "rick".to_string(),
            "tokenscope-sync".to_string(),
            "main".to_string(),
            "token".to_string(),
            base_url,
        );

        let file = client
            .get_file("tokenscope-sync/v1/devices/device-a/days/2026-06-09.tokenscope.zst.enc")
            .await
            .expect("request retries")
            .expect("file exists");

        assert_eq!(file.content, b"hello");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn github_get_file_reports_retry_count_after_repeated_send_failures() {
        let (base_url, attempts) = spawn_github_contents_server(vec![
            TestResponse::DropConnection,
            TestResponse::DropConnection,
            TestResponse::DropConnection,
        ]);
        let client = GitHubContentsClient::new_with_base_url(
            "rick".to_string(),
            "tokenscope-sync".to_string(),
            "main".to_string(),
            "token".to_string(),
            base_url,
        );

        let err = client
            .get_file("tokenscope-sync/v1/devices/device-a/days/2026-06-09.tokenscope.zst.enc")
            .await
            .expect_err("repeated send failures are reported");

        assert!(err.contains("GitHub 文件读取失败"));
        assert!(err.contains("已重试 2 次"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    enum TestResponse {
        DropConnection,
        Json(&'static str),
    }

    fn spawn_github_contents_server(responses: Vec<TestResponse>) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test server binds");
        let base_url = format!("http://{}", listener.local_addr().expect("local addr"));
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_for_thread = Arc::clone(&attempts);
        thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("test request accepted");
                attempts_for_thread.fetch_add(1, Ordering::SeqCst);
                let mut buffer = [0_u8; 2048];
                let _ = stream.read(&mut buffer);
                match response {
                    TestResponse::DropConnection => {
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                    TestResponse::Json(body) => {
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        stream
                            .write_all(response.as_bytes())
                            .expect("test response writes");
                    }
                }
            }
        });
        (base_url, attempts)
    }
}
