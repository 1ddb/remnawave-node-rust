use std::path::Path;

use crate::process::CORE_LINK;
use rw_core::download::{download_file_with_retry, DownloadOptions};
use rw_core::version::read_core_version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info, warn};

pub const CORE_STOCK: &str = "/usr/local/bin/xray";
pub const CORE_CUSTOM: &str = "/usr/local/bin/xray-custom";
pub const CORE_STAGED: &str = "/usr/local/bin/xray-custom.staged";
pub const CORE_MARKER: &str = "/usr/local/bin/.rw-core.json";

#[derive(Debug, Clone, Deserialize)]
struct CoreSchema {
    core: Option<CoreConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct CoreConfig {
    url: String,
    sha256: String,
}

impl CoreConfig {
    fn is_valid(&self) -> bool {
        self.url.starts_with("https://")
            && self.sha256.len() == 64
            && self.sha256.chars().all(|c| c.is_ascii_hexdigit())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoreMarker {
    url: String,
    sha256: String,
    size: u64,
    #[serde(rename = "mtimeMs")]
    mtime_ms: u128,
    version: String,
    #[serde(rename = "installedAt")]
    installed_at: String,
}

pub struct CoreLoader;

impl CoreLoader {
    pub async fn prepare(geodata: &Value) -> Result<(), String> {
        // `zod` treats `null` as `{}` after `geodata ?? {}`, so a null section
        // simply means "no custom core".
        if geodata.is_null() {
            Self::restore_stock().await;
            return Ok(());
        }
        let parsed: CoreSchema = match serde_json::from_value(geodata.clone()) {
            Ok(p) => p,
            Err(e) => {
                warn!("Invalid \"core\" section, skipped: {e}");
                return Ok(());
            }
        };

        let Some(core) = parsed.core else {
            Self::restore_stock().await;
            return Ok(());
        };

        if !core.is_valid() {
            warn!("Invalid \"core\" section, skipped.");
            return Ok(());
        }

        if Self::is_installed(&core).await {
            return Ok(());
        }

        Self::install(&core).await;
        Ok(())
    }

    async fn is_installed(core: &CoreConfig) -> bool {
        let Ok(content) = tokio::fs::read_to_string(CORE_MARKER).await else {
            return false;
        };
        let Ok(marker) = serde_json::from_str::<CoreMarker>(&content) else {
            return false;
        };

        if marker.url != core.url {
            info!("Configured url changed, re-downloading.");
            return false;
        }
        if !marker.sha256.eq_ignore_ascii_case(&core.sha256) {
            info!("Configured sha256 changed, re-downloading.");
            return false;
        }

        let Ok(meta) = tokio::fs::metadata(CORE_CUSTOM).await else {
            return false;
        };
        if meta.len() != marker.size || mtime_ms(&meta) != marker.mtime_ms {
            warn!("Installed binary no longer matches the marker.");
            return false;
        }

        matches!(tokio::fs::read_link(CORE_LINK).await, Ok(target) if target == Path::new(CORE_CUSTOM))
    }

    async fn install(core: &CoreConfig) {
        let options = DownloadOptions {
            expected_sha256: Some(core.sha256.to_lowercase()),
            ..Default::default()
        };

        let result = async {
            let downloaded = download_file_with_retry(&core.url, CORE_STAGED, options).await?;
            set_executable(CORE_STAGED)?;
            let version = read_core_version(CORE_STAGED).await.map_err(|e| e.to_string())?;
            tokio::fs::rename(CORE_STAGED, CORE_CUSTOM).await?;
            Self::point_link_at(CORE_CUSTOM).await?;
            let meta = tokio::fs::metadata(CORE_CUSTOM).await?;
            let marker = CoreMarker {
                url: core.url.clone(),
                sha256: downloaded.sha256,
                size: meta.len(),
                mtime_ms: mtime_ms(&meta),
                version: version.raw,
                installed_at: chrono::Utc::now().to_rfc3339(),
            };
            tokio::fs::write(CORE_MARKER, serde_json::to_string_pretty(&marker)?).await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(downloaded.size)
        }
        .await;

        match result {
            Ok(size) => info!("Installed custom core ({} bytes)", size),
            Err(e) => {
                error!("Failed to install custom core: {e}");
                let _ = tokio::fs::remove_file(CORE_STAGED).await;
            }
        }
    }

    async fn restore_stock() {
        let current = tokio::fs::read_link(CORE_LINK).await.ok();
        if current.as_deref() == Some(Path::new(CORE_STOCK)) {
            return;
        }
        if current.is_none() {
            // The link may be missing (fresh boots, manual cleanup). Recreate
            // it instead of leaving the supervisor without a core binary.
            if let Err(e) = Self::point_link_at(CORE_STOCK).await {
                warn!("Failed to link the bundled core at {CORE_LINK}: {e}");
                return;
            }
        } else {
            let _ = Self::point_link_at(CORE_STOCK).await;
        }
        let _ = tokio::fs::remove_file(CORE_MARKER).await;
        let _ = tokio::fs::remove_file(CORE_CUSTOM).await;
        warn!("No core configured, rolled back to the bundled one.");
    }

    async fn point_link_at(target: &str) -> std::io::Result<()> {
        let tmp_link = format!("{CORE_LINK}.tmp");
        let _ = tokio::fs::remove_file(&tmp_link).await;
        tokio::fs::symlink(target, &tmp_link).await?;
        tokio::fs::rename(&tmp_link, CORE_LINK).await
    }
}

fn set_executable(path: &str) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)
}

fn mtime_ms(meta: &std::fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

