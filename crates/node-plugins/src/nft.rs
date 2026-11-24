use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use crabtables::{concat_key, lang, Batch, Family, Field, SetElem, SetElemList, Socket};
use thiserror::Error;
use tracing::{error, warn};

pub const TABLE_NAME: &str = "remnanode";
pub const TABLE_NAME_V6: &str = "remnanode6";
pub const TORRENT_BLOCKER_SET_NAME: &str = "torrent-blocker";
pub const INGRESS_FILTER_IP_SET_NAME: &str = "ingress-filter-ip";
pub const EGRESS_FILTER_IP_SET_NAME: &str = "egress-filter-ip";
pub const EGRESS_FILTER_PORT_SET_NAME: &str = "egress-filter-port";

const PROTO_TCP: u8 = 6;
const PROTO_UDP: u8 = 17;

const PROCESSED_COUNTER: &str = "processed";
const CHAIN_INPUT: &str = "input";
const CHAIN_FORWARD: &str = "forward";
const CHAIN_OUTPUT: &str = "output";

#[derive(Debug, Error)]
pub enum NftError {
    #[error("nft command failed: {0}")]
    Command(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<crabtables::Error> for NftError {
    fn from(error: crabtables::Error) -> Self {
        NftError::Command(error.to_string())
    }
}

#[derive(Clone)]
pub struct NftManager {
    logging: bool,
    accept_reply_traffic: bool,
    available: bool,
}

impl NftManager {
    pub fn new(logging: bool, accept_reply_traffic: bool) -> Self {
        Self { logging, accept_reply_traffic, available: false }
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

    /// Recreate the whole ruleset. Existing tables are deleted first, so this
    /// is safe to call on startup and after plugin changes.
    pub async fn create_table(&self) -> Result<(), NftError> {
        let script = self.build_table_script();
        run_blocking(move || recreate_tables_sync(&script)).await
    }

    pub async fn delete_table(&self) -> Result<(), NftError> {
        run_blocking(delete_tables_sync).await
    }

    pub async fn add_addresses(&self, ips: &[String], set: &str) -> Result<(), NftError> {
        let elements = build_address_elements(ips, set, None)?;
        run_blocking(move || add_elements_sync(elements)).await
    }

    pub async fn add_address(
        &self,
        ip: &str,
        set: &str,
        timeout_seconds: Option<i64>,
    ) -> Result<(), NftError> {
        let timeout = match timeout_seconds {
            Some(seconds) if seconds < 0 => {
                return Err(NftError::Command(
                    "timeout must be a positive number of seconds".into(),
                ));
            }
            Some(0) | None => None,
            Some(seconds) => Some(Duration::from_secs(seconds as u64)),
        };
        let elements = build_address_elements(&[ip.to_string()], set, timeout)?;
        run_blocking(move || add_elements_sync(elements)).await
    }

    pub async fn remove_addresses(&self, ips: &[String], set: &str) -> Result<(), NftError> {
        let elements = build_address_elements(ips, set, None)?;
        run_blocking(move || delete_elements_sync(elements)).await
    }

    pub async fn add_ports(&self, ports: &[u16], set: &str) -> Result<(), NftError> {
        if ports.is_empty() {
            return Ok(());
        }
        let elements = build_port_elements(ports, set);
        run_blocking(move || add_elements_sync(elements)).await
    }

    fn build_table_script(&self) -> String {
        let mut script = String::new();
        script.push_str(&table_block(
            TABLE_NAME,
            "",
            "ip",
            self.logging,
            self.accept_reply_traffic,
        ));
        script.push_str(&table_block(
            TABLE_NAME_V6,
            "6",
            "ip6",
            self.logging,
            self.accept_reply_traffic,
        ));
        script
    }
}

fn table_block(
    table: &str,
    suffix: &str,
    family_name: &str,
    logging: bool,
    accept_reply_traffic: bool,
) -> String {
    let torrent = format!("{TORRENT_BLOCKER_SET_NAME}{suffix}");
    let ingress = format!("{INGRESS_FILTER_IP_SET_NAME}{suffix}");
    let egress_ip = format!("{EGRESS_FILTER_IP_SET_NAME}{suffix}");
    let egress_port = format!("{EGRESS_FILTER_PORT_SET_NAME}{suffix}");
    let addr_type = if suffix.is_empty() { "ipv4_addr" } else { "ipv6_addr" };
    let saddr = if suffix.is_empty() { "ip saddr" } else { "ip6 saddr" };
    let daddr = if suffix.is_empty() { "ip daddr" } else { "ip6 daddr" };

    let ingress_rule = |set: &str| {
        if logging {
            format!("    {saddr} @{set} log prefix \"{set}: \" counter name \"{set}\" drop\n")
        } else {
            format!("    {saddr} @{set} counter name \"{set}\" drop\n")
        }
    };
    let reply_rule = if accept_reply_traffic { "    ct direction reply accept\n" } else { "" };

    let mut script = String::new();
    script.push_str(&format!("table {family_name} {table} {{\n"));

    script.push_str(&format!("  counter {PROCESSED_COUNTER} {{ }}\n"));
    for name in [&torrent, &ingress, &egress_ip, &egress_port] {
        script.push_str(&format!("  counter {name} {{ }}\n"));
    }

    for name in [&torrent, &ingress, &egress_ip] {
        script.push_str(&format!(
            "  set {name} {{ type {addr_type}; flags interval,timeout; counter; }}\n"
        ));
    }
    script.push_str(&format!(
        "  set {egress_port} {{ type inet_proto . inet_service; flags timeout; counter; }}\n"
    ));

    script.push_str(&format!(
        "  chain {CHAIN_INPUT} {{\n    type filter hook input priority -10; policy accept;\n"
    ));
    script.push_str(&format!("    counter name \"{PROCESSED_COUNTER}\"\n{reply_rule}"));
    script.push_str(&ingress_rule(&ingress));
    script.push_str(&ingress_rule(&torrent));
    script.push_str("  }\n");

    script.push_str(&format!(
        "  chain {CHAIN_FORWARD} {{\n    type filter hook forward priority -10; policy accept;\n"
    ));
    script.push_str(&format!("    counter name \"{PROCESSED_COUNTER}\"\n{reply_rule}"));
    script.push_str(&ingress_rule(&ingress));
    script.push_str(&ingress_rule(&torrent));
    script.push_str("  }\n");

    script.push_str(&format!(
        "  chain {CHAIN_OUTPUT} {{\n    type filter hook output priority -10; policy accept;\n"
    ));
    script.push_str(&format!("    {daddr} @{egress_ip} counter name \"{egress_ip}\" drop\n"));
    script.push_str(&format!(
        "    meta l4proto . th dport @{egress_port} counter name \"{egress_port}\" drop\n"
    ));
    script.push_str("  }\n");

    script.push_str("}\n");
    script
}

#[derive(Debug)]
struct Elements {
    family: Family,
    table: String,
    set: String,
    elems: Vec<SetElem>,
}

fn build_address_elements(
    values: &[String],
    set: &str,
    timeout: Option<Duration>,
) -> Result<Vec<Elements>, NftError> {
    let mut v4: Vec<(u128, u128)> = Vec::new();
    let mut v6: Vec<(u128, u128)> = Vec::new();

    for value in values {
        match parse_ip_or_cidr(value) {
            Some((Family::Ipv4, start, end)) => v4.push((start, end)),
            Some((Family::Ipv6, start, end)) => v6.push((start, end)),
            _ => {
                return Err(NftError::Command(format!("invalid IP address or CIDR: {value}")));
            }
        }
    }

    let mut batches = Vec::new();
    push_address_elements(
        &mut batches,
        Family::Ipv4,
        TABLE_NAME,
        set,
        "",
        merge_ranges(v4, u32::MAX as u128),
        timeout,
    )?;
    push_address_elements(
        &mut batches,
        Family::Ipv6,
        TABLE_NAME_V6,
        set,
        "6",
        merge_ranges(v6, u128::MAX),
        timeout,
    )?;
    Ok(batches)
}

fn push_address_elements(
    batches: &mut Vec<Elements>,
    family: Family,
    table: &str,
    set: &str,
    suffix: &str,
    ranges: Vec<(u128, u128)>,
    timeout: Option<Duration>,
) -> Result<(), NftError> {
    if ranges.is_empty() {
        return Ok(());
    }
    let mut elems = Vec::new();
    for (start, end) in ranges {
        let start = family_bytes(family, start)
            .ok_or_else(|| NftError::Command("address outside its family".into()))?;
        let end = family_bytes(family, end)
            .ok_or_else(|| NftError::Command("address outside its family".into()))?;
        let mut range =
            SetElem::range(&start, &end, None).map_err(|e| NftError::Command(e.to_string()))?;
        if let Some(timeout) = timeout {
            for element in &mut range {
                if !element.is_interval_end() {
                    element.timeout = Some(timeout);
                }
            }
        }
        elems.extend(range);
    }

    batches.push(Elements {
        family,
        table: table.to_string(),
        set: format!("{set}{suffix}"),
        elems,
    });
    Ok(())
}

fn build_port_elements(ports: &[u16], set: &str) -> Vec<Elements> {
    let mut seen = HashSet::new();
    let mut elems = Vec::new();
    for port in ports {
        for proto in [PROTO_TCP, PROTO_UDP] {
            if !seen.insert((proto, *port)) {
                continue;
            }
            let key = concat_key(&[
                (&Field::L4Proto, vec![proto]),
                (&Field::ThDport, port.to_be_bytes().to_vec()),
            ]);
            elems.push(SetElem::new(key));
        }
    }

    vec![
        Elements {
            family: Family::Ipv4,
            table: TABLE_NAME.to_string(),
            set: set.to_string(),
            elems: elems.clone(),
        },
        Elements {
            family: Family::Ipv6,
            table: TABLE_NAME_V6.to_string(),
            set: format!("{set}6"),
            elems,
        },
    ]
}

fn add_elements_sync(elements: Vec<Elements>) -> Result<(), NftError> {
    let mut socket = Socket::new()?;
    for batch in elements {
        if batch.elems.is_empty() {
            continue;
        }
        let list = SetElemList {
            family: batch.family,
            table: batch.table,
            set: batch.set,
            set_id: None,
            elems: batch.elems,
        };
        let mut transaction = Batch::new();
        transaction.add_elems(&list);
        socket.commit(&transaction)?;
    }
    Ok(())
}

fn delete_elements_sync(elements: Vec<Elements>) -> Result<(), NftError> {
    let mut socket = Socket::new()?;
    for batch in elements {
        // Interval sets store a start key plus an end marker. They must be
        // deleted together, otherwise the kernel rejects the operation.
        for group in group_interval_elements(batch.elems) {
            let list = SetElemList {
                family: batch.family,
                table: batch.table.clone(),
                set: batch.set.clone(),
                set_id: None,
                elems: group,
            };
            let mut transaction = Batch::new();
            transaction.delete_elems(&list);
            // Node ignores ENOENT for removals, so a missing element must not
            // make unblock/recreate operations fail.
            if let Err(e) = socket.commit(&transaction) {
                warn!("Failed to remove element from {}.{}: {e}", batch.table, batch.set);
            }
        }
    }
    Ok(())
}

fn group_interval_elements(elements: Vec<SetElem>) -> Vec<Vec<SetElem>> {
    let mut groups = Vec::new();
    let mut current = Vec::new();
    for element in elements {
        let is_end = element.is_interval_end();
        current.push(element);
        if is_end {
            groups.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

fn recreate_tables_sync(script: &str) -> Result<(), NftError> {
    let mut socket = Socket::new()?;
    delete_existing_tables(&mut socket)?;

    let commands = lang::parse::parse(script).map_err(|e| NftError::Command(e.to_string()))?;
    let (batch, _) =
        lang::compile::compile(&commands).map_err(|e| NftError::Command(e.to_string()))?;
    socket.commit(&batch)?;
    Ok(())
}

fn delete_tables_sync() -> Result<(), NftError> {
    let mut socket = Socket::new()?;
    delete_existing_tables(&mut socket)
}

fn delete_existing_tables(socket: &mut Socket) -> Result<(), NftError> {
    for (family, name) in [
        (Family::Ipv4, TABLE_NAME),
        (Family::Ipv6, TABLE_NAME_V6),
        // Legacy table created by older rw-node builds.
        (Family::Inet, TABLE_NAME),
    ] {
        let exists = socket.list_tables(family)?.iter().any(|table| table.name == name);
        if !exists {
            continue;
        }
        let mut transaction = Batch::new();
        transaction.delete_table(family, name);
        socket.commit(&transaction)?;
    }
    Ok(())
}

async fn run_blocking<T, F>(func: F) -> Result<T, NftError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, NftError> + Send + 'static,
{
    tokio::task::spawn_blocking(func)
        .await
        .map_err(|e| NftError::Command(format!("nft task failed: {e}")))?
}

fn parse_ip_or_cidr(value: &str) -> Option<(Family, u128, u128)> {
    if let Some((addr, prefix)) = value.split_once('/') {
        if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let ip: IpAddr = addr.parse().ok()?;
        let prefix: u32 = prefix.parse().ok()?;
        match ip {
            IpAddr::V4(ip) => {
                if prefix > 32 {
                    return None;
                }
                let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
                let start = u32::from(ip) & mask;
                let end = start | !mask;
                Some((Family::Ipv4, start as u128, end as u128))
            }
            IpAddr::V6(ip) => {
                if prefix > 128 {
                    return None;
                }
                let mask = if prefix == 0 { 0 } else { u128::MAX << (128 - prefix) };
                let start = u128::from(ip) & mask;
                let end = start | !mask;
                Some((Family::Ipv6, start, end))
            }
        }
    } else {
        let ip: IpAddr = value.parse().ok()?;
        match ip {
            IpAddr::V4(ip) => {
                let value = u32::from(ip) as u128;
                Some((Family::Ipv4, value, value))
            }
            IpAddr::V6(ip) => {
                let value = u128::from(ip);
                Some((Family::Ipv6, value, value))
            }
        }
    }
}

fn merge_ranges(mut ranges: Vec<(u128, u128)>, max: u128) -> Vec<(u128, u128)> {
    ranges.sort_unstable();
    let mut merged: Vec<(u128, u128)> = Vec::with_capacity(ranges.len());

    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            let adjacent = last.1 < max && start == last.1 + 1;
            if start <= last.1 || adjacent {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

fn family_bytes(family: Family, value: u128) -> Option<Vec<u8>> {
    match family {
        Family::Ipv4 => {
            u32::try_from(value).ok().map(|value| Ipv4Addr::from(value).octets().to_vec())
        }
        Family::Ipv6 => Some(Ipv6Addr::from(value).octets().to_vec()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_script(script: &str) {
        let commands = lang::parse::parse(script).expect("script parses");
        lang::compile::compile(&commands).expect("script compiles");
    }

    #[test]
    fn generated_ruleset_is_valid_nft() {
        for logging in [true, false] {
            for accept in [true, false] {
                let manager = NftManager::new(logging, accept);
                compile_script(&manager.build_table_script());
            }
        }
    }

    #[test]
    fn parses_and_merges_cidrs() {
        assert_eq!(parse_ip_or_cidr("10.0.0.0/24"), Some((Family::Ipv4, 0x0a00_0000, 0x0a00_00ff)));
        assert_eq!(parse_ip_or_cidr("10.0.0.1"), Some((Family::Ipv4, 0x0a00_0001, 0x0a00_0001)));
        assert!(parse_ip_or_cidr("1.2.3.4/8 } flush ruleset").is_none());
        assert!(parse_ip_or_cidr("2001:db8::/129").is_none());

        let merged =
            merge_ranges(vec![(1, 5), (6, 10), (20, 30), (25, 40), (45, 50)], u32::MAX as u128);
        assert_eq!(merged, vec![(1, 10), (20, 40), (45, 50)]);
    }

    #[test]
    fn builds_v4_and_v6_address_elements() {
        let elements = build_address_elements(
            &["10.0.0.0/24".to_string(), "10.0.0.128/25".to_string(), "2001:db8::/64".to_string()],
            TORRENT_BLOCKER_SET_NAME,
            Some(Duration::from_secs(60)),
        )
        .unwrap();
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].family, Family::Ipv4);
        assert_eq!(elements[0].elems.len(), 2); // merged /24 -> start + end marker
        assert_eq!(elements[1].family, Family::Ipv6);
    }
}
