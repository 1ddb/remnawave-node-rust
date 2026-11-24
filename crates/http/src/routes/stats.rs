use std::collections::BTreeMap;

use axum::extract::State;
use axum::http::Uri;
use axum::Json;
use chrono::{DateTime, Utc};
use rw_contract::commands::*;
use rw_contract::errors;
use rw_xray_rpc::stats_proto::Stat;

use crate::error::ApiError;
use crate::extract::ContractJson;
use crate::state::AppState;

pub async fn user_online_status(
    State(state): State<AppState>,
    ContractJson(body): ContractJson<UserOnlineStatusRequest>,
) -> Result<Json<UserOnlineStatusResponse>, ApiError> {
    let online = match state.xray.rpc_for_handler().await {
        Some(rpc) => rpc
            .get_stats_online(&format!("user>>>{}>>>online", body.username))
            .await
            .unwrap_or(false),
        None => false,
    };

    Ok(Json(UserOnlineStatusResponse {
        response: UserOnlineStatusResponseData { is_online: online },
    }))
}

pub async fn users_stats(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<ResetRequest>,
) -> Result<Json<UsersStatsResponse>, ApiError> {
    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::new(&uri, errors::FAILED_TO_GET_USERS_STATS, None))?;

    let stats = rpc
        .query_stats("user>>>", body.reset)
        .await
        .map_err(|e| ApiError::new(&uri, errors::FAILED_TO_GET_USERS_STATS, Some(e.message)))?;

    let mut map: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for stat in stats {
        let Some((_, username, direction)) = stat_parts(&stat.name) else {
            continue;
        };
        let entry = map.entry(username.to_string()).or_default();
        match direction {
            "uplink" => entry.0 += stat.value,
            "downlink" => entry.1 += stat.value,
            _ => {}
        }
    }

    let users = map
        .into_iter()
        .filter(|(_, (up, down))| *up != 0 || *down != 0)
        .map(|(username, (up, down))| UserStat { username, uplink: up, downlink: down })
        .collect();

    Ok(Json(UsersStatsResponse { response: UsersStatsResponseData { users } }))
}

pub async fn inbound_stats(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<InboundStatsRequest>,
) -> Result<Json<InboundStatsResponse>, ApiError> {
    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::new(&uri, errors::FAILED_TO_GET_INBOUND_STATS, None))?;

    let stats = rpc
        .query_stats(&format!("inbound>>>{}>>>", body.tag), body.reset)
        .await
        .map_err(|e| ApiError::new(&uri, errors::FAILED_TO_GET_INBOUND_STATS, Some(e.message)))?;

    if stats.is_empty() {
        return Err(ApiError::new(&uri, errors::FAILED_TO_GET_INBOUND_STATS, None));
    }

    let (up, down) = aggregate(stats);
    Ok(Json(InboundStatsResponse {
        response: InboundStatsResponseData { inbound: body.tag, downlink: down, uplink: up },
    }))
}

pub async fn outbound_stats(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<OutboundStatsRequest>,
) -> Result<Json<OutboundStatsResponse>, ApiError> {
    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::new(&uri, errors::FAILED_TO_GET_OUTBOUND_STATS, None))?;

    let stats = rpc
        .query_stats(&format!("outbound>>>{}>>>", body.tag), body.reset)
        .await
        .map_err(|e| ApiError::new(&uri, errors::FAILED_TO_GET_OUTBOUND_STATS, Some(e.message)))?;

    if stats.is_empty() {
        return Err(ApiError::new(&uri, errors::FAILED_TO_GET_OUTBOUND_STATS, None));
    }

    let (up, down) = aggregate(stats);
    Ok(Json(OutboundStatsResponse {
        response: OutboundStatsResponseData { outbound: body.tag, downlink: down, uplink: up },
    }))
}

pub async fn all_inbounds_stats(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<ResetRequest>,
) -> Result<Json<AllInboundsStatsResponse>, ApiError> {
    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::new(&uri, errors::FAILED_TO_GET_INBOUNDS_STATS, None))?;

    let stats = rpc
        .query_stats("inbound>>>", body.reset)
        .await
        .map_err(|e| ApiError::new(&uri, errors::FAILED_TO_GET_INBOUNDS_STATS, Some(e.message)))?;

    let inbounds = aggregate_by(stats, "inbound")
        .into_iter()
        .map(|(inbound, (uplink, downlink))| InboundStat { inbound, downlink, uplink })
        .collect();

    Ok(Json(AllInboundsStatsResponse { response: AllInboundsStatsResponseData { inbounds } }))
}

