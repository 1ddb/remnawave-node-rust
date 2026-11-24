use std::collections::HashSet;

use rw_contract::xray_constants::{
    api_inbound_model, default_api_model, default_stats_model, merge_policy, routing_rules_model,
    torrent_blocker_outbound_model, torrent_blocker_routing_rules_model, XRAY_API_TAG,
};
use serde_json::{json, Map, Value};

pub const XRAY_INTERNAL_FULL_WEBHOOK_PATH: &str = "/internal/webhook";

pub struct TorrentBlockerStateView {
    pub enabled: bool,
    pub include_rule_tags: HashSet<String>,
    pub rule_position: f64,
}

pub struct InternalView {
    pub socket_path: String,
    pub token: String,
    pub xtls_api_socket_path: String,
}

pub fn generate_api_config(
    config: Map<String, Value>,
    torrent_blocker: &TorrentBlockerStateView,
    internal: &InternalView,
    has_cap_net_admin: bool,
) -> Value {
    let mut result = config.clone();

    if let Value::Object(stats) = default_stats_model() {
        for (k, v) in stats {
            result.insert(k, v);
        }
    }
    if let Value::Object(api) = default_api_model() {
        for (k, v) in api {
            result.insert(k, v);
        }
    }

    let api_inbound = api_inbound_model(&internal.xtls_api_socket_path);
    let mut inbounds = vec![api_inbound];
    if let Some(Value::Array(existing)) = config.get("inbounds") {
        inbounds.extend(existing.iter().cloned());
    }
    result.insert("inbounds".into(), Value::Array(inbounds));

    let outbounds = config.get("outbounds").and_then(Value::as_array).cloned().unwrap_or_default();
    result.insert("outbounds".into(), Value::Array(outbounds));

    let policy = merge_policy(config.get("policy"), has_cap_net_admin);
    result.insert("policy".into(), policy);

    let existing_rules = config
        .get("routing")
        .and_then(|r| r.get("rules"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut rules = vec![routing_rules_model()];
    rules.extend(
        existing_rules
            .into_iter()
            .filter(|rule| rule.get("outboundTag").and_then(Value::as_str) != Some(XRAY_API_TAG)),
    );

    let mut routing = config.get("routing").and_then(Value::as_object).cloned().unwrap_or_default();
    routing.insert("rules".into(), Value::Array(rules));
    result.insert("routing".into(), Value::Object(routing));

    if torrent_blocker.enabled {
        let webhook_url = build_webhook_url(internal);
        let outbound = torrent_blocker_outbound_model();
        if let Some(Value::Array(outbounds)) = result.get_mut("outbounds") {
            outbounds.push(outbound);
        }

        let rule = torrent_blocker_routing_rules_model(&webhook_url);
        if let Some(rules) =
            result.get_mut("routing").and_then(|r| r.get_mut("rules")).and_then(Value::as_array_mut)
        {
            let index = resolve_rule_index(torrent_blocker.rule_position, rules.len());
            rules.insert(index, rule);

            if !torrent_blocker.include_rule_tags.is_empty() {
                for rule in rules.iter_mut() {
                    let matches = rule
                        .get("ruleTag")
                        .and_then(Value::as_str)
                        .is_some_and(|tag| torrent_blocker.include_rule_tags.contains(tag));
                    if matches {
                        if let Some(obj) = rule.as_object_mut() {
                            obj.insert(
                                "webhook".into(),
                                json!({ "url": webhook_url, "deduplication": 5 }),
                            );
                        }
                    }
                }
            }
        }
    }

    Value::Object(result)
}

fn resolve_rule_index(position: f64, rules_len: usize) -> usize {
    let user_rules_count = rules_len.saturating_sub(1);
    if !position.is_finite() || position.fract() != 0.0 || position <= 0.0 {
        return 1;
    }
    1 + (position as usize).min(user_rules_count)
}

fn build_webhook_url(internal: &InternalView) -> String {
    format!(
        "@{}:{}?token={}",
        internal.socket_path, XRAY_INTERNAL_FULL_WEBHOOK_PATH, internal.token
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn internal() -> InternalView {
        InternalView {
            socket_path: "rwint-test".into(),
            token: "token".into(),
            xtls_api_socket_path: "xtls-api-test".into(),
        }
    }

    #[test]
    fn injects_api_inbound_and_policy() {
        let mut config = Map::new();
        config.insert("inbounds".into(), json!([{ "tag": "user-in", "protocol": "vless" }]));
        let state = TorrentBlockerStateView {
            enabled: false,
            include_rule_tags: Default::default(),
            rule_position: 0.0,
        };
        let result = generate_api_config(config, &state, &internal(), true);
        let inbounds = result["inbounds"].as_array().unwrap();
        assert_eq!(inbounds[0]["tag"], "REMNAWAVE_API_INBOUND");
        assert_eq!(inbounds[0]["listen"], "@xtls-api-test");
        assert_eq!(inbounds[1]["tag"], "user-in");
        assert_eq!(result["api"]["tag"], "REMNAWAVE_API");
        assert_eq!(result["stats"], json!({}));
        assert_eq!(result["policy"]["levels"]["0"]["statsUserOnline"], true);
        assert_eq!(result["routing"]["rules"][0]["outboundTag"], "REMNAWAVE_API");
    }

    #[test]
    fn inserts_torrent_blocker_rule() {
        let mut config = Map::new();
        config
            .insert("routing".into(), json!({ "rules": [{ "ruleTag": "a" }, { "ruleTag": "b" }] }));
        config.insert("outbounds".into(), json!([]));
        let state = TorrentBlockerStateView {
            enabled: true,
            include_rule_tags: ["a".to_string()].into_iter().collect(),
            rule_position: 1.0,
        };
        let result = generate_api_config(config, &state, &internal(), false);
        let rules = result["routing"]["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 4);
        assert_eq!(rules[2]["protocol"], json!(["bittorrent"]));
        assert!(rules[1]["webhook"].is_object());
        assert_eq!(
            result["outbounds"].as_array().unwrap().last().unwrap()["protocol"],
            "blackhole"
        );
    }

    #[test]
    fn resolves_rule_index_like_node() {
        assert_eq!(resolve_rule_index(0.0, 4), 1);
        assert_eq!(resolve_rule_index(-1.0, 4), 1);
        assert_eq!(resolve_rule_index(2.5, 4), 1);
        assert_eq!(resolve_rule_index(f64::NAN, 4), 1);
        assert_eq!(resolve_rule_index(f64::INFINITY, 4), 1);
        assert_eq!(resolve_rule_index(1.0, 4), 2);
        assert_eq!(resolve_rule_index(10.0, 4), 4);
    }
}
