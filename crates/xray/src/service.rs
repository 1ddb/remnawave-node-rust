use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rw_asn_lmdb::AsnLmdb;
use rw_contract::commands::{
    NodeHealthCheckResponseData, PluginConfig, StartXrayRequest, StartXrayResponseData,
    StartXraySystem,
};
use rw_contract::models::XrayWebhook;
use rw_core::capabilities::has_cap_net_admin;
use rw_core::network_stats::NetworkStats;
use rw_core::system_stats;
use rw_node_plugins::connection_drop;
use rw_node_plugins::nft::{
    NftManager, EGRESS_FILTER_IP_SET_NAME, EGRESS_FILTER_PORT_SET_NAME, INGRESS_FILTER_IP_SET_NAME,
    TORRENT_BLOCKER_SET_NAME,
};
use rw_node_plugins::pre_start;
use rw_node_plugins::schema::NodePlugin;
use rw_node_plugins::state::PluginState;
use rw_node_plugins::torrent;
use rw_xray_rpc::{RpcError, XrayRpc};
use serde_json::{Map, Value};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::api_config::{generate_api_config, InternalView, TorrentBlockerStateView};
use crate::core_loader::CoreLoader;
use crate::geodata::Geodata;
use crate::internal_service::InternalService;
use crate::process::{ProcessEvent, ProcessStatus, XrayProcess, CORE_LINK, XRAY_LOG_FILE};

pub struct XrayServiceConfig {
    pub internal_socket_path: String,
    pub internal_rest_token: String,
    pub xtls_api_socket_path: String,
    pub disable_hashed_set_check: bool,
    pub nftables_logging: bool,
    pub nftables_accept_reply_traffic: bool,
    pub nftables_table_name: String,
    pub core_link: String,
}

pub struct XrayService {
    process: XrayProcess,
    internal: InternalService,
    rpc: RwLock<Option<Arc<XrayRpc>>>,
    xray_version: RwLock<Option<String>>,
    is_xray_online: AtomicBool,
    is_started_processing: AtomicBool,
    handled_generation: AtomicU64,
    last_metadata: RwLock<Option<rw_contract::models::NodeMetadata>>,
    node_version: String,
    disable_hashed_set_check: bool,
    internal_view: InternalView,
    plugin_state: Arc<Mutex<PluginState>>,
    nft: Arc<Mutex<NftManager>>,
    network_stats: NetworkStats,
    asn: Option<Arc<AsnLmdb>>,
}

impl XrayService {
    pub async fn new(
        config: XrayServiceConfig,
        network_stats: NetworkStats,
        asn: Option<Arc<AsnLmdb>>,
    ) -> Self {
        let process = XrayProcess::new(
            config.core_link.clone(),
            XRAY_LOG_FILE,
            &config.xtls_api_socket_path,
            &config.internal_socket_path,
            &config.internal_rest_token,
        );

        let mut nft = NftManager::new(
            config.nftables_logging,
            config.nftables_accept_reply_traffic,
            config.nftables_table_name,
        );
        let mut state = PluginState::new();

        if has_cap_net_admin() {
            state.available.connection_drop = true;
            if nft.init().await {
                state.available.ingress_filter = true;
                state.available.torrent_blocker = true;
                state.available.egress_filter = true;
            }
        } else {
            warn!("CAP_NET_ADMIN is not available.");
        }
        log_available_plugins(&state);

        let service = Self {
            process,
            internal: InternalService::new(),
            rpc: RwLock::new(None),
            xray_version: RwLock::new(None),
            is_xray_online: AtomicBool::new(false),
            is_started_processing: AtomicBool::new(false),
            handled_generation: AtomicU64::new(0),
            last_metadata: RwLock::new(None),
            node_version: option_env!("RWNODE_VERSION")
                .unwrap_or(env!("CARGO_PKG_VERSION"))
                .to_string(),
            disable_hashed_set_check: config.disable_hashed_set_check,
            internal_view: InternalView {
                socket_path: config.internal_socket_path,
                token: config.internal_rest_token,
                xtls_api_socket_path: config.xtls_api_socket_path,
            },
            plugin_state: Arc::new(Mutex::new(state)),
            nft: Arc::new(Mutex::new(nft)),
            network_stats,
            asn,
        };

        service.refresh_xray_version().await;
        service
    }

