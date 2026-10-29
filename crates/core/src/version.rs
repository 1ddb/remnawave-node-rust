use std::time::Duration;

use thiserror::Error;
use tokio::process::Command;

pub const EXEC_TIMEOUT_MS: u64 = 3_000;

#[derive(Debug, Clone)]
pub struct CoreVersion {
    pub raw: String,
    pub semver: Option<String>,
}

#[derive(Debug, Error)]
pub enum VersionError {
    #[error("failed to execute binary: {0}")]
    Exec(#[from] std::io::Error),
    #[error("binary produced no version output")]
    Empty,
    #[error("binary timed out")]
    Timeout,
}

pub async fn read_core_version(path: &str) -> Result<CoreVersion, VersionError> {
    let output = tokio::time::timeout(
        Duration::from_millis(EXEC_TIMEOUT_MS),
        Command::new(path).arg("version").output(),
    )
    .await
    .map_err(|_| VersionError::Timeout)??;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw = stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(VersionError::Empty)?
        .to_string();

    let semver = coerce_semver(&raw);
    Ok(CoreVersion { raw, semver })
}

/// Approximates `semver.coerce(raw, { includePrerelease: true })`.
pub fn coerce_semver(raw: &str) -> Option<String> {
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    for token in tokens {
        let version: String = token
            .trim_start_matches('v')
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || c.is_ascii_alphabetic())
            .collect();
        if version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            if let Ok(parsed) = semver::Version::parse(&normalize(&version)) {
                return Some(parsed.to_string());
            }
        }
    }
    None
}

fn normalize(input: &str) -> String {
    let core: String = input
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || c.is_ascii_alphabetic())
        .collect();
    let core = core.trim_end_matches(['.', '-']).to_string();
    let parts: Vec<&str> = core.splitn(2, '-').collect();
    let mut nums: Vec<String> = parts[0].split('.').map(|s| s.to_string()).collect();
    while nums.len() < 3 {
        nums.push("0".into());
    }
    let mut out = nums.join(".");
    if let Some(pre) = parts.get(1) {
        if !pre.is_empty() {
            out.push('-');
            out.push_str(pre);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerce_versions() {
        assert_eq!(coerce_semver("Xray 25.3.6 (go1.24)"), Some("25.3.6".into()));
        assert_eq!(coerce_semver("v1.2"), Some("1.2.0".into()));
    }
}
