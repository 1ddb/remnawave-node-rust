use tokio::process::Command;
use tracing::{debug, warn};

use crate::state::ConnectionDropState;

pub async fn drop_connections(state: &ConnectionDropState, ips: &[String], available: bool) {
    if !available || ips.is_empty() {
        return;
    }

    for ip in ips {
        if state.is_whitelisted(ip) {
            continue;
        }
        if let Err(e) = kill_sockets(ip).await {
            warn!("Failed to destroy connections for IP {ip}: {e}");
        }
    }
}

async fn kill_sockets(ip: &str) -> Result<(), std::io::Error> {
    let out = Command::new("ss").args(["-K", "src", ip]).output().await?;
    debug!("ss -K src {ip}: {}", String::from_utf8_lossy(&out.stdout).trim());

    let out = Command::new("ss").args(["-K", "dst", ip]).output().await?;
    debug!("ss -K dst {ip}: {}", String::from_utf8_lossy(&out.stdout).trim());
    Ok(())
}
