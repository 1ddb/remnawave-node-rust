pub const ROOT: &str = "/node";

pub mod controllers {
    pub const XRAY_CONTROLLER: &str = "xray";
    pub const STATS_CONTROLLER: &str = "stats";
    pub const HANDLER_CONTROLLER: &str = "handler";
    pub const PLUGIN_CONTROLLER: &str = "plugin";
    pub const INTERNAL_CONTROLLER: &str = "internal";
}

pub mod xray {
    pub const START: &str = "start";
    pub const STOP: &str = "stop";
    pub const NODE_HEALTH_CHECK: &str = "healthcheck";

    pub const START_URL: &str = "/node/xray/start";
    pub const STOP_URL: &str = "/node/xray/stop";
    pub const NODE_HEALTH_CHECK_URL: &str = "/node/xray/healthcheck";
}

pub mod stats {
    pub const GET_USER_ONLINE_STATUS: &str = "get-user-online-status";
    pub const GET_USERS_STATS: &str = "get-users-stats";
    pub const GET_SYSTEM_STATS: &str = "get-system-stats";
    pub const GET_INBOUND_STATS: &str = "get-inbound-stats";
    pub const GET_OUTBOUND_STATS: &str = "get-outbound-stats";
    pub const GET_ALL_OUTBOUNDS_STATS: &str = "get-all-outbounds-stats";
    pub const GET_ALL_INBOUNDS_STATS: &str = "get-all-inbounds-stats";
    pub const GET_COMBINED_STATS: &str = "get-combined-stats";
    pub const GET_USER_IP_LIST: &str = "get-user-ip-list";
    pub const GET_USERS_IP_LIST: &str = "get-users-ip-list";
    pub const GET_GEOCHECK: &str = "get-geocheck";

    pub const GET_USER_ONLINE_STATUS_URL: &str = "/node/stats/get-user-online-status";
    pub const GET_USERS_STATS_URL: &str = "/node/stats/get-users-stats";
    pub const GET_SYSTEM_STATS_URL: &str = "/node/stats/get-system-stats";
    pub const GET_INBOUND_STATS_URL: &str = "/node/stats/get-inbound-stats";
    pub const GET_OUTBOUND_STATS_URL: &str = "/node/stats/get-outbound-stats";
    pub const GET_ALL_OUTBOUNDS_STATS_URL: &str = "/node/stats/get-all-outbounds-stats";
    pub const GET_ALL_INBOUNDS_STATS_URL: &str = "/node/stats/get-all-inbounds-stats";
    pub const GET_COMBINED_STATS_URL: &str = "/node/stats/get-combined-stats";
    pub const GET_USER_IP_LIST_URL: &str = "/node/stats/get-user-ip-list";
    pub const GET_USERS_IP_LIST_URL: &str = "/node/stats/get-users-ip-list";
    pub const GET_GEOCHECK_URL: &str = "/node/stats/get-geocheck";
}

pub mod handler {
    pub const REMOVE_USER: &str = "remove-user";
    pub const ADD_USER: &str = "add-user";
    pub const ADD_USERS: &str = "add-users";
    pub const REMOVE_USERS: &str = "remove-users";
    pub const DROP_USERS_CONNECTIONS: &str = "drop-users-connections";
    pub const DROP_IPS: &str = "drop-ips";

    pub const ADD_USER_URL: &str = "/node/handler/add-user";
    pub const REMOVE_USER_URL: &str = "/node/handler/remove-user";
    pub const ADD_USERS_URL: &str = "/node/handler/add-users";
    pub const REMOVE_USERS_URL: &str = "/node/handler/remove-users";
    pub const DROP_USERS_CONNECTIONS_URL: &str = "/node/handler/drop-users-connections";
    pub const DROP_IPS_URL: &str = "/node/handler/drop-ips";
}

pub mod plugin {
    pub const SYNC: &str = "sync";
    pub const TORRENT_BLOCKER_COLLECT: &str = "torrent-blocker/collect";
    pub const NFTABLES_UNBLOCK_IPS: &str = "nftables/unblock-ips";
    pub const NFTABLES_BLOCK_IPS: &str = "nftables/block-ips";
    pub const NFTABLES_RECREATE_TABLES: &str = "nftables/recreate-tables";

    pub const SYNC_URL: &str = "/node/plugin/sync";
    pub const TORRENT_BLOCKER_COLLECT_URL: &str = "/node/plugin/torrent-blocker/collect";
    pub const NFTABLES_UNBLOCK_IPS_URL: &str = "/node/plugin/nftables/unblock-ips";
    pub const NFTABLES_BLOCK_IPS_URL: &str = "/node/plugin/nftables/block-ips";
    pub const NFTABLES_RECREATE_TABLES_URL: &str = "/node/plugin/nftables/recreate-tables";
}

pub mod internal {
    pub const XRAY_INTERNAL_API_PATH: &str = "/get-config";
    pub const XRAY_INTERNAL_WEBHOOK_PATH: &str = "/webhook";
    pub const XRAY_INTERNAL_FULL_PATH: &str = "/internal/get-config";
    pub const XRAY_INTERNAL_FULL_WEBHOOK_PATH: &str = "/internal/webhook";
}
