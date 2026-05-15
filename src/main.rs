mod ai;
mod app;
mod config;
mod core;
mod erpdb;
mod erpnext;
mod error;
mod fcm;
#[cfg(test)]
mod fcm_tests;
mod http;
mod store;

use crate::app::AppState;
use crate::config::AppConfig;
use axum::serve::ListenerExt;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;
use std::num::NonZeroUsize;

#[tokio::main]
async fn main() -> Result<(), error::AppError> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = AppConfig::from_env()?;
    let bind_addr = config.bind_addr;
    let state = AppState::new(config);
    let app = http::router::build_router(state);

    tracing::info!(%bind_addr, "starting accord mobile server rs");
    let listener_count = listener_count();
    if listener_count == 1 {
        let listener = bind_tcp_listener(bind_addr)?;
        let listener = listener.tap_io(|tcp_stream| {
            if let Err(error) = tcp_stream.set_nodelay(true) {
                tracing::trace!(%error, "failed to enable TCP_NODELAY");
            }
        });
        axum::serve(listener, app).await?;
    } else {
        tracing::info!(listener_count, "starting reuseport listener workers");
        let mut handles = Vec::with_capacity(listener_count);
        for worker in 0..listener_count {
            let listener = bind_tcp_listener(bind_addr)?;
            let listener = listener.tap_io(|tcp_stream| {
                if let Err(error) = tcp_stream.set_nodelay(true) {
                    tracing::trace!(%error, "failed to enable TCP_NODELAY");
                }
            });
            let app = app.clone();
            handles.push(tokio::spawn(async move {
                tracing::info!(worker, "listener worker started");
                axum::serve(listener, app).await
            }));
        }
        for handle in handles {
            handle.await.map_err(|error| {
                error::AppError::Storage(format!("server task failed: {error}"))
            })??;
        }
    }

    Ok(())
}

fn listener_count() -> usize {
    std::env::var("MOBILE_API_LISTENER_COUNT")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .and_then(NonZeroUsize::new)
        .map(NonZeroUsize::get)
        .unwrap_or_else(default_listener_count)
}

fn default_listener_count() -> usize {
    #[cfg(unix)]
    {
        std::thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(1)
            .clamp(1, 8)
    }

    #[cfg(not(unix))]
    {
        1
    }
}

fn bind_tcp_listener(bind_addr: SocketAddr) -> Result<tokio::net::TcpListener, error::AppError> {
    let domain = if bind_addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&bind_addr.into())?;
    socket.listen(4096)?;

    let listener: std::net::TcpListener = socket.into();
    Ok(tokio::net::TcpListener::from_std(listener)?)
}
