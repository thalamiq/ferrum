//! FHIR Server - Web Server Entry Point
//!
//! This binary starts the HTTP server that handles FHIR API requests.
//! For background workers, use the `fhir-worker` binary.

use anyhow::Context;
use zunder::{api::create_router, config::Config, logging, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration first to get logging settings
    let config = Config::load().context("Failed to load configuration")?;

    // Validate configuration
    config
        .validate()
        .map_err(|e| anyhow::anyhow!("Invalid configuration: {e}"))?;

    // Initialize logging based on configuration
    let _telemetry_guard =
        logging::init_logging(&config.logging).context("Failed to initialize logging/telemetry")?;

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        environment = config.logging.deployment_environment,
        "Starting FHIR Server"
    );

    let addr = config
        .socket_addr()
        .context("Failed to determine socket address")?;

    tracing::info!(
        fhir_version = config.fhir.version,
        listen_addr = %addr,
        "Configuration loaded"
    );

    // Initialize application state (includes FHIR packages for server)
    let state = AppState::new(config)
        .await
        .context("Failed to initialize application state")?;

    // Create router
    let app = create_router(state);

    // Start server
    tracing::info!("FHIR Server listening on http://{}", addr);
    tracing::info!("Health check: http://{}/health", addr);
    tracing::info!("API endpoint: http://{}/fhir", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind TCP listener on {addr}"))?;

    // Run server with graceful shutdown
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        tracing::error!(error = %e, "Server terminated unexpectedly");
        logging::shutdown_telemetry();
        return Err(e.into());
    }

    tracing::info!("Server shutdown complete");

    // Explicitly shutdown telemetry (also happens via Drop on _telemetry_guard)
    logging::shutdown_telemetry();

    Ok(())
}

/// Wait for shutdown signal (SIGTERM or SIGINT)
/// Docker sends SIGTERM, while Ctrl+C sends SIGINT
#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm =
        signal(SignalKind::terminate()).expect("Failed to install SIGTERM signal handler");
    let sigint = tokio::signal::ctrl_c();

    tokio::select! {
        _ = sigint => {
            tracing::info!("SIGINT received, starting graceful shutdown...");
        }
        _ = sigterm.recv() => {
            tracing::info!("SIGTERM received, starting graceful shutdown...");
        }
    }
}

/// Wait for shutdown signal (SIGINT only on non-Unix platforms)
#[cfg(not(unix))]
async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutdown signal received, starting graceful shutdown...");
}
