use std::net::IpAddr;
use std::process::Stdio;

use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{error, warn};

pub const TABLE_NAME: &str = "remnanode";
pub const TORRENT_BLOCKER_SET_NAME: &str = "torrent-blocker";
pub const INGRESS_FILTER_IP_SET_NAME: &str = "ingress-filter-ip";
pub const EGRESS_FILTER_IP_SET_NAME: &str = "egress-filter-ip";
pub const EGRESS_FILTER_PORT_SET_NAME: &str = "egress-filter-port";

#[derive(Debug, Error)]
pub enum NftError {
    #[error("nft command failed: {0}")]
    Command(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone)]
pub struct NftManager {
    logging: bool,
    accept_reply_traffic: bool,
    available: bool,
}

impl NftManager {
    pub fn new(logging: bool, accept_reply_traffic: bool) -> Self {
        Self {
            logging,
            accept_reply_traffic,
            available: false,
        }
    }

    pub fn is_available(&self) -> bool {
        self.available
    }

    pub async fn init(&mut self) -> bool {
        match self.create_table().await {
            Ok(()) => {
                self.available = true;
                true
            }
            Err(e) => {
                error!("NftManager initialization failed: {e}");
                false
            }
        }
    }

    pub async fn create_table(&self) -> Result<(), NftError> {
        let mut script = String::new();
        script.push_str(&format!("add table inet {TABLE_NAME}\n"));
        script.push_str(&format!("delete table inet {TABLE_NAME}\n"));
        script.push_str(&format!("table inet {TABLE_NAME} {{\n"));

        script.push_str(&format!(
            "  set {TORRENT_BLOCKER_SET_NAME} {{ type ipv4_addr; flags interval,timeout; }}\n"
        ));
        script.push_str(&format!(
            "  set {TORRENT_BLOCKER_SET_NAME}6 {{ type ipv6_addr; flags interval,timeout; }}\n"
        ));
        for name in [
            INGRESS_FILTER_IP_SET_NAME,
            EGRESS_FILTER_IP_SET_NAME,
        ] {
            script.push_str(&format!("  set {name} {{ type ipv4_addr; flags interval; }}\n"));
            script.push_str(&format!("  set {name}6 {{ type ipv6_addr; flags interval; }}\n"));
        }
        script.push_str(&format!(
            "  set {EGRESS_FILTER_PORT_SET_NAME} {{ type inet_service; flags interval; }}\n"
        ));

        script.push_str("  chain ingress {\n");
        script.push_str("    type filter hook input priority filter; policy accept;\n");
        if self.accept_reply_traffic {
            script.push_str("    ct state established,related accept\n");
        }
        script.push_str(&format!("    ip saddr @{INGRESS_FILTER_IP_SET_NAME} drop\n"));
        script.push_str(&format!("    ip6 saddr @{INGRESS_FILTER_IP_SET_NAME}6 drop\n"));
        script.push_str(&format!("    ip saddr @{TORRENT_BLOCKER_SET_NAME} drop\n"));
        script.push_str(&format!("    ip6 saddr @{TORRENT_BLOCKER_SET_NAME}6 drop\n"));
        script.push_str("  }\n");

        script.push_str("  chain egress {\n");
        script.push_str("    type filter hook output priority filter; policy accept;\n");
        script.push_str(&format!("    ip daddr @{EGRESS_FILTER_IP_SET_NAME} drop\n"));
        script.push_str(&format!("    ip6 daddr @{EGRESS_FILTER_IP_SET_NAME}6 drop\n"));
        script.push_str(&format!(
            "    tcp dport @{EGRESS_FILTER_PORT_SET_NAME} drop\n"
        ));
        script.push_str(&format!(
            "    udp dport @{EGRESS_FILTER_PORT_SET_NAME} drop\n"
        ));
        script.push_str("  }\n");
        script.push_str("}\n");

        self.run_script(&script).await
    }

    pub async fn delete_table(&self) -> Result<(), NftError> {
        let script = format!("delete table inet {TABLE_NAME}\n");
        let _ = self.run_script(&script).await;
        Ok(())
    }

    pub async fn add_addresses(&self, ips: &[String], set: &str) -> Result<(), NftError> {
        for ip in ips {
            self.add_address(ip, set, None).await?;
        }
        Ok(())
    }

    pub async fn add_address(
        &self,
        ip: &str,
        set: &str,
        timeout_seconds: Option<i64>,
    ) -> Result<(), NftError> {
        let (set_name, timeout) = match parse_ip_or_cidr(ip) {
            Some(IpFamily::V4) => (set.to_string(), timeout_seconds),
            Some(IpFamily::V6) => (format!("{set}6"), timeout_seconds),
            None => {
                warn!("Ignoring invalid nftables address: {ip}");
                return Ok(());
            }
        };

        let element = match timeout {
            Some(t) if t > 0 => format!("{{ {ip} timeout {t}s }}"),
            _ => format!("{{ {ip} }}"),
        };

        let script = format!("add element inet {TABLE_NAME} {set_name} {element}\n");
        self.run_script(&script).await
    }

    pub async fn remove_addresses(&self, ips: &[String], set: &str) -> Result<(), NftError> {
        for ip in ips {
            for name in [set.to_string(), format!("{set}6")] {
                let script = format!("delete element inet {TABLE_NAME} {name} {{ {ip} }}\n");
                if let Err(e) = self.run_script(&script).await {
                    warn!("Failed to remove {ip} from {name}: {e}");
                }
            }
        }
        Ok(())
    }

    pub async fn add_ports(&self, ports: &[u16], set: &str) -> Result<(), NftError> {
        if ports.is_empty() {
            return Ok(());
        }
        let list = ports
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let script = format!("add element inet {TABLE_NAME} {set} {{ {list} }}\n");
        self.run_script(&script).await
    }

    async fn run_script(&self, script: &str) -> Result<(), NftError> {
        let mut child = Command::new("nft")
            .arg("-f")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(script.as_bytes()).await?;
        }

        let output = child.wait_with_output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if self.logging {
                warn!("nft stderr: {}", stderr.trim());
            }
            if script.starts_with("delete ") {
                return Ok(());
            }
            return Err(NftError::Command(stderr.trim().to_string()));
        }
        Ok(())
    }
}

enum IpFamily {
    V4,
    V6,
}

fn parse_ip_or_cidr(value: &str) -> Option<IpFamily> {
    let addr = value.split('/').next().unwrap_or(value);
    match addr.parse::<IpAddr>() {
        Ok(IpAddr::V4(_)) => Some(IpFamily::V4),
        Ok(IpAddr::V6(_)) => Some(IpFamily::V6),
        Err(_) => None,
    }
}
