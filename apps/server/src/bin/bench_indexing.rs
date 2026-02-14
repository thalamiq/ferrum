//! Benchmarking tool for indexing performance
//!
//! This tool loads a FHIR package from the registry and benchmarks the indexing performance
//! similar to how the indexing worker processes resources.
//!
//! Usage:
//!   cargo run --bin bench-indexing -- --package-name hl7.fhir.r4.core --package-version 4.0.1 --fhir-version R4 [--database-url <url>] [--include-examples]

use anyhow::{Context, Result};
use chrono::Utc;
use clap::Parser;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use ferrum_registry_client::RegistryClient;
use tracing::{info, warn};

use ferrum::models::Resource;
use ferrum::services::IndexingService;

#[derive(Parser, Debug)]
#[clap(name = "bench-indexing")]
#[clap(about = "Benchmark indexing performance for FHIR packages")]
struct Args {
    /// Package name (e.g., hl7.fhir.r4.core)
    #[clap(short = 'n', long, default_value = "hl7.fhir.r4.core")]
    package_name: String,

    /// Package version (use "latest" for latest version)
    #[clap(short = 'v', long, default_value = "4.0.1")]
    package_version: String,

    /// FHIR version (e.g., R4, R4B, R5)
    #[clap(short = 'f', long, default_value = "R4")]
    fhir_version: String,

    /// Database connection URL (or set DATABASE_URL env var)
    #[clap(short, long)]
    database_url: Option<String>,

    /// Include example resources from package
    #[clap(long)]
    include_examples: bool,

    /// Use bulk indexing (COPY) instead of batch indexing
    #[clap(long)]
    use_bulk: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,bench_indexing=debug".into()),
        )
        .init();

    let args = Args::parse();

    info!("=== FHIR Indexing Benchmark ===");
    info!("Package: {}#{}", args.package_name, args.package_version);
    info!("FHIR version: {}", args.fhir_version);
    info!("Include examples: {}", args.include_examples);
    info!("Use bulk indexing: {}", args.use_bulk);

    // Load package from registry
    let load_start = std::time::Instant::now();
    info!("Loading FHIR package from registry...");
    let registry = Arc::new(RegistryClient::new(None));

    let version = if args.package_version == "latest" {
        None
    } else {
        Some(args.package_version.as_str())
    };

    let packages = registry
        .load_package_with_dependencies(&args.package_name, version)
        .await
        .context("Failed to load package from registry")?;

    if packages.is_empty() {
        anyhow::bail!("No packages loaded from registry");
    }

    // Use the first package (main package, not dependencies)
    let package = packages
        .iter()
        .find(|p| p.manifest.name == args.package_name)
        .or_else(|| packages.first())
        .context("Failed to find main package")?;

    let load_time = load_start.elapsed();
    info!("Loaded package in {:?}", load_time);
    info!(
        "Package: {}#{}",
        package.manifest.name, package.manifest.version
    );
    if packages.len() > 1 {
        info!(
            "Loaded {} total packages (including dependencies)",
            packages.len()
        );
    }

    // Collect resources
    let mut json_resources: Vec<&JsonValue> = package.conformance_resources().iter().collect();
    if args.include_examples {
        json_resources.extend(package.example_resources().iter());
    }

    info!(
        "Found {} conformance resources and {} example resources",
        package.conformance_resources().len(),
        package.example_resources().len()
    );
    info!("Total resources to index: {}", json_resources.len());

    if json_resources.is_empty() {
        warn!("No resources found in package!");
        return Ok(());
    }

    // Convert to Resource models
    let convert_start = std::time::Instant::now();
    let resources: Vec<Resource> = json_resources
        .iter()
        .enumerate()
        .map(|(idx, json)| {
            let resource_type = json
                .get("resourceType")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            let id = json
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("gen-{}", idx));

            Resource {
                id,
                resource_type,
                version_id: 1,
                resource: (*json).clone(),
                last_updated: Utc::now(),
                deleted: false,
            }
        })
        .collect();
    let convert_time = convert_start.elapsed();
    info!(
        "Converted {} resources in {:?}",
        resources.len(),
        convert_time
    );

    // Group by resource type for reporting
    let mut by_type: HashMap<String, usize> = HashMap::new();
    for resource in &resources {
        *by_type.entry(resource.resource_type.clone()).or_insert(0) += 1;
    }
    info!("Resource types: {}", by_type.len());
    let mut type_counts: Vec<_> = by_type.iter().collect();
    type_counts.sort_by_key(|(k, _)| *k);
    for (resource_type, count) in type_counts {
        info!("  {}: {}", resource_type, count);
    }

    // Connect to database
    let database_url = args.database_url.unwrap_or_else(|| {
        std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/fhir".to_string())
    });

    info!("Connecting to database...");
    let pool = PgPool::connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    // Create indexing service
    let indexing_service = IndexingService::new(pool, &args.fhir_version, 50, 200, true, true)
        .context("Failed to create indexing service")?;

    // Run indexing benchmark
    let total_start = std::time::Instant::now();
    info!("Starting indexing benchmark...");

    if args.use_bulk {
        info!("Using COPY-based bulk indexing");
        use ferrum::services::indexing::BulkIndexer;
        let bulk_indexer = BulkIndexer::new(indexing_service.pool().clone());
        bulk_indexer
            .bulk_index_with_copy(&resources, &indexing_service)
            .await
            .with_context(|| format!("Bulk indexing failed for {} resources", resources.len()))?;
    } else {
        info!("Using batch indexing");
        indexing_service
            .index_resources_auto(&resources)
            .await
            .with_context(|| {
                format!(
                    "Batch indexing failed for {} resources. Check logs above for the first error that caused transaction abort.",
                    resources.len()
                )
            })?;
    }

    let total_time = total_start.elapsed();
    let throughput = resources.len() as f64 / total_time.as_secs_f64();

    // Print summary
    println!("\n=== Benchmark Results ===");
    println!("Total resources: {}", resources.len());
    println!("Total time: {:?}", total_time);
    println!("Throughput: {:.2} resources/sec", throughput);
    println!("\nBreakdown by resource type:");
    let mut type_counts: Vec<_> = by_type.iter().collect();
    type_counts.sort_by_key(|(k, _)| *k);
    for (resource_type, count) in type_counts {
        println!("  {}: {} resources", resource_type, count);
    }

    info!("=== Benchmark Complete ===");
    Ok(())
}
