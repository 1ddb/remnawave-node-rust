use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

pub const INTERVAL_MS: u64 = 1_000;

#[derive(Debug, Clone)]
pub struct InterfaceRate {
    pub interface: String,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub rx_total: f64,
    pub tx_total: f64,
}

impl InterfaceRate {
    pub fn to_contract(&self) -> rw_contract::models::NetworkInterface {
        rw_contract::models::NetworkInterface {
            interface: self.interface.clone(),
            rx_bytes_per_sec: self.rx_bytes_per_sec,
            tx_bytes_per_sec: self.tx_bytes_per_sec,
            rx_total: self.rx_total,
            tx_total: self.tx_total,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NetworkSnapshot {
    pub is_available: bool,
    pub interfaces: Vec<InterfaceRate>,
    pub default_interface: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Default)]
struct Inner {
    available: bool,
    previous: HashMap<String, (u64, u64, i64)>,
    rates: HashMap<String, InterfaceRate>,
    default_iface: Option<String>,
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct NetworkStats {
    inner: Arc<RwLock<Inner>>,
}

impl NetworkStats {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::default())),
        }
    }

    pub async fn start(&self) {
        if !std::path::Path::new("/proc/net/dev").exists() {
            warn!("/proc/net/dev not found — network stats polling disabled (non-Linux OS?)");
            return;
        }
        let default_iface = resolve_default_interface();
        let previous = read_proc_net_dev();
        {
            let mut inner = self.inner.write().await;
            inner.available = true;
            inner.default_iface = default_iface.clone();
            inner.previous = previous;
            inner.updated_at = Some(Utc::now());
        }
        info!(
            "Network stats polling started (interval: {}ms, default: {})",
            INTERVAL_MS,
            default_iface.as_deref().unwrap_or("unknown")
        );

        let this = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(INTERVAL_MS));
            loop {
                interval.tick().await;
                this.tick().await;
            }
        });
    }

    async fn tick(&self) {
        let now = read_proc_net_dev();
        let mut inner = self.inner.write().await;

        for (iface, (rx, tx, ts)) in &now {
            let Some((prev_rx, prev_tx, prev_ts)) = inner.previous.get(iface).copied() else {
                continue;
            };
            let elapsed = (*ts - prev_ts) as f64 / 1000.0;
            if elapsed <= 0.0 {
                continue;
            }
            inner.rates.insert(
                iface.clone(),
                InterfaceRate {
                    interface: iface.clone(),
                    rx_bytes_per_sec: ((*rx).saturating_sub(prev_rx) as f64 / elapsed).max(0.0),
                    tx_bytes_per_sec: ((*tx).saturating_sub(prev_tx) as f64 / elapsed).max(0.0),
                    rx_total: *rx as f64,
                    tx_total: *tx as f64,
                },
            );
        }

        inner.previous = now;
        inner.updated_at = Some(Utc::now());
    }

    pub async fn snapshot(&self) -> NetworkSnapshot {
        let inner = self.inner.read().await;
        NetworkSnapshot {
            is_available: inner.available,
            interfaces: inner.rates.values().cloned().collect(),
            default_interface: inner.default_iface.clone(),
            updated_at: inner.updated_at.unwrap_or_else(Utc::now),
        }
    }

    pub async fn get_default(&self) -> Option<InterfaceRate> {
        let inner = self.inner.read().await;
        inner
            .default_iface
            .as_ref()
            .and_then(|name| inner.rates.get(name).cloned())
    }

    pub async fn get_by_interface(&self, name: &str) -> Option<InterfaceRate> {
        self.inner.read().await.rates.get(name).cloned()
    }

    pub async fn get_default_interface_name(&self) -> Option<String> {
        self.inner.read().await.default_iface.clone()
    }
}

impl Default for NetworkStats {
    fn default() -> Self {
        Self::new()
    }
}

fn read_proc_net_dev() -> HashMap<String, (u64, u64, i64)> {
    let mut result = HashMap::new();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let Ok(content) = std::fs::read_to_string("/proc/net/dev") else {
        error!("Failed to read /proc/net/dev");
        return result;
    };

    for line in content.lines().skip(2) {
        let Some((iface, rest)) = line.split_once(':') else {
            continue;
        };
        let iface = iface.trim();
        if iface.is_empty() {
            continue;
        }
        let fields: Vec<&str> = rest.split_whitespace().collect();
        if fields.len() < 16 {
            continue;
        }
        let Ok(rx) = fields[0].parse::<u64>() else {
            continue;
        };
        let Ok(tx) = fields[8].parse::<u64>() else {
            continue;
        };
        result.insert(iface.to_string(), (rx, tx, timestamp));
    }
    result
}

fn resolve_default_interface() -> Option<String> {
    let content = std::fs::read_to_string("/proc/net/route").ok()?;
    let mut best: Option<(String, u64)> = None;

    for line in content.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 11 {
            continue;
        }
        let iface = fields[0];
        let destination = fields[1];
        if destination != "00000000" {
            continue;
        }
        let metric = fields[6].parse::<u64>().unwrap_or(0);
        if best.as_ref().map_or(true, |(_, m)| metric < *m) {
            best = Some((iface.to_string(), metric));
        }
    }

    best.map(|(iface, _)| iface)
}
