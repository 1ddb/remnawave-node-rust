use std::path::PathBuf;
use std::time::Duration;

use rw_core::download::{download_file, DownloadOptions};
use serde::Deserialize;
use serde_json::Value;
use tracing::{error, info, warn};

pub const ASSETS_DIR: &str = "/usr/local/share/xray";
pub const TOTAL_TIMEOUT_MS: u64 = 15_000;
pub const CONCURRENCY: usize = 5;

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

        let assets = parsed.assets;
        // Await every chunk before starting the next one, otherwise `chunks`
        // only groups the tasks and all assets are fetched at once.
        for chunk in assets.chunks(CONCURRENCY.max(1)) {
            let mut join = Vec::new();
            for asset in chunk {
                let asset = asset.clone();
                join.push(tokio::spawn(async move { prepare_asset(asset).await }));
            }
            for handle in join {
                let _ = handle.await;
            }
        }
        info!("{} asset(s) processed", assets.len());
        Ok(())
    }
}

async fn prepare_asset(asset: Asset) {
    if !safe_file_name(&asset.file) {
        warn!("Invalid asset file name: {}", asset.file);
        return;
    }
    let path = PathBuf::from(ASSETS_DIR).join(&asset.file);

    if exists_nonempty(&path).await {
        return;
    }

    if let Ok(downloaded) = download_file(
        &asset.url,
        &path,
        DownloadOptions {
            total_timeout: Duration::from_millis(TOTAL_TIMEOUT_MS),
            ..Default::default()
        },
    )
    .await
    {
        info!("Downloaded \"{}\" ({} bytes)", asset.file, downloaded.size);
        return;
    }

    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
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
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}
