use std::path::PathBuf;

use rw_core::download::{download_file_with_retry, DownloadOptions};
use serde::Deserialize;
use serde_json::Value;
use tracing::{error, info, warn};

pub const DEFAULT_ASSETS_DIR: &str = "/usr/local/share/xray";
pub const ASSETS_DIR_ENV: &str = "XRAY_LOCATION_ASSET";
pub const CONCURRENCY: usize = 5;

/// Mirrors the Node `resolve(process.env.XRAY_LOCATION_ASSET || default)`.
pub fn assets_dir() -> PathBuf {
    let dir = std::env::var(ASSETS_DIR_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_ASSETS_DIR));
    std::path::absolute(&dir).unwrap_or(dir)
}

#[derive(Debug, Clone, Deserialize)]
struct AssetsSchema {
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    url: String,
    file: String,
}

pub struct Geodata;

impl Geodata {
    pub async fn prepare(geodata: &Value) -> Result<(), String> {
        if geodata.is_null() {
            return Ok(());
        }
        let parsed: AssetsSchema = match serde_json::from_value(geodata.clone()) {
            Ok(p) => p,
            Err(e) => {
                warn!("Invalid \"assets\" section, skipped: {e}");
                return Ok(());
            }
        };
        if parsed.assets.is_empty() {
            return Ok(());
        }
        for asset in &parsed.assets {
            if !asset.url.starts_with("https://") || !safe_file_name(&asset.file) {
                warn!("Invalid \"assets\" section, skipped.");
                return Ok(());
            }
        }

        let assets = parsed.assets;
        let dir = assets_dir();
        // Await every chunk before starting the next one, otherwise `chunks`
        // only groups the tasks and all assets are fetched at once.
        for chunk in assets.chunks(CONCURRENCY.max(1)) {
            let mut join = Vec::new();
            for asset in chunk {
                let asset = asset.clone();
                let dir = dir.clone();
                join.push(tokio::spawn(async move { prepare_asset(asset, dir).await }));
            }
            for handle in join {
                let _ = handle.await;
            }
        }
        info!("{} asset(s) processed", assets.len());
        Ok(())
    }
}

async fn prepare_asset(asset: Asset, dir: PathBuf) {
    if !safe_file_name(&asset.file) {
        warn!("Invalid asset file name: {}", asset.file);
        return;
    }
    let path = dir.join(&asset.file);

    if exists_nonempty(&path).await {
        return;
    }

    if let Ok(downloaded) =
        download_file_with_retry(&asset.url, &path, DownloadOptions::default()).await
    {
        info!("Downloaded \"{}\" ({} bytes)", asset.file, downloaded.size);
        return;
    }

    match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(_) => warn!("Created empty stub asset \"{}\".", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => error!("Failed to create stub asset \"{}\": {e}", path.display()),
    }
}

async fn exists_nonempty(path: &PathBuf) -> bool {
    match tokio::fs::metadata(path).await {
        Ok(meta) => meta.is_file() && meta.len() > 0,
        Err(_) => false,
    }
}

fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}