pub async fn all_outbounds_stats(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<ResetRequest>,
) -> Result<Json<AllOutboundsStatsResponse>, ApiError> {
    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::new(&uri, errors::FAILED_TO_GET_OUTBOUNDS_STATS, None))?;

    let stats = rpc
        .query_stats("outbound>>>", body.reset)
        .await
        .map_err(|e| ApiError::new(&uri, errors::FAILED_TO_GET_OUTBOUNDS_STATS, Some(e.message)))?;

    let outbounds = aggregate_by(stats, "outbound")
        .into_iter()
        .map(|(outbound, (uplink, downlink))| OutboundStat { outbound, downlink, uplink })
        .collect();

    Ok(Json(AllOutboundsStatsResponse { response: AllOutboundsStatsResponseData { outbounds } }))
}

pub async fn combined_stats(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<ResetRequest>,
) -> Result<Json<CombinedStatsResponse>, ApiError> {
    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::new(&uri, errors::FAILED_TO_GET_COMBINED_STATS, None))?;

    let inbounds_stats = rpc
        .query_stats("inbound>>>", body.reset)
        .await
        .map_err(|e| ApiError::new(&uri, errors::FAILED_TO_GET_COMBINED_STATS, Some(e.message)))?;
    let outbounds_stats = rpc
        .query_stats("outbound>>>", body.reset)
        .await
        .map_err(|e| ApiError::new(&uri, errors::FAILED_TO_GET_COMBINED_STATS, Some(e.message)))?;

    let inbounds = aggregate_by(inbounds_stats, "inbound")
        .into_iter()
        .map(|(inbound, (uplink, downlink))| InboundStat { inbound, downlink, uplink })
        .collect();
    let outbounds = aggregate_by(outbounds_stats, "outbound")
        .into_iter()
        .map(|(outbound, (uplink, downlink))| OutboundStat { outbound, downlink, uplink })
        .collect();

    Ok(Json(CombinedStatsResponse { response: CombinedStatsResponseData { inbounds, outbounds } }))
}

pub async fn system_stats(
    State(state): State<AppState>,
    uri: Uri,
) -> Result<Json<SystemStatsResponse>, ApiError> {
    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::new(&uri, errors::FAILED_TO_GET_SYSTEM_STATS, None))?;

    let sys = rpc
        .get_sys_stats()
        .await
        .map_err(|e| ApiError::new(&uri, errors::FAILED_TO_GET_SYSTEM_STATS, Some(e.message)))?;

    let interface = state.network.get_default().await.map(|r| r.to_contract());

    let mut stats = rw_core::system_stats::get_system_stats();
    stats.interface = interface;

    let reports_count = {
        let plugin_state = state.xray.plugin_state();
        let guard = plugin_state.lock().await;
        guard.torrent_blocker.reports_count()
    };

    Ok(Json(SystemStatsResponse {
        response: SystemStatsResponseData {
            xray_info: Some(XraySysStats {
                num_goroutine: sys.num_goroutine,
                num_gc: sys.num_gc,
                alloc: sys.alloc,
                total_alloc: sys.total_alloc,
                sys: sys.sys,
                mallocs: sys.mallocs,
                frees: sys.frees,
                live_objects: sys.live_objects,
                pause_total_ns: sys.pause_total_ns,
                uptime: sys.uptime,
            }),
            plugins: PluginStats { torrent_blocker: TorrentBlockerPluginStats { reports_count } },
            system: SystemStatsInner { stats },
        },
    }))
}

