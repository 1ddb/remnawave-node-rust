use std::collections::BTreeSet;

use rw_contract::models::{NodeSystemInfo, NodeSystemStats};

pub fn get_system_info() -> NodeSystemInfo {
    let uname = uname();
    let cpu_model = read_cpu_model();
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get() as i64)
        .unwrap_or(1);

    NodeSystemInfo {
        arch: node_arch(),
        cpus,
        cpu_model,
        memory_total: total_memory_bytes() as f64,
        hostname: hostname(),
        platform: std::env::consts::OS.to_string(),
        release: uname.release,
        type_: uname.sysname,
        version: uname.version,
        network_interfaces: network_interfaces(),
    }
}

pub fn get_system_stats() -> NodeSystemStats {
    let (total, free) = memory_info();
    NodeSystemStats {
        memory_free: free as f64,
        memory_used: total.saturating_sub(free) as f64,
        uptime: uptime_seconds(),
        load_avg: load_avg(),
        interface: None,
    }
}

fn node_arch() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "x64".to_string(),
        "aarch64" => "arm64".to_string(),
        "x86" => "ia32".to_string(),
        "arm" => "arm".to_string(),
        other => other.to_string(),
    }
}

fn memory_info() -> (u64, u64) {
    let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") else {
        return (0, 0);
    };
    let mut total = 0u64;
    let mut free = 0u64;
    let mut available = None;
    for line in meminfo.lines() {
        if let Some(v) = line.strip_prefix("MemTotal:") {
            total = parse_kb(v);
        } else if let Some(v) = line.strip_prefix("MemFree:") {
            free = parse_kb(v);
        } else if let Some(v) = line.strip_prefix("MemAvailable:") {
            available = Some(parse_kb(v));
        }
    }
    (total, available.unwrap_or(free))
}

fn total_memory_bytes() -> u64 {
    memory_info().0
}

fn parse_kb(value: &str) -> u64 {
    value
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse::<u64>()
        .unwrap_or(0)
        * 1024
}

fn uptime_seconds() -> f64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
        .unwrap_or(0.0)
}

fn load_avg() -> Vec<f64> {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .map(|s| {
            s.split_whitespace()
                .take(3)
                .filter_map(|v| v.parse().ok())
                .collect()
        })
        .unwrap_or_else(|| vec![0.0, 0.0, 0.0])
}

fn read_cpu_model() -> String {
    let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") else {
        return "unknown".into();
    };
    for line in cpuinfo.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim() == "model name" || key.trim() == "Hardware" || key.trim() == "Processor" {
                return value.trim().to_string();
            }
        }
    }
    "unknown".into()
}

fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: buf is a valid writable buffer of the given length.
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if rc == 0 {
        if let Some(end) = buf.iter().position(|&b| b == 0) {
            return String::from_utf8_lossy(&buf[..end]).into_owned();
        }
    }
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

fn network_interfaces() -> Vec<String> {
    let mut set = BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir("/sys/class/net") {
        for entry in entries.flatten() {
            set.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }
    set.into_iter().collect()
}

struct Uname {
    sysname: String,
    release: String,
    version: String,
}

fn uname() -> Uname {
    let mut buf: libc::utsname = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::uname(&mut buf) };
    if rc != 0 {
        return Uname {
            sysname: std::env::consts::OS.into(),
            release: "unknown".into(),
            version: "unknown".into(),
        };
    }
    let cstr = |p: &[libc::c_char]| {
        let bytes: Vec<u8> = p.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
        String::from_utf8_lossy(&bytes).into_owned()
    };
    Uname {
        sysname: cstr(&buf.sysname),
        release: cstr(&buf.release),
        version: cstr(&buf.version),
    }
}
