use std::os::unix::fs::FileTypeExt;

use tracing::{debug, info, warn};

use crate::state::PreStartState;

const GLOB_MAGIC: &[char] = &['*', '?', '[', ']'];
const MAX_MATCHES_PER_PATTERN: usize = 256;

pub async fn run(state: &PreStartState) {
    if !state.enabled || !state.cleanup_sockets_enabled {
        return;
    }
    if state.cleanup_socket_files.is_empty() {
        return;
    }

    let mut removed = 0usize;
    for pattern in &state.cleanup_socket_files {
        for path in resolve_pattern(pattern) {
            if remove_socket(&path).await {
                removed += 1;
            }
        }
    }

    info!("[PRE-START] Cleanup Sockets: {removed} stale socket(s) removed.");
}

fn resolve_pattern(pattern: &str) -> Vec<String> {
    if !pattern.contains(GLOB_MAGIC) {
        return vec![pattern.to_string()];
    }

    let mut matches = Vec::new();
    match glob::glob(pattern) {
        Ok(paths) => {
            for entry in paths.flatten() {
                if matches.len() >= MAX_MATCHES_PER_PATTERN {
                    warn!(
                        "[PRE-START] Cleanup Sockets: pattern \"{pattern}\" matched more than \
                         {MAX_MATCHES_PER_PATTERN} entries, the rest is skipped."
                    );
                    break;
                }
                matches.push(entry.to_string_lossy().into_owned());
            }
        }
        Err(e) => warn!("[PRE-START] Cleanup Sockets: failed to expand pattern \"{pattern}\": {e}"),
    }
    matches
}

async fn remove_socket(path: &str) -> bool {
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => {
            if !metadata.file_type().is_socket() {
                debug!("[PRE-START] Cleanup Sockets: \"{path}\" is not a socket, skipped.");
                return false;
            }
            match tokio::fs::remove_file(path).await {
                Ok(()) => {
                    info!("[PRE-START] Cleanup Sockets: removed stale socket \"{path}\".");
                    true
                }
                Err(e) => {
                    warn!("[PRE-START] Cleanup Sockets: failed to remove \"{path}\": {e}");
                    false
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            warn!("[PRE-START] Cleanup Sockets: failed to stat \"{path}\": {e}");
            false
        }
    }
}