    pub fn internal(&self) -> &InternalService {
        &self.internal
    }

    pub fn plugin_state(&self) -> Arc<Mutex<PluginState>> {
        self.plugin_state.clone()
    }

    pub fn is_online(&self) -> bool {
        self.is_xray_online.load(Ordering::SeqCst)
    }

    /// Watch process lifecycle events and refresh the cached RPC/online state
    /// when the supervisor restarts Xray on its own.
    pub fn start_watchdog(self: &Arc<Self>) {
        let this = self.clone();
        let mut events = self.process.subscribe();

        tokio::spawn(async move {
            loop {
                if events.changed().await.is_err() {
                    return;
                }
                let event = *events.borrow_and_update();
                match event {
                    ProcessEvent::Down => {
                        *this.rpc.write().await = None;
                        this.is_xray_online.store(false, Ordering::SeqCst);
                    }
                    ProcessEvent::Up { pid, generation } => {
                        if this.is_started_processing.load(Ordering::SeqCst)
                            || this.handled_generation.load(Ordering::SeqCst) == generation
                        {
                            continue;
                        }
                        this.recover_after_supervisor_restart(pid, generation).await;
                    }
                }
            }
        });
    }

    async fn recover_after_supervisor_restart(&self, pid: u32, generation: u64) {
        warn!("Xray supervisor restarted the core (pid {pid}); reconnecting RPC");
        *self.rpc.write().await = None;
        self.is_xray_online.store(false, Ordering::SeqCst);
        self.refresh_xray_version().await;

        let (ready, error) = self.wait_for_rpc(Some(pid)).await;
        if !ready {
            error!(
                "Xray Core restarted by supervisor but did not become ready: {}",
                error.unwrap_or_else(|| "unknown error".into())
            );
            return;
        }

        self.handled_generation.store(generation, Ordering::SeqCst);
        self.is_xray_online.store(true, Ordering::SeqCst);
        self.refresh_xray_version().await;
        info!("✔ XRay Core v{:?} is up after supervisor restart.", *self.xray_version.read().await);

        let metadata = self.last_metadata.read().await.clone();
        self.run_post_start(&metadata).await;
    }

    pub async fn get_xray_info(&self) -> (Option<String>, String) {
        (self.xray_version.read().await.clone(), CORE_LINK.to_string())
    }

    async fn refresh_xray_version(&self) {
        if let Some(version) = self.process.get_core_version().await {
            if let Some(semver) = version.semver {
                *self.xray_version.write().await = Some(semver);
            }
        }
    }

    async fn rpc(&self) -> Option<Arc<XrayRpc>> {
        self.rpc.read().await.clone()
    }

    pub async fn status(&self) -> ProcessStatus {
        self.process.status().await
    }

    pub async fn start_xray(&self, body: StartXrayRequest) -> StartXrayResponseData {
        let system = self.build_start_system().await;

        if self.is_started_processing.swap(true, Ordering::SeqCst) {
            warn!("Request already in progress");
            return StartXrayResponseData {
                is_started: false,
                version: self.xray_version.read().await.clone(),
                error: Some("Request already in progress".into()),
                node_information: rw_contract::commands::NodeInformation {
                    version: Some(self.node_version.clone()),
                },
                system,
            };
        }

        let result = self.start_xray_inner(&body, system).await;
        self.is_started_processing.store(false, Ordering::SeqCst);
        result
    }

