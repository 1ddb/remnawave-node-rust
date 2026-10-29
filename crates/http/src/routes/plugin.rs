use axum::extract::State;
use axum::Json;
use rw_contract::commands::*;

use crate::state::AppState;

pub async fn sync(
    State(state): State<AppState>,
    Json(body): Json<SyncRequest>,
) -> Json<AcceptedResponse> {
    let accepted = state.xray.sync_plugins(body.plugin).await;
    Json(AcceptedResponse {
        response: AcceptedResponseData { accepted },
    })
}

pub async fn collect_reports(State(state): State<AppState>) -> Json<TorrentBlockerReportsResponse> {
    let reports = state.xray.collect_reports().await;
    Json(TorrentBlockerReportsResponse {
        response: TorrentBlockerReportsResponseData { reports },
    })
}

pub async fn block_ips(
    State(state): State<AppState>,
    Json(body): Json<BlockIpsRequest>,
) -> Json<AcceptedResponse> {
    let ips = body.ips.into_iter().map(|ip| (ip.ip, ip.timeout)).collect();
    let accepted = state.xray.block_ips(ips).await;
    Json(AcceptedResponse {
        response: AcceptedResponseData { accepted },
    })
}

pub async fn unblock_ips(
    State(state): State<AppState>,
    Json(body): Json<UnblockIpsRequest>,
) -> Json<AcceptedResponse> {
    let accepted = state.xray.unblock_ips(body.ips).await;
    Json(AcceptedResponse {
        response: AcceptedResponseData { accepted },
    })
}

pub async fn recreate_tables(State(state): State<AppState>) -> Json<AcceptedResponse> {
    let accepted = state.xray.recreate_tables().await;
    Json(AcceptedResponse {
        response: AcceptedResponseData { accepted },
    })
}
