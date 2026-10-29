use axum::extract::State;
use axum::http::Uri;
use axum::Json;
use rw_contract::commands::*;
use rw_core::capabilities::has_cap_net_admin;

use crate::error::ApiError;
use crate::extract::ContractJson;
use crate::state::AppState;

async fn user_ips(state: &AppState, user_id: &str) -> Option<Vec<String>> {
    if !has_cap_net_admin() {
        return None;
    }
    let rpc = state.xray.rpc_for_handler().await?;
    let ips = rpc
        .get_stats_online_ip_list(&format!("user>>>{user_id}>>>online"), true)
        .await
        .ok()?;
    Some(ips.into_keys().collect())
}

pub async fn add_user(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<AddUserRequest>,
) -> Result<Json<AddUserResponse>, ApiError> {
    let Some(first) = body.data.first() else {
        return Ok(Json(AddUserResponse {
            response: AddUserResponseData {
                success: true,
                error: None,
            },
        }));
    };
    let user_id = first.tag_username();

    for item in &body.data {
        state
            .xray
            .internal()
            .add_xtls_config_inbound(item.tag())
            .await;
    }

    let prev = body.hash_data.prev_vless_uuid.clone();
    let user_ips = if prev.is_some() {
        user_ips(&state, &user_id).await
    } else {
        None
    };

    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::internal(&uri, "Xray RPC unavailable"))?;

    let inbound_tags = state.xray.internal().get_xtls_config_inbounds().await;
    for tag in &inbound_tags {
        let _ = rpc.remove_user(tag, &user_id).await;
        let hash = prev.as_deref().unwrap_or(&body.hash_data.vless_uuid);
        state
            .xray
            .internal()
            .remove_user_from_inbound(tag, hash)
            .await;
    }

    if let (Some(ips), Some(_)) = (&user_ips, &prev) {
        state.xray.drop_ips(ips.clone()).await;
    }

    let mut results: Vec<Result<bool, String>> = Vec::new();
    for item in &body.data {
        let result = match item {
            AddUserItem::Trojan {
                tag,
                username,
                password,
            } => rpc.add_trojan_user(tag, username, password, 0).await,
            AddUserItem::Vless {
                tag,
                username,
                uuid,
                flow,
            } => rpc.add_vless_user(tag, username, uuid, flow, 0).await,
            AddUserItem::Shadowsocks {
                tag,
                username,
                password,
                cipher_type,
                ..
            } => {
                rpc.add_shadowsocks_user(tag, username, password, *cipher_type as i32, false, 0)
                    .await
            }
            AddUserItem::Shadowsocks22 {
                tag,
                username,
                password,
            } => {
                rpc.add_shadowsocks2022_user(tag, username, password, 0)
                    .await
            }
            AddUserItem::Hysteria {
                tag,
                username,
                password,
            } => rpc.add_hysteria_user(tag, username, password, 0).await,
        };

        match result {
            Ok(ok) => {
                if ok {
                    if let Some(tag) = item.tag_option() {
                        state
                            .xray
                            .internal()
                            .add_user_to_inbound(tag, &body.hash_data.vless_uuid)
                            .await;
                    }
                }
                results.push(Ok(ok));
            }
            Err(e) => results.push(Err(e.message)),
        }
    }

    let all_failed = results.iter().all(|r| matches!(r, Err(_)));
    if all_failed {
        return Ok(Json(AddUserResponse {
            response: AddUserResponseData {
                success: false,
                error: results.iter().find_map(|r| r.as_ref().err().cloned()),
            },
        }));
    }

    Ok(Json(AddUserResponse {
        response: AddUserResponseData {
            success: true,
            error: None,
        },
    }))
}

pub async fn remove_user(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<RemoveUserRequest>,
) -> Result<Json<RemoveUserResponse>, ApiError> {
    let inbound_tags = state.xray.internal().get_xtls_config_inbounds().await;
    if inbound_tags.is_empty() {
        return Ok(Json(RemoveUserResponse {
            response: RemoveUserResponseData {
                success: true,
                error: None,
            },
        }));
    }

    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::internal(&uri, "Xray RPC unavailable"))?;

    let user_ips = user_ips(&state, &body.username).await;
    let mut results: Vec<Result<bool, String>> = Vec::new();

    for tag in &inbound_tags {
        results.push(
            rpc.remove_user(tag, &body.username)
                .await
                .map_err(|e| e.message),
        );
        state
            .xray
            .internal()
            .remove_user_from_inbound(tag, &body.hash_data.vless_uuid)
            .await;
    }

    if let Some(ips) = user_ips {
        state.xray.drop_ips(ips).await;
    }

    let all_failed = results.iter().all(|r| matches!(r, Err(_)));
    if all_failed {
        return Ok(Json(RemoveUserResponse {
            response: RemoveUserResponseData {
                success: false,
                error: results.iter().find_map(|r| r.as_ref().err().cloned()),
            },
        }));
    }

    Ok(Json(RemoveUserResponse {
        response: RemoveUserResponseData {
            success: true,
            error: None,
        },
    }))
}