    async fn start_xray_inner(
        &self,
        body: &StartXrayRequest,
        system: StartXraySystem,
    ) -> StartXrayResponseData {
        let node_information =
            rw_contract::commands::NodeInformation { version: Some(self.node_version.clone()) };

        *self.last_metadata.write().await = body.internals.metadata.clone();

        if body.internals.force_restart {
            warn!("Force restart requested");
        }

        if self.is_xray_online.load(Ordering::SeqCst)
            && !self.disable_hashed_set_check
            && !body.internals.force_restart
        {
            let should_restart = match self.rpc().await {
                Some(rpc) => match rpc.get_sys_stats().await {
                    Ok(_) => self.internal.needs_restart(&body.internals.hashes).await,
                    Err(_) => {
                        self.is_xray_online.store(false, Ordering::SeqCst);
                        warn!("Xray Core health check failed, restarting...");
                        true
                    }
                },
                None => true,
            };

            if !should_restart {
                return StartXrayResponseData {
                    is_started: true,
                    version: self.xray_version.read().await.clone(),
                    error: None,
                    node_information,
                    system,
                };
            }
        }

        let torrent_state = {
            let state = self.plugin_state.lock().await;
            TorrentBlockerStateView {
                enabled: state.torrent_blocker.enabled,
                include_rule_tags: state.torrent_blocker.include_rule_tags.clone(),
                rule_position: state.torrent_blocker.rule_position,
            }
        };

        let full_config = generate_api_config(
            body.xray_config.clone(),
            &torrent_state,
            &self.internal_view,
            has_cap_net_admin(),
        );

        self.internal.debug_hashes(&body.internals.hashes).await;
        self.internal.extract_users_from_config(&body.internals.hashes, &full_config).await;

        let geodata = full_config.get("geodata").cloned().unwrap_or(Value::Null);
        if let Err(e) = CoreLoader::prepare(&geodata).await {
            warn!("Core loader: {e}");
        }
        let _ = Geodata::prepare(&geodata).await;

        self.process.stop().await;
        pre_start::run(&self.plugin_state.lock().await.pre_start).await;

        if let Err(e) = self.process.start().await {
            error!("Failed to start Xray process: {e}");
            *self.rpc.write().await = None;
            self.is_xray_online.store(false, Ordering::SeqCst);
            self.process.stop().await;
            return StartXrayResponseData {
                is_started: false,
                version: None,
                error: Some(e.to_string()),
                node_information,
                system,
            };
        }
        self.handled_generation.store(self.process.current_generation().await, Ordering::SeqCst);

        let status = self.wait_for_internal_status().await;
        if !status.0 {
            self.is_xray_online.store(false, Ordering::SeqCst);
            *self.rpc.write().await = None;
            error!("Xray Core v{:?} failed to start.", *self.xray_version.read().await);
            // Do not leave a process that never passed the health check running.
            self.process.stop().await;
            let tail = self.process.tail_log_lines(5).await;
            let log_reason = tail.last().map(|l| l.trim().chars().take(500).collect::<String>());
            let error_message = match log_reason {
                Some(reason) if !reason.is_empty() => {
                    format!("{} · {}", status.1.unwrap_or_default(), reason)
                }
                _ => status.1.unwrap_or_default(),
            };
            return StartXrayResponseData {
                is_started: false,
                version: self.xray_version.read().await.clone(),
                error: Some(error_message),
                node_information,
                system,
            };
        }

        self.is_xray_online.store(true, Ordering::SeqCst);
        self.refresh_xray_version().await;
        info!("✔ XRay Core v{:?} is up and running.", *self.xray_version.read().await);

        self.run_post_start(&body.internals.metadata).await;

        StartXrayResponseData {
            is_started: true,
            version: self.xray_version.read().await.clone(),
            error: None,
            node_information,
            system,
        }
    }

    async fn build_start_system(&self) -> StartXraySystem {
        let interface = self.network_stats.get_default().await.map(|r| r.to_contract());
        StartXraySystem {
            info: system_stats::get_system_info(),
            stats: system_stats::get_system_stats(),
            interface,
        }
    }

    async fn run_post_start(&self, metadata: &Option<rw_contract::models::NodeMetadata>) {
        let state = self.plugin_state.lock().await;
        if !state.available.post_start
            || !state.post_start.enabled
            || !state.post_start.webhook_enabled
        {
            return;
        }
        let metadata = match metadata {
            Some(metadata) => {
                serde_json::to_value(metadata).unwrap_or_else(|_| serde_json::json!({}))
            }
            None => serde_json::json!({}),
        };
        let body = serde_json::json!({
            "scope": "service",
            "event": "service.core_started",
            "timestamp": chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "metadata": metadata,
        });
        rw_core::webhook::send_webhook(&state.post_start.webhook_url, body);
    }

