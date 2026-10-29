use std::net::IpAddr;

use chrono::{Duration, Utc};
use rw_contract::models::{TorrentBlockerActionReport, TorrentBlockerReport, XrayWebhook};
use tracing::{debug, error, info, warn};

use crate::nft::{NftManager, TORRENT_BLOCKER_SET_NAME};
use crate::state::TorrentBlockerState;

pub async fn handle_webhook(
    state: &mut TorrentBlockerState,
    nft: &NftManager,
    webhook: XrayWebhook,
) -> Option<TorrentBlockerReport> {
    if !state.enabled {
        return None;
    }

    let ip = extract_ip(webhook.source.as_deref())?;
    let email = webhook.email.as_deref()?;

    if state.is_ip_ignored(&ip) || state.is_user_ignored(email) {
        return None;
    }

    let block_duration = state.block_duration?;
    let mut blocked = false;

    match nft
        .add_address(&ip, TORRENT_BLOCKER_SET_NAME, Some(block_duration))
        .await
    {
        Ok(()) => {
            blocked = true;
            info!(
                "[TORRENT-BLOCKER] IP: {ip}, user: {email}, blocked: {blocked}, \
                 duration: {block_duration}s"
            );
        }
        Err(e) => error!("Failed to block IP {ip}: {e}"),
    }

    let report = TorrentBlockerReport {
        action_report: TorrentBlockerActionReport {
            blocked,
            ip,
            block_duration,
            will_unblock_at: Utc::now() + Duration::seconds(block_duration),
            user_id: email.to_string(),
            processed_at: Utc::now(),
        },
        xray_report: webhook,
    };

    state.add_report(report.clone());

    if let Some(url) = state.webhook_url.clone() {
        match serde_json::to_value(&report) {
            Ok(body) => rw_core::webhook::send_webhook(&url, body),
            Err(e) => warn!("[TORRENT-BLOCKER] failed to serialize report: {e}"),
        }
    }

    debug!("[TORRENT-BLOCKER] report: {report:?}");
    Some(report)
}

pub fn extract_ip(source: Option<&str>) -> Option<String> {
    let source = source?;
    let source = source
        .strip_prefix("tcp:")
        .or_else(|| source.strip_prefix("udp:"))
        .unwrap_or(source);

    let candidate = if let Some(rest) = source.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else {
        match source.rsplit_once(':') {
            Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
            _ => source,
        }
    };

    candidate.parse::<IpAddr>().ok().map(|ip| ip.to_string())
}
