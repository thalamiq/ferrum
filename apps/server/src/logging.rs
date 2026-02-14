//! Logging and OpenTelemetry initialization for FHIR server binaries
//!
//! Provides consistent logging setup with OpenTelemetry integration for distributed tracing.
//! Supports configuration-based logging with file rotation, JSON formatting, and
//! environment variable overrides.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    trace::{Sampler, TracerProvider},
    Resource,
};
use std::fs;
use std::time::Duration;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::config::LoggingConfig;

/// Guard type that ensures OpenTelemetry is properly shut down
/// Must be kept alive for the duration of the program
pub struct TelemetryGuard {
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

/// Initialize logging with full configuration support including OpenTelemetry
///
/// This function sets up logging based on the provided `LoggingConfig`, supporting:
/// - JSON or human-readable formats
/// - File logging with rotation (daily, hourly, minutely, never)
/// - Environment variable overrides via `RUST_LOG`
/// - OpenTelemetry integration for distributed tracing
/// - Trace context in all logs (trace_id, span_id)
///
/// Returns a `TelemetryGuard` that must be kept alive for the program duration
/// to ensure proper shutdown of OpenTelemetry resources.
pub fn init_logging(config: &LoggingConfig) -> anyhow::Result<TelemetryGuard> {
    // Build OpenTelemetry resource attributes
    let resource = build_resource_attributes(config);

    // Initialize OpenTelemetry tracer provider (if enabled)
    let mut otel_init_error: Option<String> = None;
    let tracer_provider = if config.opentelemetry_enabled {
        match init_tracer_provider(config, resource.clone()) {
            Ok(provider) => Some(provider),
            Err(e) => {
                otel_init_error = Some(e.to_string());
                None
            }
        }
    } else {
        None
    };

    // Build environment filter
    let env_filter = build_env_filter(config);

    // Initialize subscriber with or without OpenTelemetry
    let file_guard = if let Some(provider) = &tracer_provider {
        // With OpenTelemetry
        let tracer = provider.tracer("fhir-server");
        let otel_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_tracked_inactivity(true); // Close spans when dropped

        let subscriber = tracing_subscriber::registry()
            .with(env_filter)
            .with(otel_layer);

        // Add console/file logging layers
        if config.json {
            init_json_logging_with_subscriber(subscriber, config)?
        } else {
            init_human_logging_with_subscriber(subscriber, config)?
        }
    } else {
        // Without OpenTelemetry
        let subscriber = tracing_subscriber::registry().with(env_filter);

        if config.json {
            init_json_logging_with_subscriber(subscriber, config)?
        } else {
            init_human_logging_with_subscriber(subscriber, config)?
        }
    };

    // Set global tracer provider
    if let Some(provider) = tracer_provider {
        global::set_tracer_provider(provider);
    }

    if let Some(err) = otel_init_error {
        tracing::warn!(
            error = %err,
            "Failed to initialize OpenTelemetry tracer provider, continuing without OpenTelemetry"
        );
    }

    tracing::info!(
        otel_enabled = config.opentelemetry_enabled,
        service_name = %config.service_name,
        environment = %config.deployment_environment,
        "Logging initialized"
    );

    Ok(TelemetryGuard {
        _file_guard: file_guard,
    })
}

/// Build OpenTelemetry resource attributes
fn build_resource_attributes(config: &LoggingConfig) -> Resource {
    let service_version = config
        .service_version
        .clone()
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    Resource::new(vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", service_version),
        KeyValue::new(
            "deployment.environment",
            config.deployment_environment.clone(),
        ),
        KeyValue::new("telemetry.sdk.name", "opentelemetry"),
        KeyValue::new("telemetry.sdk.language", "rust"),
    ])
}

/// Initialize OpenTelemetry tracer provider with OTLP exporter
fn init_tracer_provider(
    config: &LoggingConfig,
    resource: Resource,
) -> anyhow::Result<TracerProvider> {
    use opentelemetry_sdk::trace::Config;

    // Build OTLP trace exporter
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(&config.otlp_endpoint)
        .with_timeout(Duration::from_secs(config.otlp_timeout_seconds))
        .build_span_exporter()
        .map_err(|e| anyhow::anyhow!("Failed to create OTLP exporter: {}", e))?;

    // Configure sampler based on sample ratio
    let sampler = if config.trace_sample_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else if config.trace_sample_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        // ParentBased sampler: respects parent decision, otherwise uses TraceIdRatio
        Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            config.trace_sample_ratio,
        )))
    };

    // Build trace config with sampler and resource
    let trace_config = Config::default()
        .with_sampler(sampler)
        .with_resource(resource);

    // Build tracer provider with batch span processor
    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_config(trace_config)
        .build();

    Ok(provider)
}