    async fn wait_for_internal_status(&self) -> (bool, Option<String>) {
        let started_pid = self.process.status().await.pid;
        self.wait_for_rpc(started_pid).await
    }

    async fn wait_for_rpc(&self, expected_pid: Option<u32>) -> (bool, Option<String>) {
        let mut delay = Duration::from_millis(100);

        for attempt in 1..=30 {
            if let Ok(rpc) =
                XrayRpc::connect_abstract(&self.internal_view.xtls_api_socket_path).await
            {
                let result =
                    tokio::time::timeout(Duration::from_secs(5), rpc.get_sys_stats()).await;
                let error = match result {
                    Ok(Ok(_)) => {
                        *self.rpc.write().await = Some(Arc::new(rpc));
                        return (true, None);
                    }
                    Ok(Err(e)) => e.to_string(),
                    Err(_) => "request timed out".to_string(),
                };
                if !self.alive_with_pid(expected_pid).await {
                    return (
                        false,
                        Some(format!(
                            "Xray Core process is not running anymore ({})",
                            self.process.tail_status_line().await
                        )),
                    );
                }
                debug!("Xray status check {attempt}/30 failed: {error}");
            } else if !self.alive_with_pid(expected_pid).await {
                return (false, Some("Xray Core process is not running anymore".into()));
            }

            warn!("▸ XRay Core status check, {attempt}/30 · retrying in {}ms", delay.as_millis());
            tokio::time::sleep(delay).await;
            delay = Duration::from_millis((delay.as_millis() as f64 * 1.5) as u64)
                .min(Duration::from_secs(2));
        }

        (false, Some("Xray Core did not become ready in time".into()))
    }

    async fn alive_with_pid(&self, started_pid: Option<u32>) -> bool {
        let status = self.process.status().await;
        status.up && (started_pid.is_none() || status.pid == started_pid)
    }

    pub async fn stop_xray(&self, with_plugin_cleanup: bool, with_online_check: bool) -> bool {
        if with_plugin_cleanup {
            self.reset_plugins().await;
        }
        if with_online_check && !self.is_xray_online.load(Ordering::SeqCst) {
            return true;
        }

        self.process.stop().await;
        *self.rpc.write().await = None;
        self.is_xray_online.store(false, Ordering::SeqCst);
        self.internal.cleanup().await;
        true
    }

    pub async fn kill_all(&self) {
        self.process.stop().await;
        let nft = self.nft.lock().await;
        if let Err(e) = nft.delete_table().await {
            warn!("Failed to delete nftables table: {e}");
        }
    }

    pub async fn get_node_health_check(&self) -> NodeHealthCheckResponseData {
        NodeHealthCheckResponseData {
            is_alive: true,
            xray_internal_status_cached: self.is_xray_online.load(Ordering::SeqCst),
            xray_version: self.xray_version.read().await.clone(),
            node_version: self.node_version.clone(),
        }
    }

    pub async fn reset_plugins(&self) {
        {
            let mut state = self.plugin_state.lock().await;
            state.reset_state();
            state.clean_up_active_plugin();
        }
        let _ = self.nft.lock().await.create_table().await;
    }

    pub async fn collect_reports(&self) -> Vec<rw_contract::models::TorrentBlockerReport> {
        let mut state = self.plugin_state.lock().await;
        if state.torrent_blocker.reports_count() == 0 {
            return Vec::new();
        }
        state.torrent_blocker.flush_reports()
    }

    pub async fn handle_xray_webhook(&self, webhook: XrayWebhook) {
        // Keep a single lock order everywhere: nft -> plugin state.
        let report = {
            let nft = self.nft.lock().await;
            let mut state = self.plugin_state.lock().await;
            torrent::handle_webhook(&mut state.torrent_blocker, &nft, webhook).await
        };

        if let Some(report) = report {
            if report.action_report.blocked {
                let state = self.plugin_state.lock().await;
                let available = state.available.connection_drop;
                let drop_state = state.connection_drop.clone();
                drop(state);
                connection_drop::drop_connections(
                    &drop_state,
                    &[report.action_report.ip],
                    available,
                )
                .await;
            }
        }
    }