pub async fn user_ip_list(
    State(state): State<AppState>,
    ContractJson(body): ContractJson<UserIpListRequest>,
) -> Result<Json<UserIpListResponse>, ApiError> {
    let ips = match state.xray.rpc_for_handler().await {
        Some(rpc) => rpc
            .get_stats_online_ip_list(&format!("user>>>{}>>>online", body.user_id), true)
            .await
            .unwrap_or_default(),
        None => Default::default(),
    };

    let mut ips: Vec<DetailedIp> = ips
        .into_iter()
        .map(|(ip, ts)| DetailedIp {
            ip,
            last_seen: DateTime::<Utc>::from_timestamp(ts, 0).unwrap_or_else(Utc::now),
        })
        .collect();
    ips.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));

    Ok(Json(UserIpListResponse { response: UserIpListResponseData { ips } }))
}

pub async fn users_ip_list(
    State(state): State<AppState>,
) -> Result<Json<UsersIpListResponse>, ApiError> {
    let users = match state.xray.rpc_for_handler().await {
        Some(rpc) => rpc
            .get_users_stats(false, false)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|user| UserIpListEntry {
                user_id: user.email,
                ips: user
                    .ips
                    .into_iter()
                    .map(|ip| DetailedIp {
                        ip: ip.ip,
                        last_seen: DateTime::<Utc>::from_timestamp(ip.last_seen, 0)
                            .unwrap_or_else(Utc::now),
                    })
                    .collect(),
            })
            .collect(),
        None => Vec::new(),
    };

    Ok(Json(UsersIpListResponse { response: UsersIpListResponseData { users } }))
}

/// Splits an Xray traffic metric name such as
/// `user>>>email>>>traffic>>>uplink` into `(kind, id, direction)`.
///
/// Xray (and `@remnawave/xtls-sdk`) reads `parts[1]` and `parts[3]`; the
/// `traffic` segment in between is always present for traffic counters.
fn stat_parts(name: &str) -> Option<(&str, &str, &str)> {
    let mut parts = name.split(">>>");
    let kind = parts.next()?;
    let id = parts.next()?;
    let _traffic = parts.next()?;
    let direction = parts.next()?;
    Some((kind, id, direction))
}

fn aggregate(stats: Vec<Stat>) -> (i64, i64) {
    let mut up = 0;
    let mut down = 0;
    for stat in stats {
        let Some((_, _, direction)) = stat_parts(&stat.name) else {
            continue;
        };
        match direction {
            "uplink" => up += stat.value,
            "downlink" => down += stat.value,
            _ => {}
        }
    }
    (up, down)
}

fn aggregate_by(stats: Vec<Stat>, prefix: &str) -> BTreeMap<String, (i64, i64)> {
    let mut map: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for stat in stats {
        let Some((kind, id, direction)) = stat_parts(&stat.name) else {
            continue;
        };
        if kind != prefix {
            continue;
        }
        let entry = map.entry(id.to_string()).or_default();
        match direction {
            "uplink" => entry.0 += stat.value,
            "downlink" => entry.1 += stat.value,
            _ => {}
        }
    }
    map
}

const GEOCHECK_TIMEOUT_MS: u64 = 45_000;
const GEOCHECK_MAX_OUTPUT: usize = 32 * 1024 * 1024;
const GEOCHECK_MAX_STDERR: usize = 1024 * 1024;

