use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rw_config::AppConfig;
use rw_core::network_stats::NetworkStats;
use rw_xray::XrayService;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub xray: Arc<XrayService>,
    pub network: NetworkStats,
    pub geocheck_running: Arc<AtomicBool>,
}