    pub async fn drop_ips(&self, ips: Vec<String>) {
        let state = self.plugin_state.lock().await;
        let available = state.available.connection_drop;
        let drop_state = state.connection_drop.clone();
        drop(state);
        connection_drop::drop_connections(&drop_state, &ips, available).await;
    }

    pub async fn block_ips(&self, ips: Vec<(String, i64)>) -> bool {
        let nft = self.nft.lock().await;
        if !nft.is_available() {
            return false;
        }
        for (ip, timeout) in ips {
            if nft.add_address(&ip, TORRENT_BLOCKER_SET_NAME, Some(timeout)).await.is_err() {
                return false;
            }
            let state = self.plugin_state.lock().await;
            let drop_state = state.connection_drop.clone();
            drop(state);
            connection_drop::drop_connections(&drop_state, &[ip], true).await;
        }
        true
    }

    pub async fn unblock_ips(&self, ips: Vec<String>) -> bool {
        let nft = self.nft.lock().await;
        if !nft.is_available() {
            return false;
        }
        let _ = nft.remove_addresses(&ips, TORRENT_BLOCKER_SET_NAME).await;
        let _ = nft.remove_addresses(&ips, INGRESS_FILTER_IP_SET_NAME).await;
        true
    }

    pub async fn recreate_tables(&self) -> bool {
        let nft = self.nft.lock().await;
        if !nft.is_available() {
            return false;
        }
        nft.create_table().await.is_ok()
    }

    pub async fn sync_plugins(&self, plugin: Option<PluginConfig>) -> bool {
        let Some(plugin) = plugin else {
            let has_active = self.plugin_state.lock().await.has_active_plugin();
            if !has_active {
                return false;
            }
            info!("[PLUGIN] Received empty plugins, but there is an active plugin. Cleaning up...");
            self.reset_plugins().await;
            self.stop_xray(false, true).await;
            return true;
        };

        let config_hash = hash_config(&plugin.config);
        {
            let state = self.plugin_state.lock().await;
            if !state.is_config_changed(&config_hash) {
                debug!("[PLUGIN] Config unchanged. Skipping sync.");
                return true;
            }
        }

        let parsed = match NodePlugin::parse(&plugin.config) {
            Ok(p) => p,
            Err(e) => {
                error!("[PLUGIN] Invalid config: {e}");
                self.reset_plugins().await;
                self.stop_xray(false, true).await;
                return false;
            }
        };

        let previous = {
            let state = self.plugin_state.lock().await;
            (
                state.torrent_blocker.enabled,
                state.torrent_blocker.include_rule_tags.clone(),
                state.torrent_blocker.rule_position,
            )
        };

        let shared_map = self.resolve_shared_lists(&parsed).await;

        {
            let mut state = self.plugin_state.lock().await;
            state.reset_state();
            state.clean_up_active_plugin();
        }
        let _ = self.nft.lock().await.create_table().await;

        self.sync_connection_drop(&parsed, &shared_map).await;
        self.sync_torrent_blocker(&parsed, &shared_map).await;
        self.sync_pre_start(&parsed).await;
        self.sync_post_start(&parsed).await;
        self.sync_ingress_filter(&parsed, &shared_map).await;
        self.sync_egress_filter(&parsed, &shared_map).await;

        {
            let mut state = self.plugin_state.lock().await;
            state.update_config_hash(config_hash);
            state.set_plugin_config_details(plugin.uuid, plugin.name);
        }

        info!("[PLUGIN] Plugins changed...");

        let now_enabled = parsed.torrent_blocker.as_ref().is_some_and(|t| t.enabled);
        let was_enabled = previous.0;

        let needs_restart = if was_enabled && !now_enabled {
            if parsed.torrent_blocker.as_ref().and_then(|t| t.include_rule_tags.as_ref()).is_none()
            {
                if let Some(rpc) = self.rpc().await {
                    let _ = rpc
                        .remove_outbound(
                            rw_contract::xray_constants::XRAY_TORRENT_BLOCKER_OUTBOUND_TAG,
                        )
                        .await;
                }
                false
            } else {
                true
            }
        } else {
            let still_enabled = was_enabled && now_enabled;
            let new_tags: HashSet<String> = parsed
                .torrent_blocker
                .as_ref()
                .and_then(|t| t.include_rule_tags.clone())
                .unwrap_or_default()
                .into_iter()
                .collect();
            let new_position =
                parsed.torrent_blocker.as_ref().and_then(|t| t.rule_placement).unwrap_or(0.0);
            (was_enabled != now_enabled)
                || (still_enabled && previous.1 != new_tags)
                || (still_enabled && previous.2 != new_position)
        };

        if needs_restart {
            self.stop_xray(false, true).await;
        }

        true
    }