/// Releases the `geocheck_running` flag even if the handler future is dropped
/// (client disconnect, request cancellation, panic).
struct GeocheckGuard(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for GeocheckGuard {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

pub async fn geocheck(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<GeocheckRequest>,
) -> Result<Json<GeocheckResponse>, ApiError> {
    use std::net::IpAddr;
    use std::sync::atomic::Ordering;

    let ip = body.ip.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let iface = body.interface.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let bind_to = if let Some(ip) = ip {
        if ip.parse::<IpAddr>().is_err() {
            return Err(ApiError::new(
                &uri,
                errors::FAILED_TO_GET_GEOCHECK,
                Some(format!("Geocheck: \"{ip}\" is not a valid IP address.")),
            ));
        }
        Some(ip.to_string())
    } else {
        iface.map(str::to_string)
    };

    if state
        .geocheck_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(ApiError::new(
            &uri,
            errors::FAILED_TO_GET_GEOCHECK,
            Some("Geocheck: a run is already in progress.".into()),
        ));
    }
    let _guard = GeocheckGuard(state.geocheck_running.clone());

    let target = bind_to.clone().unwrap_or_else(|| "default route".into());
    let stdout = run_geocheck(bind_to, &target)
        .await
        .map_err(|message| ApiError::new(&uri, errors::FAILED_TO_GET_GEOCHECK, Some(message)))?;

    let report: GeocheckResponseData = serde_json::from_slice(&stdout).map_err(|e| {
        ApiError::new(
            &uri,
            errors::FAILED_TO_GET_GEOCHECK,
            Some(format!("Geocheck via {target} failed: {e}")),
        )
    })?;

    if report.image.data.is_empty() {
        return Err(ApiError::new(
            &uri,
            errors::FAILED_TO_GET_GEOCHECK,
            Some("geocheck report carries no image".into()),
        ));
    }

    Ok(Json(GeocheckResponse { response: report }))
}

async fn run_geocheck(bind_to: Option<String>, target: &str) -> Result<Vec<u8>, String> {
    use std::process::Stdio;

    let mut cmd = tokio::process::Command::new("/usr/local/bin/geocheck");
    if let Some(bind) = &bind_to {
        cmd.arg("--interface").arg(bind);
    }
    cmd.args(["--json", "--svg-base64", "--quiet"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| format!("Geocheck via {target} failed: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("Geocheck via {target} failed: stdout is not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("Geocheck via {target} failed: stderr is not piped"))?;

    let work = async {
        tokio::join!(
            read_capped(stdout, GEOCHECK_MAX_OUTPUT),
            read_capped(stderr, GEOCHECK_MAX_STDERR),
            child.wait()
        )
    };

    let (stdout_res, stderr_res, status_res) =
        match tokio::time::timeout(std::time::Duration::from_millis(GEOCHECK_TIMEOUT_MS), work)
            .await
        {
            Ok(result) => result,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(format!(
                    "Geocheck via {target} exceeded {GEOCHECK_TIMEOUT_MS}ms and was killed."
                ));
            }
        };

    let (stdout, stdout_exceeded) =
        stdout_res.map_err(|e| format!("Geocheck via {target} failed: {e}"))?;
    if stdout_exceeded {
        return Err("Geocheck output exceeds 32MB.".into());
    }

    let (stderr, _) = stderr_res.map_err(|e| format!("Geocheck via {target} failed: {e}"))?;
    let status = status_res.map_err(|e| format!("Geocheck via {target} failed: {e}"))?;
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        let detail = detail.trim();
        let detail: String = if detail.is_empty() {
            format!("exit status {status}")
        } else {
            detail.chars().take(500).collect()
        };
        return Err(format!("Geocheck via {target} failed: {detail}"));
    }

    Ok(stdout)
}

async fn read_capped<R>(mut reader: R, limit: usize) -> std::io::Result<(Vec<u8>, bool)>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut buffer = Vec::new();
    let mut limited = (&mut reader).take(limit as u64 + 1);
    limited.read_to_end(&mut buffer).await?;
    if buffer.len() > limit {
        buffer.truncate(limit);
        Ok((buffer, true))
    } else {
        Ok((buffer, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(name: &str, value: i64) -> Stat {
        Stat { name: name.to_string(), value }
    }

    #[test]
    fn parses_standard_xray_names() {
        assert_eq!(
            stat_parts("user>>>alice>>>traffic>>>uplink"),
            Some(("user", "alice", "uplink"))
        );
        assert_eq!(
            stat_parts("inbound>>>vless-in>>>traffic>>>downlink"),
            Some(("inbound", "vless-in", "downlink"))
        );
        assert_eq!(stat_parts("user>>>alice>>>online"), None);
    }

    #[test]
    fn aggregates_user_traffic() {
        let stats = vec![
            stat("user>>>alice>>>traffic>>>uplink", 10),
            stat("user>>>alice>>>traffic>>>uplink", 5),
            stat("user>>>alice>>>traffic>>>downlink", 7),
            stat("user>>>bob>>>traffic>>>downlink", 3),
        ];

        let (up, down) = aggregate(stats.clone());
        assert_eq!((up, down), (15, 10));

        let by_kind = aggregate_by(stats, "user");
        assert_eq!(by_kind.get("alice"), Some(&(15, 7)));
        assert_eq!(by_kind.get("bob"), Some(&(0, 3)));
    }
}
