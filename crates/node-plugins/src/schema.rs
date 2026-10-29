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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TorrentBlockerPlugin {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, rename = "blockDuration")]
    pub block_duration: i64,
    #[serde(default, rename = "ignoreLists")]
    pub ignore_lists: TorrentBlockerIgnoreLists,
    #[serde(default, rename = "includeRuleTags")]
    pub include_rule_tags: Option<Vec<String>>,
    #[serde(default, rename = "webhookUrl")]
    pub webhook_url: Option<String>,
    #[serde(default, rename = "rulePlacement")]
    pub rule_placement: Option<f64>,
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
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, rename = "whitelistIps")]
    pub whitelist_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IngressFilterPlugin {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, rename = "blockedIps")]
    pub blocked_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EgressFilterPlugin {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, rename = "blockedIps")]
    pub blocked_ips: Option<Vec<String>>,
    #[serde(default, rename = "blockedPorts")]
    pub blocked_ports: Option<Vec<u16>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CleanupSockets {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PreStartPlugin {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, rename = "cleanupSockets")]
    pub cleanup_sockets: Option<CleanupSockets>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PostStartWebhook {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PostStartPlugin {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub webhook: Option<PostStartWebhook>,
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
        serde_json::from_value(Value::Object(config.clone()))
    }
}
