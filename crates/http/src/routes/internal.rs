use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use rw_contract::models::XrayWebhook;
use serde_json::Value;
use tracing::error;

use crate::extract::ContractJson;
use crate::state::AppState;

pub async fn get_xray_config(State(state): State<AppState>) -> Json<Value> {
    Json(state.xray.internal().get_xray_config().await)
}

pub async fn webhook(
    State(state): State<AppState>,
    ContractJson(body): ContractJson<Value>,
) -> StatusCode {
    match serde_json::from_value::<XrayWebhook>(body) {
        Ok(webhook) => {
            tokio::spawn(async move {
                state.xray.handle_xray_webhook(webhook).await;
            });
        }
        Err(e) => error!("Invalid webhook: {e}"),
    }
    StatusCode::OK
}