/// Build environment filter
fn build_env_filter(config: &LoggingConfig) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Suppress verbose PostgreSQL/sqlx debug logs by default
        // Include both fhir_server (binary) and ferrum (library crate) module paths
        EnvFilter::new(format!(
            "fhir_server={},ferrum={},tower_http=debug,sqlx=warn,tokio_postgres=warn,postgres=warn",
            config.level, config.level
        ))
    })
}

/// Initialize JSON logging with a pre-configured subscriber
fn init_json_logging_with_subscriber<S>(
    subscriber: S,
    config: &LoggingConfig,
) -> anyhow::Result<Option<tracing_appender::non_blocking::WorkerGuard>>
where
    S: SubscriberExt + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    let console_layer = fmt::layer()
        .json()
        .with_current_span(true) // Include span fields
        .with_span_list(false) // Don't include full span list (too verbose)
        .with_writer(std::io::stdout);

    if config.file_enabled {
        // Console + File logging (JSON)
        let (file_appender, file_guard) = create_file_appender(config)?;
        let file_layer = fmt::layer()
            .json()
            .with_current_span(true)
            .with_span_list(false)
            .with_writer(file_appender);

        subscriber.with(console_layer).with(file_layer).init();
        Ok(Some(file_guard))
    } else {
        // Console only (JSON)
        subscriber.with(console_layer).init();
        Ok(None)
    }
}

/// Initialize human-readable logging with a pre-configured subscriber
fn init_human_logging_with_subscriber<S>(
    subscriber: S,
    config: &LoggingConfig,
) -> anyhow::Result<Option<tracing_appender::non_blocking::WorkerGuard>>
where
    S: SubscriberExt + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    let console_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_writer(std::io::stdout);

    if config.file_enabled {
        // Console + File logging (human-readable)
        let (file_appender, file_guard) = create_file_appender(config)?;
        let file_layer = fmt::layer()
            .with_target(true)
            .with_ansi(false) // No ANSI colors in files
            .with_writer(file_appender);

        subscriber.with(console_layer).with(file_layer).init();
        Ok(Some(file_guard))
    } else {
        // Console only (human-readable)
        subscriber.with(console_layer).init();
        Ok(None)
    }
}

/// Create file appender with rotation
fn create_file_appender(
    config: &LoggingConfig,
) -> anyhow::Result<(
    tracing_appender::non_blocking::NonBlocking,
    tracing_appender::non_blocking::WorkerGuard,
)> {
    // Create log directory if it doesn't exist
    fs::create_dir_all(&config.file_directory)?;

    // Create rotating file appender
    let file_appender = match config.file_rotation.as_str() {
        "daily" => tracing_appender::rolling::daily(&config.file_directory, &config.file_prefix),
        "hourly" => tracing_appender::rolling::hourly(&config.file_directory, &config.file_prefix),
        "minutely" => {
            tracing_appender::rolling::minutely(&config.file_directory, &config.file_prefix)
        }
        "never" => tracing_appender::rolling::never(
            &config.file_directory,
            format!("{}.log", config.file_prefix),
        ),
        _ => tracing_appender::rolling::daily(&config.file_directory, &config.file_prefix),
    };

    // Use non-blocking writer to avoid blocking on I/O
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    Ok((non_blocking, guard))
}

/// Shutdown OpenTelemetry gracefully
///
/// Call this before process termination to ensure all spans/logs are exported.
/// The TelemetryGuard will automatically call this on drop, but you can call
/// it explicitly if you need to ensure immediate shutdown.
pub fn shutdown_telemetry() {
    tracing::info!("Shutting down OpenTelemetry...");
    global::shutdown_tracer_provider();
}

/// Initialize simple logging using only environment variables
///
/// This is a lightweight alternative for binaries that don't need full config support.
/// It uses `RUST_LOG` environment variable or defaults to reasonable values.
/// Useful for worker binaries that want minimal dependencies on config.
///
/// Note: This does not initialize OpenTelemetry. Use `init_logging` for full features.
pub fn init_simple_logging() {
    // Suppress verbose PostgreSQL/sqlx debug logs by default
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "fhir_server=info,ferrum=info,sqlx=warn,tokio_postgres=warn,postgres=warn".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        shutdown_telemetry();
    }
}
