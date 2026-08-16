use anyhow::Context;
use std::future::IntoFuture;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing_subscriber::EnvFilter;

use orion_api::{app, config::AppConfig, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let config = AppConfig::from_env().context("invalid API configuration")?;
    let bind_address = config.bind_address;
    let shutdown_timeout = config.shutdown_timeout;
    let state = AppState::connect(config)
        .await
        .context("API dependency startup failed")?;
    let app = app(state.clone());
    let listener = TcpListener::bind(bind_address)
        .await
        .with_context(|| format!("could not bind API listener to {bind_address}"))?;
    tracing::info!(%bind_address, "orion-api is ready");

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        })
        .into_future();
    tokio::pin!(server);
    let shutdown_signal = shutdown_signal(state.clone());
    tokio::pin!(shutdown_signal);

    tokio::select! {
        result = &mut server => match result {
            Ok(()) => tracing::info!("orion-api stopped cleanly"),
            Err(error) => return Err(error).context("API server failed"),
        },
        _ = &mut shutdown_signal => {
            let _ = shutdown_tx.send(());
            match tokio::time::timeout(shutdown_timeout, &mut server).await {
                Ok(Ok(())) => tracing::info!("orion-api stopped cleanly"),
                Ok(Err(error)) => return Err(error).context("API server failed"),
                Err(_) => tracing::error!(?shutdown_timeout, "API shutdown deadline exceeded"),
            }
        }
    }
    state.close().await;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("orion_api=info,tower_http=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();
}

async fn shutdown_signal(state: AppState) {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install Ctrl-C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "failed to install SIGTERM handler"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    state.mark_draining();
    tracing::info!("API is draining in-flight requests");
}
