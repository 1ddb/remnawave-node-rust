use axum::extract::State;
use axum::Json;
use rw_contract::commands::*;
use tracing::info;

use crate::state::AppState;

pub async fn start(
    State(state): State<AppState>,
    Json(body): Json<StartXrayRequest>,
) -> Json<StartXrayResponse> {
    let data = state.xray.start_xray(body).await;
    Json(StartXrayResponse { response: data })
}

pub async fn stop(State(state): State<AppState>) -> Json<StopXrayResponse> {
    info!("Remnawave requested to stop Xray.");
    let is_stopped = state.xray.stop_xray(true, false).await;
    Json(StopXrayResponse {
        response: StopXrayResponseData { is_stopped },
    })
}

pub async fn healthcheck(State(state): State<AppState>) -> Json<NodeHealthCheckResponse> {
    let data = state.xray.get_node_health_check().await;
    Json(NodeHealthCheckResponse { response: data })
}
