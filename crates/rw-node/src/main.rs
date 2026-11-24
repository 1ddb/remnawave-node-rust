mod cli;

use std::sync::Arc;

use rw_config::{derive_sni, ensure_crypto_provider, init_runtime_env, run_payload_checks};
use rw_core::network_stats::NetworkStats;
use rw_core::render_box::{render_box, Align, BoxOptions};
use rw_core::system_stats::get_system_info;
use rw_http::{serve, tls, AppState};
use rw_xray::{XrayService, XrayServiceConfig};
use tracing::{error, info};

fn main() {
    init_tracing();

    if let Some(action) = cli::parse_action() {
        ensure_crypto_provider();
        if let Err(e) = cli::run(action) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }

    ensure_crypto_provider();

    init_runtime_env();

    let config = match rw_config::load() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Environment configuration error: {e}");
            std::process::exit(1);
        }
    };

    // Acquire the instance lock before touching nftables, otherwise a duplicate
    // process would clobber the state of the instance that is already running.
    // Like upstream, a duplicate instance is only warned about (unless
    // suppressed) and is allowed to continue.
    let _lock = acquire_instance_lock(&config);

    log_payload_report(&config);

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("Failed to start tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    runtime.block_on(async move {
        let config = Arc::new(config);

        let tls_config = match tls::build_server_config(
            &config.payload.ca_cert_pem,
            &config.payload.node_cert_pem,
            &config.payload.node_key_pem,
            &config.jwt_public_key,
            config.sni_verification,
        ) {
            Ok(config) => config,
            Err(e) => {
                error!("Failed to build TLS configuration: {e}");
                std::process::exit(1);
            }
        };

        let network = NetworkStats::new();
        network.start().await;

        let asn = rw_asn_lmdb::AsnLmdb::open_default().map(Arc::new);

        let xray = XrayService::new(
            XrayServiceConfig {
                internal_socket_path: config.internal_socket_path.clone(),
                internal_rest_token: config.internal_rest_token.clone(),
                xtls_api_socket_path: config.xtls_api_socket_path.clone(),
                disable_hashed_set_check: config.disable_hashed_set_check,
                nftables_logging: config.nftables_logging,
                nftables_accept_reply_traffic: config.nftables_accept_reply_traffic,
                nftables_table_name: config.nftables_table_name.clone(),
                core_link: rw_xray::process::CORE_LINK.to_string(),
            },
            network.clone(),
            asn,
        )
        .await;
        let xray = Arc::new(xray);
        xray.start_watchdog();

        let state = match AppState::new(config.clone(), xray.clone(), network.clone()) {
            Ok(state) => state,
            Err(e) => {
                error!("Invalid JWT public key: {e}");
                xray.kill_all().await;
                std::process::exit(1);
            }
        };

        let start_message = build_start_message(&config, &xray, &network).await;
        info!("\n{start_message}\n");

        if let Err(e) = serve(state, tls_config).await {
            error!("Server error: {e}");
            xray.kill_all().await;
            std::process::exit(1);
        }

        xray.kill_all().await;
    });
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let default =
        if std::env::var("NODE_ENV").as_deref() == Ok("development") { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    tracing_subscriber::fmt().with_env_filter(filter).with_target(false).with_level(true).init();
}

fn log_payload_report(config: &rw_config::AppConfig) {
    let checks = run_payload_checks(&config.payload);
    let all_ok = checks.iter().all(|c| c.ok);

    let rows: Vec<String> = checks
        .iter()
        .map(|c| format!("{} {}  {}", if c.ok { '✓' } else { '✗' }, c.name, c.detail))
        .collect();
    let mut sections = vec![rows.join("\n")];

    if all_ok {
        let sni = derive_sni(&config.payload.ca_cert_pem, &config.payload.jwt_public_key);
        sections.push(sni);
        info!(
            "\n{}\n",
            render_box("SECRET_KEY OK", &sections, BoxOptions { align: Align::Left, width: 62 })
        );
    } else {
        error!(
            "\n{}\n",
            render_box(
                "SECRET_KEY INVALID",
                &sections,
                BoxOptions { align: Align::Left, width: 62 }
            )
        );
        std::process::exit(1);
    }
}

async fn build_start_message(
    config: &rw_config::AppConfig,
    xray: &XrayService,
    _network: &NetworkStats,
) -> String {
    let (version, _) = xray.get_xray_info().await;
    let system = get_system_info();
    let node_version = option_env!("RWNODE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));

    render_box(
        &format!("Remnawave Node v{node_version}"),
        &[
            "Docs → https://docs.rw\nCommunity → https://t.me/remnawave".to_string(),
            format!("API Port: {}", config.node_port),
            format!(
                "XRay Core: v{}\nXRay Path: /usr/local/bin/xray",
                version.unwrap_or_else(|| "N/A".into())
            ),
            format!("{}C, {}, {} bytes", system.cpus, system.cpu_model, system.memory_total),
            format!("Kernel: {} {} {}", system.release, system.type_, system.platform),
            format!("Network Interfaces: {}", system.network_interfaces.join(", ")),
        ],
        BoxOptions { align: Align::Center, width: 62 },
    )
}

fn acquire_instance_lock(
    config: &rw_config::AppConfig,
) -> Option<std::os::unix::net::UnixListener> {
    use std::os::linux::net::SocketAddrExt;

    let Ok(addr) = std::os::unix::net::SocketAddr::from_abstract_name(b"rwnode-lock") else {
        return None;
    };
    match std::os::unix::net::UnixListener::bind_addr(&addr) {
        Ok(listener) => Some(listener),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            if !config.suppress_duplicate_instance_warning {
                error!("\n{}\n", get_duplicate_instance_message());
            }
            None
        }
        Err(_) => None,
    }
}

fn get_duplicate_instance_message() -> String {
    let rendered = render_box(
        "ANOTHER REMNAWAVE NODE IS ALREADY RUNNING",
        &[
            "A second instance was detected in this network namespace. Both act on the \
             same host-level state, which leads to unexpected behaviour that is hard to \
             trace back to its cause."
                .to_string(),
            "Running more than one node per network namespace is not supported and is \
             strongly discouraged. Use one node per host, or give each container a \
             namespace of its own."
                .to_string(),
        ],
        BoxOptions { align: Align::Left, width: 76 },
    );

    if std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty()) {
        return rendered;
    }

    rendered
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i == 1 {
                format!("\x1b[1;31m{line}\x1b[0m")
            } else {
                format!("\x1b[31m{line}\x1b[0m")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
