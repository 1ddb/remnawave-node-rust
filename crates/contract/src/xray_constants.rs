use serde_json::{json, Map, Value};

pub const XRAY_TORRENT_BLOCKER_OUTBOUND_TAG: &str = "RW_TB_OUTBOUND_BLOCK";
pub const XRAY_API_TAG: &str = "REMNAWAVE_API";
pub const XRAY_API_INBOUND_TAG: &str = "REMNAWAVE_API_INBOUND";

pub fn default_stats_model() -> Value {
    json!({ "stats": {} })
}

pub fn default_api_model() -> Value {
    json!({
        "api": {
            "services": ["HandlerService", "StatsService", "RoutingService"],
            "tag": XRAY_API_TAG,
        }
    })
}

pub fn api_inbound_model(xtls_api_socket_path: &str) -> Value {
    json!({
        "tag": XRAY_API_INBOUND_TAG,
        "listen": format!("@{xtls_api_socket_path}"),
        "protocol": "tunnel",
    })
}

pub fn routing_rules_model() -> Value {
    json!({
        "inboundTag": [XRAY_API_INBOUND_TAG],
        "outboundTag": XRAY_API_TAG,
    })
}

pub fn torrent_blocker_routing_rules_model(webhook_url: &str) -> Value {
    json!({
        "protocol": ["bittorrent"],
        "outboundTag": XRAY_TORRENT_BLOCKER_OUTBOUND_TAG,
        "webhook": {
            "url": webhook_url,
            "deduplication": 5,
        },
    })
}

pub fn torrent_blocker_outbound_model() -> Value {
    json!({
        "tag": XRAY_TORRENT_BLOCKER_OUTBOUND_TAG,
        "protocol": "blackhole",
    })
}

pub fn default_policy(stats_user_online: bool) -> Value {
    json!({
        "levels": {
            "0": {
                "statsUserUplink": true,
                "statsUserDownlink": true,
                "statsUserOnline": stats_user_online,
            }
        },
        "system": {
            "statsInboundDownlink": true,
            "statsInboundUplink": true,
            "statsOutboundDownlink": true,
            "statsOutboundUplink": true,
        }
    })
}

pub fn merge_policy(existing: Option<&Value>, stats_user_online: bool) -> Value {
    let mut built = default_policy(stats_user_online);
    if let Some(existing) = existing {
        if let Some(level0) =
            existing.get("levels").and_then(|l| l.get("0")).and_then(Value::as_object)
        {
            if let Some(levels) = built
                .get_mut("levels")
                .and_then(Value::as_object_mut)
                .and_then(|l| l.get_mut("0"))
                .and_then(Value::as_object_mut)
            {
                let mut merged: Map<String, Value> = level0.clone();
                for (k, v) in levels.clone() {
                    merged.insert(k, v);
                }
                *levels = merged;
            }
        }
    }
    built
}
