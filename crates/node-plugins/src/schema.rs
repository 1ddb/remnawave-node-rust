use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum SharedList {
    #[serde(rename = "ipList")]
    IpList { name: String, items: Vec<String> },
    #[serde(rename = "asList")]
    AsList { name: String, items: Vec<u32> },
}

impl SharedList {
    pub fn name(&self) -> &str {
        match self {
            SharedList::IpList { name, .. } | SharedList::AsList { name, .. } => name,
        }
    }

    fn validate(&self) -> Result<(), String> {
        let name = self.name();
        if !name.starts_with("ext:") {
            return Err(format!("shared list \"{name}\" must start with \"ext:\""));
        }
        match self {
            SharedList::IpList { items, .. } => {
                for item in items {
                    if !is_ip_or_cidr(item) {
                        return Err(format!(
                            "shared list \"{name}\" contains invalid IP/CIDR \"{item}\""
                        ));
                    }
                }
            }
            SharedList::AsList { items, .. } => {
                if items.contains(&0) {
                    return Err(format!("shared list \"{name}\" contains ASN 0"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TorrentBlockerPlugin {
    pub enabled: bool,
    #[serde(rename = "blockDuration")]
    pub block_duration: i64,
    #[serde(rename = "ignoreLists")]
    pub ignore_lists: TorrentBlockerIgnoreLists,
    #[serde(default, rename = "includeRuleTags")]
    pub include_rule_tags: Option<Vec<String>>,
    #[serde(default, rename = "webhookUrl")]
    pub webhook_url: Option<String>,
    #[serde(default, rename = "rulePlacement")]
    pub rule_placement: Option<f64>,
}

impl TorrentBlockerPlugin {
    fn validate(&self) -> Result<(), String> {
        if let Some(ips) = &self.ignore_lists.ip {
            for ip in ips {
                if !is_ip_or_ext(ip) {
                    return Err(format!(
                        "torrentBlocker.ignoreLists.ip contains invalid IP \"{ip}\""
                    ));
                }
            }
        }
        if let Some(tags) = &self.include_rule_tags {
            if tags.is_empty() {
                return Err("torrentBlocker.includeRuleTags must not be empty".into());
            }
        }
        if let Some(url) = &self.webhook_url {
            if !is_url(url) {
                return Err(format!("torrentBlocker.webhookUrl is not a valid URL: {url}"));
            }
        }
        if let Some(position) = self.rule_placement {
            if !(0.0..=1000.0).contains(&position) {
                return Err("torrentBlocker.rulePlacement must be between 0 and 1000".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TorrentBlockerIgnoreLists {
    #[serde(default)]
    pub ip: Option<Vec<String>>,
    #[serde(default, rename = "userId")]
    pub user_id: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionDropPlugin {
    pub enabled: bool,
    #[serde(rename = "whitelistIps")]
    pub whitelist_ips: Vec<String>,
}

impl ConnectionDropPlugin {
    fn validate(&self) -> Result<(), String> {
        for ip in &self.whitelist_ips {
            if !is_ip_or_ext(ip) {
                return Err(format!("connectionDrop.whitelistIps contains invalid IP \"{ip}\""));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IngressFilterPlugin {
    pub enabled: bool,
    #[serde(rename = "blockedIps")]
    pub blocked_ips: Vec<String>,
}

impl IngressFilterPlugin {
    fn validate(&self) -> Result<(), String> {
        for ip in &self.blocked_ips {
            if !is_ip_cidr_or_ext(ip) {
                return Err(format!("ingressFilter.blockedIps contains invalid IP/CIDR \"{ip}\""));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EgressFilterPlugin {
    pub enabled: bool,
    #[serde(default, rename = "blockedIps")]
    pub blocked_ips: Option<Vec<String>>,
    #[serde(default, rename = "blockedPorts")]
    pub blocked_ports: Option<Vec<u16>>,
}

impl EgressFilterPlugin {
    fn validate(&self) -> Result<(), String> {
        if let Some(ips) = &self.blocked_ips {
            for ip in ips {
                if !is_ip_cidr_or_ext(ip) {
                    return Err(format!(
                        "egressFilter.blockedIps contains invalid IP/CIDR \"{ip}\""
                    ));
                }
            }
        }
        if let Some(ports) = &self.blocked_ports {
            if ports.contains(&0) {
                return Err("egressFilter.blockedPorts must be between 1 and 65535".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CleanupSockets {
    pub enabled: bool,
    pub files: Vec<String>,
}

impl CleanupSockets {
    fn validate(&self) -> Result<(), String> {
        if self.files.len() > 64 {
            return Err("preStart.cleanupSockets.files allows at most 64 entries".into());
        }
        for file in &self.files {
            let file = file.trim();
            if file.is_empty() || !file.starts_with('/') || file.contains('\0') {
                return Err(format!(
                    "preStart.cleanupSockets.files contains invalid path \"{file}\""
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PreStartPlugin {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, rename = "cleanupSockets")]
    pub cleanup_sockets: Option<CleanupSockets>,
}

impl PreStartPlugin {
    fn validate(&self) -> Result<(), String> {
        if let Some(cleanup) = &self.cleanup_sockets {
            cleanup.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PostStartWebhook {
    pub enabled: bool,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PostStartPlugin {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub webhook: Option<PostStartWebhook>,
}

impl PostStartPlugin {
    fn validate(&self) -> Result<(), String> {
        if let Some(webhook) = &self.webhook {
            if !is_http_url(&webhook.url) {
                return Err(format!(
                    "postStart.webhook.url must be a valid http(s) URL: {}",
                    webhook.url
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodePlugin {
    #[serde(default, rename = "sharedLists")]
    pub shared_lists: Vec<SharedList>,
    #[serde(default, rename = "torrentBlocker")]
    pub torrent_blocker: Option<TorrentBlockerPlugin>,
    #[serde(default, rename = "ingressFilter")]
    pub ingress_filter: Option<IngressFilterPlugin>,
    #[serde(default, rename = "egressFilter")]
    pub egress_filter: Option<EgressFilterPlugin>,
    #[serde(default, rename = "connectionDrop")]
    pub connection_drop: Option<ConnectionDropPlugin>,
    #[serde(default, rename = "preStart")]
    pub pre_start: Option<PreStartPlugin>,
    #[serde(default, rename = "postStart")]
    pub post_start: Option<PostStartPlugin>,
}

impl NodePlugin {
    pub fn parse(config: &serde_json::Map<String, Value>) -> Result<Self, serde_json::Error> {
        let mut plugin: NodePlugin = serde_json::from_value(Value::Object(config.clone()))?;
        plugin.normalize();
        Self::validate(&plugin).map_err(|message| {
            serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, message))
        })?;
        Ok(plugin)
    }

    fn normalize(&mut self) {
        if let Some(cleanup) =
            self.pre_start.as_mut().and_then(|plugin| plugin.cleanup_sockets.as_mut())
        {
            for file in &mut cleanup.files {
                *file = file.trim().to_string();
            }
        }
    }

    fn validate(plugin: &Self) -> Result<(), String> {
        for list in &plugin.shared_lists {
            list.validate()?;
        }
        if let Some(plugin) = &plugin.torrent_blocker {
            plugin.validate()?;
        }
        if let Some(plugin) = &plugin.ingress_filter {
            plugin.validate()?;
        }
        if let Some(plugin) = &plugin.egress_filter {
            plugin.validate()?;
        }
        if let Some(plugin) = &plugin.connection_drop {
            plugin.validate()?;
        }
        if let Some(plugin) = &plugin.pre_start {
            plugin.validate()?;
        }
        if let Some(plugin) = &plugin.post_start {
            plugin.validate()?;
        }
        Ok(())
    }
}

fn is_ip(value: &str) -> bool {
    value.parse::<IpAddr>().is_ok()
}

fn is_cidr(value: &str) -> bool {
    let Some((addr, prefix)) = value.split_once('/') else {
        return false;
    };
    let Ok(addr) = addr.parse::<IpAddr>() else {
        return false;
    };
    let Ok(prefix) = prefix.parse::<u32>() else {
        return false;
    };
    match addr {
        IpAddr::V4(_) => prefix <= 32,
        IpAddr::V6(_) => prefix <= 128,
    }
}

fn is_ip_or_cidr(value: &str) -> bool {
    is_ip(value) || is_cidr(value)
}

fn is_ext(value: &str) -> bool {
    value.starts_with("ext:")
}

fn is_ip_or_ext(value: &str) -> bool {
    is_ip(value) || is_ext(value)
}

fn is_ip_cidr_or_ext(value: &str) -> bool {
    is_ip_or_cidr(value) || is_ext(value)
}

fn is_url(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };
    !scheme.is_empty()
        && !rest.is_empty()
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn is_http_url(value: &str) -> bool {
    is_url(value) && (value.starts_with("http://") || value.starts_with("https://"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: Value) -> Result<NodePlugin, serde_json::Error> {
        NodePlugin::parse(value.as_object().unwrap())
    }

    #[test]
    fn accepts_valid_config() {
        let plugin = parse(serde_json::json!({
            "sharedLists": [{ "type": "ipList", "name": "ext:blocked", "items": ["1.1.1.1", "10.0.0.0/8"] }],
            "ingressFilter": { "enabled": true, "blockedIps": ["ext:blocked", "2001:db8::/32"] },
            "egressFilter": { "enabled": true, "blockedIps": ["1.2.3.4"], "blockedPorts": [25, 465] },
            "torrentBlocker": {
                "enabled": true,
                "blockDuration": 60,
                "ignoreLists": { "ip": ["8.8.8.8", "ext:blocked"], "userId": [1, 2] },
                "includeRuleTags": ["a"],
                "rulePlacement": 1
            },
            "preStart": { "enabled": true, "cleanupSockets": { "enabled": true, "files": ["/tmp/*.sock"] } },
            "postStart": { "enabled": true, "webhook": { "enabled": true, "url": "https://example.com/hook" } }
        }))
        .unwrap();
        assert!(plugin.torrent_blocker.is_some());
    }

    #[test]
    fn rejects_invalid_ip() {
        assert!(parse(serde_json::json!({
            "ingressFilter": { "enabled": true, "blockedIps": ["1.2.3.4/8 } flush ruleset"] }
        }))
        .is_err());
    }

    #[test]
    fn rejects_non_ext_shared_list_and_empty_tags() {
        assert!(parse(serde_json::json!({
            "sharedLists": [{ "type": "ipList", "name": "blocked", "items": ["1.1.1.1"] }]
        }))
        .is_err());
        assert!(parse(serde_json::json!({
            "torrentBlocker": {
                "enabled": true,
                "blockDuration": 60,
                "ignoreLists": {},
                "includeRuleTags": []
            }
        }))
        .is_err());
    }

    #[test]
    fn rejects_bad_ports_paths_and_urls() {
        assert!(parse(serde_json::json!({
            "egressFilter": { "enabled": true, "blockedPorts": [0] }
        }))
        .is_err());
        assert!(parse(serde_json::json!({
            "preStart": { "enabled": true, "cleanupSockets": { "enabled": true, "files": ["tmp/x.sock"] } }
        }))
        .is_err());
        assert!(parse(serde_json::json!({
            "postStart": { "enabled": true, "webhook": { "enabled": true, "url": "ftp://example.com" } }
        }))
        .is_err());
    }
}
