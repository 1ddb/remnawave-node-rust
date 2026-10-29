use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::models::{
    NetworkInterface, NodeMetadata, NodeSystemInfo, NodeSystemStats, TorrentBlockerReport,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyResponse {
    pub response: Value,
}

// ---------------------------------------------------------------------------
// Xray
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Hashes {
    pub empty_config: String,
    pub inbounds: Vec<InboundHash>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboundHash {
    pub users_count: i64,
    pub hash: String,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartXrayInternals {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<NodeMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrations: Option<Map<String, Value>>,
    #[serde(default)]
    pub force_restart: bool,
    pub hashes: Hashes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartXrayRequest {
    pub internals: StartXrayInternals,
    pub xray_config: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartXraySystem {
    pub info: NodeSystemInfo,
    pub stats: NodeSystemStats,
    pub interface: Option<NetworkInterface>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInformation {
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartXrayResponseData {
    pub is_started: bool,
    pub version: Option<String>,
    pub error: Option<String>,
    pub node_information: NodeInformation,
    pub system: StartXraySystem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartXrayResponse {
    pub response: StartXrayResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopXrayResponseData {
    #[serde(rename = "isStopped")]
    pub is_stopped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopXrayResponse {
    pub response: StopXrayResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeHealthCheckResponseData {
    pub is_alive: bool,
    pub xray_internal_status_cached: bool,
    pub xray_version: Option<String>,
    pub node_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealthCheckResponse {
    pub response: NodeHealthCheckResponseData,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AddUserItem {
    Trojan {
        tag: String,
        username: String,
        password: String,
    },
    Vless {
        tag: String,
        username: String,
        uuid: String,
        flow: String,
    },
    Shadowsocks {
        tag: String,
        username: String,
        password: String,
        #[serde(rename = "cipherType")]
        cipher_type: i64,
        #[serde(rename = "ivCheck")]
        iv_check: bool,
    },
    Shadowsocks22 {
        tag: String,
        username: String,
        password: String,
    },
    Hysteria {
        tag: String,
        username: String,
        password: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddUserHashData {
    pub vless_uuid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_vless_uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddUserRequest {
    pub data: Vec<AddUserItem>,
    pub hash_data: AddUserHashData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddUserResponseData {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddUserResponse {
    pub response: AddUserResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveUserHashData {
    #[serde(rename = "vlessUuid")]
    pub vless_uuid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveUserRequest {
    pub username: String,
    pub hash_data: RemoveUserHashData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveUserResponseData {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveUserResponse {
    pub response: RemoveUserResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AddUsersInbound {
    Trojan { tag: String },
    Vless { tag: String, flow: String },
    Shadowsocks { tag: String },
    Shadowsocks22 { tag: String },
    Hysteria { tag: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddUsersUserData {
    pub user_id: String,
    pub hash_uuid: String,
    pub vless_uuid: String,
    pub trojan_password: String,
    pub ss_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddUsersUser {
    pub inbound_data: Vec<AddUsersInbound>,
    pub user_data: AddUsersUserData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddUsersRequest {
    pub affected_inbound_tags: Vec<String>,
    pub users: Vec<AddUsersUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddUsersResponse {
    pub response: AddUserResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveUsersUser {
    pub user_id: String,
    pub hash_uuid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveUsersRequest {
    pub users: Vec<RemoveUsersUser>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveUsersResponse {
    pub response: RemoveUserResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropUsersConnectionsRequest {
    #[serde(rename = "userIds")]
    pub user_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropIpsRequest {
    pub ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericSuccessResponseData {
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericSuccessResponse {
    pub response: GenericSuccessResponseData,
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetRequest {
    pub reset: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundStatsRequest {
    pub tag: String,
    pub reset: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundStatsRequest {
    pub tag: String,
    pub reset: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOnlineStatusRequest {
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOnlineStatusResponseData {
    #[serde(rename = "isOnline")]
    pub is_online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserOnlineStatusResponse {
    pub response: UserOnlineStatusResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIpListRequest {
    #[serde(rename = "userId")]
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedIp {
    pub ip: String,
    #[serde(rename = "lastSeen", with = "crate::time::rfc3339_millis")]
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIpListResponseData {
    pub ips: Vec<DetailedIp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIpListResponse {
    pub response: UserIpListResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserIpListEntry {
    pub user_id: String,
    pub ips: Vec<DetailedIp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsersIpListResponseData {
    pub users: Vec<UserIpListEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsersIpListResponse {
    pub response: UsersIpListResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundStat {
    pub inbound: String,
    pub downlink: i64,
    pub uplink: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundStat {
    pub outbound: String,
    pub downlink: i64,
    pub uplink: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStat {
    pub username: String,
    pub downlink: i64,
    pub uplink: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundStatsResponseData {
    pub inbound: String,
    pub downlink: i64,
    pub uplink: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundStatsResponse {
    pub response: InboundStatsResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundStatsResponseData {
    pub outbound: String,
    pub downlink: i64,
    pub uplink: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundStatsResponse {
    pub response: OutboundStatsResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllInboundsStatsResponseData {
    pub inbounds: Vec<InboundStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllInboundsStatsResponse {
    pub response: AllInboundsStatsResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllOutboundsStatsResponseData {
    pub outbounds: Vec<OutboundStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllOutboundsStatsResponse {
    pub response: AllOutboundsStatsResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsersStatsResponseData {
    pub users: Vec<UserStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsersStatsResponse {
    pub response: UsersStatsResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedStatsResponseData {
    pub inbounds: Vec<InboundStat>,
    pub outbounds: Vec<OutboundStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedStatsResponse {
    pub response: CombinedStatsResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XraySysStats {
    #[serde(rename = "numGoroutine")]
    pub num_goroutine: u32,
    #[serde(rename = "numGC")]
    pub num_gc: u32,
    pub alloc: u64,
    #[serde(rename = "totalAlloc")]
    pub total_alloc: u64,
    pub sys: u64,
    pub mallocs: u64,
    pub frees: u64,
    #[serde(rename = "liveObjects")]
    pub live_objects: u64,
    #[serde(rename = "pauseTotalNs")]
    pub pause_total_ns: u64,
    pub uptime: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentBlockerPluginStats {
    #[serde(rename = "reportsCount")]
    pub reports_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStats {
    #[serde(rename = "torrentBlocker")]
    pub torrent_blocker: TorrentBlockerPluginStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatsInner {
    pub stats: NodeSystemStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatsResponseData {
    pub xray_info: Option<XraySysStats>,
    pub plugins: PluginStats,
    pub system: SystemStatsInner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatsResponse {
    pub response: SystemStatsResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeocheckRequest {
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub interface: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeocheckImage {
    pub format: String,
    #[serde(rename = "media_type")]
    pub media_type: String,
    pub encoding: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeocheckResponseData {
    pub image: GeocheckImage,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeocheckResponse {
    pub response: GeocheckResponseData,
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub config: Map<String, Value>,
    pub uuid: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub plugin: Option<PluginConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedResponseData {
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedResponse {
    pub response: AcceptedResponseData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockIp {
    pub ip: String,
    pub timeout: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockIpsRequest {
    pub ips: Vec<BlockIp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnblockIpsRequest {
    pub ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentBlockerReportsResponseData {
    pub reports: Vec<TorrentBlockerReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentBlockerReportsResponse {
    pub response: TorrentBlockerReportsResponseData,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TorrentBlockerActionReport, TorrentBlockerReport, XrayWebhook};

    #[test]
    fn add_user_item_tags() {
        let vless: AddUserItem = serde_json::from_value(serde_json::json!({
            "type": "vless", "tag": "in", "username": "u", "uuid": "x", "flow": ""
        }))
        .unwrap();
        assert!(matches!(vless, AddUserItem::Vless { .. }));

        let ss22: AddUserItem = serde_json::from_value(serde_json::json!({
            "type": "shadowsocks22", "tag": "in", "username": "u", "password": "p"
        }))
        .unwrap();
        assert!(matches!(ss22, AddUserItem::Shadowsocks22 { .. }));
    }

    #[test]
    fn start_request_camel_case() {
        let value = serde_json::json!({
            "internals": {
                "forceRestart": true,
                "hashes": {
                    "emptyConfig": "e",
                    "inbounds": [{ "usersCount": 3, "hash": "h", "tag": "t" }]
                }
            },
            "xrayConfig": { "inbounds": [] }
        });
        let req: StartXrayRequest = serde_json::from_value(value).unwrap();
        assert!(req.internals.force_restart);
        assert_eq!(req.internals.hashes.inbounds[0].users_count, 3);
        assert_eq!(req.internals.hashes.empty_config, "e");
    }

    #[test]
    fn report_dates_are_rfc3339() {
        use chrono::{TimeZone, Utc};
        let report = TorrentBlockerReport {
            action_report: TorrentBlockerActionReport {
                blocked: true,
                ip: "1.2.3.4".into(),
                block_duration: 60,
                will_unblock_at: Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap(),
                user_id: "u".into(),
                processed_at: Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap(),
            },
            xray_report: XrayWebhook {
                email: Some("u".into()),
                level: Some(0),
                protocol: Some("bittorrent".into()),
                network: "tcp".into(),
                source: Some("1.2.3.4".into()),
                destination: "5.6.7.8".into(),
                route_target: None,
                original_target: None,
                inbound_tag: None,
                inbound_name: None,
                inbound_local: None,
                outbound_tag: None,
                ts: 0.0,
            },
        };
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(
            json["actionReport"]["willUnblockAt"],
            "2026-01-02T03:04:05.000Z"
        );
        assert_eq!(
            json["actionReport"]["processedAt"],
            "2026-01-02T03:04:05.000Z"
        );
    }
}
