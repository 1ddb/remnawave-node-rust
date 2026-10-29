use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use jsonwebtoken::{DecodingKey, Validation};
use rw_config::AppConfig;
use rw_core::network_stats::NetworkStats;
use rw_xray::XrayService;

use crate::middleware::jwt_validation;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub xray: Arc<XrayService>,
    pub network: NetworkStats,
    pub geocheck_running: Arc<AtomicBool>,
    /// Public JWT key parsed once at startup instead of on every request.
    pub jwt_key: Arc<DecodingKey>,
    pub jwt_validation: Validation,
}

impl AppState {
    pub fn new(
        config: Arc<AppConfig>,
        xray: Arc<XrayService>,
        network: NetworkStats,
    ) -> Result<Self, jsonwebtoken::errors::Error> {
        let jwt_key = Arc::new(DecodingKey::from_rsa_pem(config.jwt_public_key.as_bytes())?);
        Ok(Self {
            config,
            xray,
            network,
            geocheck_running: Arc::new(AtomicBool::new(false)),
            jwt_key,
            jwt_validation: jwt_validation(),
        })
    }
}