pub async fn add_users(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<AddUsersRequest>,
) -> Result<Json<AddUsersResponse>, ApiError> {
    for tag in &body.affected_inbound_tags {
        state
            .xray
            .internal()
            .add_xtls_config_inbound(tag)
            .await;
    }

    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::internal(&uri, "Xray RPC unavailable"))?;

    for user in &body.users {
        let inbound_tags = state.xray.internal().get_xtls_config_inbounds().await;
        for tag in &inbound_tags {
            let _ = rpc.remove_user(tag, &user.user_data.user_id).await;
            state
                .xray
                .internal()
                .remove_user_from_inbound(tag, &user.user_data.hash_uuid)
                .await;
        }

        for item in &user.inbound_data {
            let result = match item {
                AddUsersInbound::Trojan { tag } => {
                    rpc.add_trojan_user(
                        tag,
                        &user.user_data.user_id,
                        &user.user_data.trojan_password,
                        0,
                    )
                    .await
                }
                AddUsersInbound::Vless { tag, flow } => {
                    rpc.add_vless_user(
                        tag,
                        &user.user_data.user_id,
                        &user.user_data.vless_uuid,
                        flow,
                        0,
                    )
                    .await
                }
                AddUsersInbound::Shadowsocks { tag } => {
                    rpc.add_shadowsocks_user(
                        tag,
                        &user.user_data.user_id,
                        &user.user_data.ss_password,
                        0,
                        false,
                        0,
                    )
                    .await
                }
                AddUsersInbound::Shadowsocks22 { tag } => {
                    let key = base64_encode(user.user_data.ss_password.as_bytes());
                    rpc.add_shadowsocks2022_user(
                        tag,
                        &user.user_data.user_id,
                        &key,
                        0,
                    )
                    .await
                }
                AddUsersInbound::Hysteria { tag } => {
                    rpc.add_hysteria_user(
                        tag,
                        &user.user_data.user_id,
                        &user.user_data.vless_uuid,
                        0,
                    )
                    .await
                }
            };

            if matches!(result, Ok(true)) {
                if let Some(tag) = inbound_tag(item) {
                    state
                        .xray
                        .internal()
                        .add_user_to_inbound(tag, &user.user_data.vless_uuid)
                        .await;
                }
            }
        }
    }

    Ok(Json(AddUsersResponse {
        response: AddUserResponseData {
            success: true,
            error: None,
        },
    }))
}

pub async fn remove_users(
    State(state): State<AppState>,
    uri: Uri,
    ContractJson(body): ContractJson<RemoveUsersRequest>,
) -> Result<Json<RemoveUsersResponse>, ApiError> {
    let inbound_tags = state.xray.internal().get_xtls_config_inbounds().await;
    if inbound_tags.is_empty() {
        return Ok(Json(RemoveUsersResponse {
            response: RemoveUserResponseData {
                success: true,
                error: None,
            },
        }));
    }

    let rpc = state
        .xray
        .rpc_for_handler()
        .await
        .ok_or_else(|| ApiError::internal(&uri, "Xray RPC unavailable"))?;

    let mut results: Vec<Result<bool, String>> = Vec::new();
    for user in &body.users {
        let user_ips = user_ips(&state, &user.user_id).await;
        for tag in &inbound_tags {
            results.push(
                rpc.remove_user(tag, &user.user_id)
                    .await
                    .map_err(|e| e.message),
            );
            state
                .xray
                .internal()
                .remove_user_from_inbound(tag, &user.hash_uuid)
                .await;
        }
        if let Some(ips) = user_ips {
            state.xray.drop_ips(ips).await;
        }
    }

    let all_failed = results.iter().all(|r| matches!(r, Err(_)));
    if all_failed {
        return Ok(Json(RemoveUsersResponse {
            response: RemoveUserResponseData {
                success: false,
                error: results.iter().find_map(|r| r.as_ref().err().cloned()),
            },
        }));
    }

    Ok(Json(RemoveUsersResponse {
        response: RemoveUserResponseData {
            success: true,
            error: None,
        },
    }))
}

pub async fn drop_users_connections(
    State(state): State<AppState>,
    ContractJson(body): ContractJson<DropUsersConnectionsRequest>,
) -> Result<Json<GenericSuccessResponse>, ApiError> {
    for user_id in &body.user_ids {
        if let Some(ips) = user_ips(&state, user_id).await {
            state.xray.drop_ips(ips).await;
        }
    }
    Ok(Json(GenericSuccessResponse {
        response: GenericSuccessResponseData { success: true },
    }))
}

pub async fn drop_ips(
    State(state): State<AppState>,
    ContractJson(body): ContractJson<DropIpsRequest>,
) -> Result<Json<GenericSuccessResponse>, ApiError> {
    state.xray.drop_ips(body.ips).await;
    Ok(Json(GenericSuccessResponse {
        response: GenericSuccessResponseData { success: true },
    }))
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn inbound_tag(item: &AddUsersInbound) -> Option<&String> {
    match item {
        AddUsersInbound::Trojan { tag }
        | AddUsersInbound::Vless { tag, .. }
        | AddUsersInbound::Shadowsocks { tag }
        | AddUsersInbound::Shadowsocks22 { tag }
        | AddUsersInbound::Hysteria { tag } => Some(tag),
    }
}

trait AddUserItemExt {
    fn tag_username(&self) -> String;
    fn tag(&self) -> &String;
    fn tag_option(&self) -> Option<&String>;
}

impl AddUserItemExt for AddUserItem {
    fn tag_username(&self) -> String {
        match self {
            AddUserItem::Trojan { username, .. }
            | AddUserItem::Vless { username, .. }
            | AddUserItem::Shadowsocks { username, .. }
            | AddUserItem::Shadowsocks22 { username, .. }
            | AddUserItem::Hysteria { username, .. } => username.clone(),
        }
    }

    fn tag(&self) -> &String {
        match self {
            AddUserItem::Trojan { tag, .. }
            | AddUserItem::Vless { tag, .. }
            | AddUserItem::Shadowsocks { tag, .. }
            | AddUserItem::Shadowsocks22 { tag, .. }
            | AddUserItem::Hysteria { tag, .. } => tag,
        }
    }

    fn tag_option(&self) -> Option<&String> {
        Some(self.tag())
    }
}