    async fn sync_connection_drop(
        &self,
        plugin: &NodePlugin,
        shared: &HashMap<String, Vec<String>>,
    ) {
        let Some(cd) = plugin.connection_drop.as_ref() else {
            return;
        };
        let state = self.plugin_state.lock().await;
        if !cd.enabled || !state.available.connection_drop {
            return;
        }
        drop(state);
        let ips = resolve_ip_list(&cd.whitelist_ips, shared);
        let mut state = self.plugin_state.lock().await;
        state.connection_drop.set_whitelist_ips(ips.clone());
        info!("[PLUGIN] Connection-Drop: {} whitelisted IPs synced.", ips.len());
    }

    async fn sync_torrent_blocker(
        &self,
        plugin: &NodePlugin,
        shared: &HashMap<String, Vec<String>>,
    ) {
        let Some(tb) = plugin.torrent_blocker.as_ref() else {
            return;
        };
        let available = self.plugin_state.lock().await.available.torrent_blocker;
        if !tb.enabled || !available {
            return;
        }
        let ips = resolve_ip_list(tb.ignore_lists.ip.as_deref().unwrap_or(&[]), shared);
        let users: Vec<String> = tb
            .ignore_lists
            .user_id
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|v| v.to_string())
            .collect();

        let mut state = self.plugin_state.lock().await;
        state.torrent_blocker.ignored_ips = ips.iter().cloned().collect();
        state.torrent_blocker.ignored_users = users.iter().cloned().collect();
        state.torrent_blocker.configure(tb.block_duration, tb.rule_placement);
        state.torrent_blocker.webhook_url = tb.webhook_url.clone();
        state.torrent_blocker.include_rule_tags =
            tb.include_rule_tags.clone().unwrap_or_default().into_iter().collect();
        info!(
            "[PLUGIN] Torrent-Blocker: blockDuration={}s, {} ignored IPs, {} ignored users",
            tb.block_duration,
            ips.len(),
            users.len()
        );
    }

    async fn sync_pre_start(&self, plugin: &NodePlugin) {
        let Some(ps) = plugin.pre_start.as_ref() else {
            return;
        };
        let available = self.plugin_state.lock().await.available.pre_start;
        if !ps.enabled || !available {
            return;
        }
        let mut state = self.plugin_state.lock().await;
        state.pre_start.configure(ps.enabled, ps.cleanup_sockets.clone());
        info!(
            "[PLUGIN] Pre-Start: socket cleanup {}.",
            if state.pre_start.cleanup_sockets_enabled { "enabled" } else { "disabled" }
        );
    }

    async fn sync_post_start(&self, plugin: &NodePlugin) {
        let Some(ps) = plugin.post_start.as_ref() else {
            return;
        };
        let available = self.plugin_state.lock().await.available.post_start;
        if !ps.enabled || !available {
            return;
        }
        let mut state = self.plugin_state.lock().await;
        state.post_start.configure(ps.enabled, ps.webhook.clone());
        info!(
            "[PLUGIN] Post-Start: webhook {}.",
            if state.post_start.webhook_enabled { "enabled" } else { "disabled" }
        );
    }

    async fn sync_ingress_filter(
        &self,
        plugin: &NodePlugin,
        shared: &HashMap<String, Vec<String>>,
    ) {
        let Some(f) = plugin.ingress_filter.as_ref() else {
            return;
        };
        let available = self.plugin_state.lock().await.available.ingress_filter;
        if !f.enabled || !available {
            return;
        }
        let ips = resolve_ip_list(&f.blocked_ips, shared);
        let _ = self.nft.lock().await.add_addresses(&ips, INGRESS_FILTER_IP_SET_NAME).await;
        info!("[PLUGIN] Ingress Filter: {} IPs synced.", ips.len());
    }

    async fn sync_egress_filter(&self, plugin: &NodePlugin, shared: &HashMap<String, Vec<String>>) {
        let Some(f) = plugin.egress_filter.as_ref() else {
            return;
        };
        let available = self.plugin_state.lock().await.available.egress_filter;
        if !f.enabled || !available {
            return;
        }
        let ips = resolve_ip_list(f.blocked_ips.as_deref().unwrap_or(&[]), shared);
        let ports = f.blocked_ports.clone().unwrap_or_default();

        let nft = self.nft.lock().await;
        let _ = nft.add_addresses(&ips, EGRESS_FILTER_IP_SET_NAME).await;
        let _ = nft.add_ports(&ports, EGRESS_FILTER_PORT_SET_NAME).await;
        info!("[PLUGIN] Egress Filter: {} IPs, {} ports synced.", ips.len(), ports.len());
    }

    async fn resolve_shared_lists(&self, plugin: &NodePlugin) -> HashMap<String, Vec<String>> {
        let mut map = HashMap::new();
        for list in &plugin.shared_lists {
            match list {
                rw_node_plugins::schema::SharedList::IpList { name, items } => {
                    map.insert(name.clone(), items.clone());
                }
                rw_node_plugins::schema::SharedList::AsList { name, items } => {
                    let mut prefixes = Vec::new();
                    for asn in items {
                        if let Some(db) = &self.asn {
                            if let Ok(Some(resolved)) = db.get_by_asn(*asn) {
                                info!(
                                    "[PLUGIN] ASN {asn} resolved: {} IPv4, {} IPv6",
                                    resolved.ipv4.len(),
                                    resolved.ipv6.len()
                                );
                                prefixes.extend(resolved.ipv4);
                                prefixes.extend(resolved.ipv6);
                            } else {
                                warn!("[PLUGIN] ASN {asn} not found");
                            }
                        } else {
                            warn!("[PLUGIN] ASN {asn} not found (LMDB unavailable)");
                        }
                    }
                    map.insert(name.clone(), prefixes);
                }
            }
        }
        map
    }
}

