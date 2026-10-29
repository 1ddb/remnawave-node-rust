use std::collections::HashSet;

use rw_contract::models::TorrentBlockerReport;

pub const DEFAULT_IGNORED_IPS: &[&str] = &[
    "::",
    "::1",
    "0.0.0.0",
    "0.0.0.0/0",
    "127.0.0.0/8",
    "127.0.0.1",
    "255.255.255.255",
];

fn default_ignored(ip: &str) -> bool {
    DEFAULT_IGNORED_IPS.contains(&ip)
}

#[derive(Debug, Clone, Default)]
pub struct TorrentBlockerState {
    pub enabled: bool,
    pub block_duration: Option<i64>,
    pub ignored_ips: HashSet<String>,
    pub ignored_users: HashSet<String>,
    pub include_rule_tags: HashSet<String>,
    pub webhook_url: Option<String>,
    pub rule_position: u32,
    pub reports: Vec<TorrentBlockerReport>,
}

impl TorrentBlockerState {
    pub fn configure(&mut self, block_duration: i64, rule_position: Option<f64>) {
        self.enabled = true;
        self.block_duration = Some(block_duration);
        self.rule_position = rule_position.unwrap_or(0.0).max(0.0) as u32;
    }

    pub fn is_ip_ignored(&self, ip: &str) -> bool {
        self.ignored_ips.contains(ip) || default_ignored(ip)
    }

    pub fn is_user_ignored(&self, user: &str) -> bool {
        self.ignored_users.contains(user)
    }

    pub fn add_report(&mut self, report: TorrentBlockerReport) {
        self.reports.push(report);
    }

    pub fn flush_reports(&mut self) -> Vec<TorrentBlockerReport> {
        std::mem::take(&mut self.reports)
    }

    pub fn reports_count(&self) -> usize {
        self.reports.len()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionDropState {
    pub whitelist_ips: HashSet<String>,
}

impl ConnectionDropState {
    pub fn set_whitelist_ips(&mut self, ips: Vec<String>) {
        self.whitelist_ips = ips.into_iter().collect();
    }

    pub fn is_whitelisted(&self, ip: &str) -> bool {
        self.whitelist_ips.contains(ip) || default_ignored(ip)
    }

    pub fn reset(&mut self) {
        self.whitelist_ips.clear();
    }
}

#[derive(Debug, Clone, Default)]
pub struct PreStartState {
    pub enabled: bool,
    pub cleanup_sockets_enabled: bool,
    pub cleanup_socket_files: Vec<String>,
}

impl PreStartState {
    pub fn configure(&mut self, enabled: bool, cleanup: Option<crate::schema::CleanupSockets>) {
        self.enabled = enabled;
        match cleanup {
            Some(c) => {
                self.cleanup_sockets_enabled = c.enabled;
                self.cleanup_socket_files = c.files;
            }
            None => {
                self.cleanup_sockets_enabled = false;
                self.cleanup_socket_files.clear();
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PostStartState {
    pub enabled: bool,
    pub webhook_enabled: bool,
    pub webhook_url: String,
}

impl PostStartState {
    pub fn configure(&mut self, enabled: bool, webhook: Option<crate::schema::PostStartWebhook>) {
        self.enabled = enabled;
        match webhook {
            Some(w) => {
                self.webhook_enabled = w.enabled;
                self.webhook_url = w.url;
            }
            None => {
                self.webhook_enabled = false;
                self.webhook_url.clear();
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PluginAvailability {
    pub ingress_filter: bool,
    pub egress_filter: bool,
    pub torrent_blocker: bool,
    pub connection_drop: bool,
    pub pre_start: bool,
    pub post_start: bool,
}

#[derive(Debug, Clone)]
pub struct PluginConfigDetails {
    pub uuid: String,
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct PluginState {
    pub torrent_blocker: TorrentBlockerState,
    pub connection_drop: ConnectionDropState,
    pub pre_start: PreStartState,
    pub post_start: PostStartState,
    pub available: PluginAvailability,
    pub initialized: bool,
    pub last_config_hash: Option<String>,
    pub config_details: Option<PluginConfigDetails>,
}

impl PluginState {
    pub fn new() -> Self {
        Self {
            available: PluginAvailability {
                connection_drop: false,
                ingress_filter: false,
                torrent_blocker: false,
                egress_filter: false,
                pre_start: true,
                post_start: true,
            },
            ..Default::default()
        }
    }

    pub fn is_config_changed(&self, hash: &str) -> bool {
        self.last_config_hash.as_deref() != Some(hash)
    }

    pub fn update_config_hash(&mut self, hash: String) {
        self.last_config_hash = Some(hash);
        self.initialized = true;
    }

    pub fn reset_state(&mut self) {
        self.torrent_blocker.reset();
        self.connection_drop.reset();
        self.pre_start = PreStartState::default();
        self.post_start = PostStartState::default();
    }

    pub fn clean_up_active_plugin(&mut self) {
        self.config_details = None;
        self.last_config_hash = None;
        self.initialized = false;
    }

    pub fn set_plugin_config_details(&mut self, uuid: String, name: String) {
        self.config_details = Some(PluginConfigDetails { uuid, name });
    }

    pub fn has_active_plugin(&self) -> bool {
        self.config_details.is_some()
    }
}
