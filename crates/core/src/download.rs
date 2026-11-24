use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

pub const IDLE_TIMEOUT_MS: u64 = 5_000;
pub const FIRST_BYTE_TIMEOUT_MS: u64 = 5_000;
pub const MAX_SIZE: u64 = 128 * 1024 * 1024;
pub const RETRY_ATTEMPTS: usize = 3;
pub const RETRY_PAUSE_MS: u64 = 2_000;

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unexpected response {0}")]
    BadStatus(u16),
    #[error("redirected to a non-https url \"{0}\"")]
    NonHttps(String),
    #[error("content-length exceeds {0} bytes")]
    TooLarge(u64),
    #[error("body exceeds {0} bytes")]
    BodyTooLarge(u64),
    #[error("empty response body")]
    Empty,
    #[error("sha256 mismatch, got {0}, expected {1}")]
    HashMismatch(String, String),
    #[error("download timed out")]
    Timeout,
}

impl DownloadError {
    /// Transient failures worth retrying (connection/timeout/IO).
    pub fn is_retryable(&self) -> bool {
        matches!(self, DownloadError::Http(_) | DownloadError::Io(_) | DownloadError::Timeout)
    }
}

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    /// Maximum gap between two body chunks.
    pub idle_timeout: Duration,
    /// Maximum wait for response headers (first byte) after connecting.
    pub first_byte_timeout: Duration,
    pub max_size: u64,
    pub expected_sha256: Option<String>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            idle_timeout: Duration::from_millis(IDLE_TIMEOUT_MS),
            first_byte_timeout: Duration::from_millis(FIRST_BYTE_TIMEOUT_MS),
            max_size: MAX_SIZE,
            expected_sha256: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadResult {
    pub sha256: String,
    pub size: u64,
}

pub async fn download_file(
    url: &str,
    path: impl AsRef<Path>,
    options: DownloadOptions,
) -> Result<DownloadResult, DownloadError> {
    let path = path.as_ref().to_path_buf();
    let tmp_path = PathBuf::from(format!("{}.download", path.display()));

    let client =
        reqwest::Client::builder().redirect(reqwest::redirect::Policy::limited(10)).build()?;

    let result = async {
        // Wait for the first byte (response headers) with a dedicated timeout.
        // A total-operation deadline is intentionally not used: large cores and
        // geo assets may legitimately take longer than a few seconds to stream.
        let response = tokio::time::timeout(options.first_byte_timeout, client.get(url).send())
            .await
            .map_err(|_| DownloadError::Timeout)??;
        if !response.status().is_success() {
            return Err(DownloadError::BadStatus(response.status().as_u16()));
        }
        if !response.url().scheme().eq_ignore_ascii_case("https") {
            return Err(DownloadError::NonHttps(response.url().to_string()));
        }
        if let Some(len) = response.content_length() {
            if len > options.max_size {
                return Err(DownloadError::TooLarge(len));
            }
        }

        let mut file = tokio::fs::File::create(&tmp_path).await?;
        let mut stream = response.bytes_stream();
        let mut hasher = Sha256::new();
        let mut size: u64 = 0;

        loop {
            let chunk = match tokio::time::timeout(options.idle_timeout, stream.next()).await {
                Ok(Some(chunk)) => chunk?,
                Ok(None) => break,
                Err(_) => return Err(DownloadError::Timeout),
            };
            size += chunk.len() as u64;
            if size > options.max_size {
                return Err(DownloadError::BodyTooLarge(size));
            }
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
        }

        file.flush().await?;
        drop(file);

        if size == 0 {
            return Err(DownloadError::Empty);
        }

        let sha256 = format!("{:x}", hasher.finalize());
        if let Some(expected) = &options.expected_sha256 {
            if !sha256.eq_ignore_ascii_case(expected) {
                return Err(DownloadError::HashMismatch(sha256, expected.clone()));
            }
        }

        tokio::fs::rename(&tmp_path, &path).await?;
        Ok(DownloadResult { sha256, size })
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&tmp_path).await;
    }

    result
}

/// Download with a bounded number of retries. Each failed attempt is logged,
/// then the download pauses before trying again.
pub async fn download_file_with_retry(
    url: &str,
    path: impl AsRef<Path>,
    options: DownloadOptions,
) -> Result<DownloadResult, DownloadError> {
    let path = path.as_ref().to_path_buf();
    let attempts = RETRY_ATTEMPTS.max(1);

    for attempt in 1..=attempts {
        match download_file(url, &path, options.clone()).await {
            Ok(result) => return Ok(result),
            Err(error) if attempt < attempts && error.is_retryable() => {
                tracing::warn!(
                    "download {url} failed (attempt {attempt}/{attempts}): {error}; retrying in {RETRY_PAUSE_MS}ms"
                );
                tokio::time::sleep(Duration::from_millis(RETRY_PAUSE_MS)).await;
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("the retry loop always returns on the last attempt")
}