fn resolve_ip_list(ips: &[String], shared: &HashMap<String, Vec<String>>) -> Vec<String> {
    ips.iter()
        .flat_map(|ip| {
            if ip.starts_with("ext:") {
                match shared.get(ip.as_str()) {
                    Some(items) => items.clone(),
                    None => {
                        warn!("[PLUGIN] Shared IP list \"{ip}\" not found");
                        Vec::new()
                    }
                }
            } else {
                vec![ip.clone()]
            }
        })
        .collect()
}

fn hash_config(config: &Map<String, Value>) -> String {
    use sha2::{Digest, Sha256};
    let serialized = serde_json::to_string(config).unwrap_or_default();
    let digest = Sha256::digest(serialized.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn log_available_plugins(state: &PluginState) {
    for (name, enabled) in [
        ("Ingress Filter", state.available.ingress_filter),
        ("Egress Filter", state.available.egress_filter),
        ("Torrent Blocker", state.available.torrent_blocker),
        ("Connection Drop", state.available.connection_drop),
    ] {
        if enabled {
            info!("[PLUGIN] {name}: available");
        } else {
            warn!("[PLUGIN] {name}: not available");
        }
    }
}

impl XrayService {
    pub async fn rpc_for_handler(&self) -> Option<Arc<XrayRpc>> {
        self.rpc().await
    }

    pub async fn require_rpc(&self) -> Result<Arc<XrayRpc>, RpcError> {
        self.rpc()
            .await
            .ok_or_else(|| RpcError { code: 14, message: "Xray RPC unavailable".into() })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::resolve_ip_list;

    #[test]
    fn resolves_ext_shared_lists_and_keeps_plain_ips() {
        let mut shared = HashMap::new();
        shared.insert("ext:list".to_string(), vec!["1.1.1.1".to_string()]);

        let ips = vec!["ext:list".to_string(), "8.8.8.8".to_string(), "ext:missing".to_string()];

        assert_eq!(
            resolve_ip_list(&ips, &shared),
            vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()]
        );
    }
}
