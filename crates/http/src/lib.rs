pub mod error;
pub mod middleware;
pub mod routes;
pub mod state;
pub mod tls;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::http::Request as HyperRequest;
use axum::routing::{get, post};
use axum::Router;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::service::TowerToHyperService;
use rw_contract::routes as r;
use rustls::ServerConfig;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tracing::{error, info};

pub use state::AppState;

pub(crate) const MAX_BODY: usize = 1000 * 1024 * 1024;

pub fn build_public_router(state: AppState) -> Router {
    let router = Router::new()
        .route(r::xray::START_URL, post(routes::xray::start))
        .route(r::xray::STOP_URL, get(routes::xray::stop))
        .route(r::xray::NODE_HEALTH_CHECK_URL, get(routes::xray::healthcheck))
        .route(
            r::stats::GET_USER_ONLINE_STATUS_URL,
            post(routes::stats::user_online_status),
        )
        .route(
            r::stats::GET_USERS_STATS_URL,
            post(routes::stats::users_stats),
        )
        .route(
            r::stats::GET_SYSTEM_STATS_URL,
            get(routes::stats::system_stats),
        )
        .route(
            r::stats::GET_INBOUND_STATS_URL,
            post(routes::stats::inbound_stats),
        )
        .route(
            r::stats::GET_OUTBOUND_STATS_URL,
            post(routes::stats::outbound_stats),
        )
        .route(
            r::stats::GET_ALL_OUTBOUNDS_STATS_URL,
            post(routes::stats::all_outbounds_stats),
        )
        .route(
            r::stats::GET_ALL_INBOUNDS_STATS_URL,
            post(routes::stats::all_inbounds_stats),
        )
        .route(
            r::stats::GET_COMBINED_STATS_URL,
            post(routes::stats::combined_stats),
        )
        .route(
            r::stats::GET_USER_IP_LIST_URL,
            post(routes::stats::user_ip_list),
        )
        .route(
            r::stats::GET_USERS_IP_LIST_URL,
            get(routes::stats::users_ip_list),
        )
        .route(
            r::stats::GET_GEOCHECK_URL,
            post(routes::stats::geocheck),
        )
        .route(r::handler::ADD_USER_URL, post(routes::handler::add_user))
        .route(
            r::handler::REMOVE_USER_URL,
            post(routes::handler::remove_user),
        )
        .route(r::handler::ADD_USERS_URL, post(routes::handler::add_users))
        .route(
            r::handler::REMOVE_USERS_URL,
            post(routes::handler::remove_users),
        )
        .route(
            r::handler::DROP_USERS_CONNECTIONS_URL,
            post(routes::handler::drop_users_connections),
        )
        .route(r::handler::DROP_IPS_URL, post(routes::handler::drop_ips))
        .route(r::plugin::SYNC_URL, post(routes::plugin::sync))
        .route(
            r::plugin::TORRENT_BLOCKER_COLLECT_URL,
            post(routes::plugin::collect_reports),
        )
        .route(
            r::plugin::NFTABLES_BLOCK_IPS_URL,
            post(routes::plugin::block_ips),
        )
        .route(
            r::plugin::NFTABLES_UNBLOCK_IPS_URL,
            post(routes::plugin::unblock_ips),
        )
        .route(
            r::plugin::NFTABLES_RECREATE_TABLES_URL,
            post(routes::plugin::recreate_tables),
        )
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(axum::middleware::from_fn(middleware::zstd_body))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::jwt_auth,
        ))
        .with_state(state);

    router
        .layer(CompressionLayer::new())
        .layer(axum::middleware::from_fn(middleware::security_headers))
}

pub fn build_internal_router(state: AppState) -> Router {
    Router::new()
        .route(r::internal::XRAY_INTERNAL_FULL_PATH, get(routes::internal::get_xray_config))
        .route(
            r::internal::XRAY_INTERNAL_FULL_WEBHOOK_PATH,
            post(routes::internal::webhook),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::internal_token,
        ))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(state)
}

pub async fn serve(
    state: AppState,
    tls_config: Arc<ServerConfig>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let public_router = build_public_router(state.clone());
    let internal_router = build_internal_router(state.clone());

    let bind = format!("0.0.0.0:{}", state.config.node_port);
    let listener = TcpListener::bind(&bind).await?;
    info!("HTTPS API listening on {bind}");

    let internal_listener = abstract_listener(&state.config.internal_socket_path)?;
    info!(
        "Internal API listening on @{}",
        state.config.internal_socket_path
    );

    let acceptor = TlsAcceptor::from(tls_config);

    let public_task = tokio::spawn(async move {
        loop {
            let (stream, addr) = match listener.accept().await {
                Ok(v) => v,
                Err(e) => {
                    error!("HTTPS accept error: {e}");
                    continue;
                }
            };
            let acceptor = acceptor.clone();
            let router = public_router.clone();
            tokio::spawn(async move {
                let tls = match tokio::time::timeout(
                    Duration::from_secs(10),
                    acceptor.accept(stream),
                )
                .await
                {
                    Ok(Ok(tls)) => tls,
                    Ok(Err(e)) => {
                        error!("TLS handshake with {addr} failed: {e}");
                        return;
                    }
                    Err(_) => {
                        error!("TLS handshake with {addr} timed out");
                        return;
                    }
                };
                if let Err(e) = serve_connection(tls, router).await {
                    error!("Connection error: {e}");
                }
            });
        }
    });

    let internal_task = tokio::spawn(async move {
        loop {
            let (stream, _) = match internal_listener.accept().await {
                Ok(v) => v,
                Err(e) => {
                    error!("Internal accept error: {e}");
                    continue;
                }
            };
            let router = internal_router.clone();
            tokio::spawn(async move {
                if let Err(e) = serve_connection(stream, router).await {
                    error!("Internal connection error: {e}");
                }
            });
        }
    });

    tokio::select! {
        _ = shutdown_signal() => {},
        _ = public_task => {},
        _ = internal_task => {},
    }

    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {},
                    _ = terminate.recv() => {},
                }
            }
            Err(e) => {
                error!("Failed to install SIGTERM handler: {e}");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn serve_connection<I>(
    io: I,
    router: Router,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let service = ServiceBuilder::new()
        .map_request(|req: HyperRequest<Incoming>| req.map(Body::new))
        .service(router);
    let service = TowerToHyperService::new(service);

    hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
        .serve_connection_with_upgrades(TokioIo::new(io), service)
        .await?;
    Ok(())
}

fn abstract_listener(
    name: &str,
) -> Result<tokio::net::UnixListener, Box<dyn std::error::Error + Send + Sync>> {
    use std::os::linux::net::SocketAddrExt;

    let addr = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes())?;
    let listener = std::os::unix::net::UnixListener::bind_addr(&addr)?;
    listener.set_nonblocking(true)?;
    Ok(tokio::net::UnixListener::from_std(listener)?)
}
