use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkInterface {
    pub interface: String,
    #[serde(rename = "rxBytesPerSec")]
    pub rx_bytes_per_sec: f64,
    #[serde(rename = "txBytesPerSec")]
    pub tx_bytes_per_sec: f64,
    #[serde(rename = "rxTotal")]
    pub rx_total: f64,
    #[serde(rename = "txTotal")]
    pub tx_total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeSystemInfo {
    pub arch: String,
    pub cpus: i64,
    #[serde(rename = "cpuModel")]
    pub cpu_model: String,
    #[serde(rename = "memoryTotal")]
    pub memory_total: f64,
    pub hostname: String,
    pub platform: String,
    pub release: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub version: String,
    #[serde(rename = "networkInterfaces")]
    pub network_interfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeSystemStats {
    #[serde(rename = "memoryFree")]
    pub memory_free: f64,
    #[serde(rename = "memoryUsed")]
    pub memory_used: f64,
    pub uptime: f64,
    #[serde(rename = "loadAvg")]
    pub load_avg: Vec<f64>,
    pub interface: Option<NetworkInterface>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeSystem {
    pub info: NodeSystemInfo,
    pub stats: NodeSystemStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeMetadata {
    pub name: String,
    pub uuid: String,
    pub id: i64,
    pub tags: Vec<String>,
    #[serde(rename = "countryCode")]
    pub country_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XrayWebhook {
    pub email: Option<String>,
    pub level: Option<i64>,
    pub protocol: Option<String>,
    pub network: String,
    pub source: Option<String>,
    pub destination: String,
    #[serde(rename = "routeTarget")]
    pub route_target: Option<String>,
    #[serde(rename = "originalTarget")]
    pub original_target: Option<String>,
    #[serde(rename = "inboundTag")]
    pub inbound_tag: Option<String>,
    #[serde(rename = "inboundName")]
    pub inbound_name: Option<String>,
    #[serde(rename = "inboundLocal")]
    pub inbound_local: Option<String>,
    #[serde(rename = "outboundTag")]
    pub outbound_tag: Option<String>,
    pub ts: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TorrentBlockerActionReport {
    pub blocked: bool,
    pub ip: String,
    #[serde(rename = "blockDuration")]
    pub block_duration: i64,
    #[serde(rename = "willUnblockAt", with = "crate::time::rfc3339_millis")]
    pub will_unblock_at: DateTime<Utc>,
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "processedAt", with = "crate::time::rfc3339_millis")]
    pub processed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TorrentBlockerReport {
    #[serde(rename = "actionReport")]
    pub action_report: TorrentBlockerActionReport,
    #[serde(rename = "xrayReport")]
    pub xray_report: XrayWebhook,
}
