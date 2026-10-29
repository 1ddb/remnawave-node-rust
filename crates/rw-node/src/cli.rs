use std::io::{Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::UnixStream;

use rw_config::resolve_runtime_var;

pub enum Action {
    DumpConfig { raw: bool },
    KillSockets(Option<String>),
    Help,
}

pub fn parse_action() -> Option<Action> {
    let mut args: Vec<String> = std::env::args().collect();
    let argv0 = args.first().cloned().unwrap_or_default();
    let invoked_as_cli = argv0.rsplit('/').next().is_some_and(|n| n == "cli");
    if invoked_as_cli {
        args.remove(0);
    } else {
        if args.get(1).map(String::as_str) != Some("cli") {
            return None;
        }
        args.drain(0..2);
    }

    let arg = args.first().map(String::as_str);
    match arg {
        Some("--dump-config") | Some("-d") => Some(Action::DumpConfig { raw: false }),
        Some("--dump-config-raw") | Some("-D") => Some(Action::DumpConfig { raw: true }),
        Some("--kill-sockets") | Some("-k") => Some(Action::KillSockets(args.get(1).cloned())),
        Some("--help") | Some("-h") | None => Some(Action::Help),
        Some(other) => {
            eprintln!("Unknown command: {other}");
            Some(Action::Help)
        }
    }
}

pub fn run(action: Action) -> Result<(), String> {
    match action {
        Action::DumpConfig { raw } => dump_config(raw),
        Action::KillSockets(ip) => kill_sockets(ip),
        Action::Help => {
            println!(
                "Usage: cli [command]\n\n\
                 Commands:\n\
                 \x20 --dump-config, -d         Dump current XRay configuration (pretty)\n\
                 \x20 --dump-config-raw, -D     Dump raw XRay configuration to stdout\n\
                 \x20 --kill-sockets, -k <ip>   Kill sockets by IP address\n\
                 \x20 --help, -h                Show this help message"
            );
            Ok(())
        }
    }
}

fn dump_config(raw: bool) -> Result<(), String> {
    let socket_path = resolve_runtime_var("INTERNAL_SOCKET_PATH")
        .ok_or_else(|| "Missing environment variables.".to_string())?;
    let token = resolve_runtime_var("INTERNAL_REST_TOKEN")
        .ok_or_else(|| "Missing environment variables.".to_string())?;

    if !raw {
        eprintln!("Dumping cold XRay configuration...");
    }

    let (status, body) = request_over_socket(
        &socket_path,
        &format!("/internal/get-config?token={}", percent_encoding::utf8_percent_encode(&token, percent_encoding::NON_ALPHANUMERIC)),
    )?;

    if status != 200 {
        return Err(format!("Failed to fetch configuration with status {status}"));
    }

    if raw {
        print!("{body}");
        if !body.ends_with('\n') {
            println!();
        }
        return Ok(());
    }

    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {e}"))?;
    if value.as_object().map_or(false, |o| o.is_empty()) {
        eprintln!("Configuration is empty.");
        return Ok(());
    }
    println!("Configuration retrieved successfully!\n");
    println!("{}", serde_json::to_string_pretty(&value).unwrap_or(body));
    println!("Configuration dumped successfully!");
    Ok(())
}

fn request_over_socket(socket_path: &str, path: &str) -> Result<(u16, String), String> {
    let addr = std::os::unix::net::SocketAddr::from_abstract_name(socket_path.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut stream = UnixStream::connect_addr(&addr).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .ok();

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nAccept: application/json\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| e.to_string())?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&response).into_owned();

    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| "malformed HTTP response".to_string())?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    Ok((status, body.to_string()))
}

fn kill_sockets(ip: Option<String>) -> Result<(), String> {
    let ip = match ip {
        Some(ip) => ip,
        None => {
            eprint!("Enter IP address to kill sockets for: ");
            std::io::stdout()
                .flush()
                .map_err(|e| e.to_string())?;
            let mut buf = String::new();
            std::io::stdin()
                .read_line(&mut buf)
                .map_err(|e| e.to_string())?;
            let trimmed = buf.trim();
            if trimmed.is_empty() {
                return Err("IP address is required".into());
            }
            trimmed.to_string()
        }
    };

    let status = std::process::Command::new("ss")
        .args(["-K", "src", &ip])
        .status()
        .map_err(|e| e.to_string())?;
    let dst = std::process::Command::new("ss")
        .args(["-K", "dst", &ip])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() && dst.success() {
        println!("Sockets for {ip} processed.");
        Ok(())
    } else {
        Err("Failed to kill sockets".into())
    }
}
