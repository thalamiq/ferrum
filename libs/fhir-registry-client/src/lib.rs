//! FHIR Package Registry Client
//!
//! This crate provides async-first functionality to load and cache FHIR packages from
//! local cache and the Simplifier registry.
//!
//! # Examples
//!
//! ## Load from cache (async)
//!
//! ```rust,no_run
//! use ferrum_registry_client::RegistryClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = RegistryClient::new(None);
//! let package = client.load_or_download_package("hl7.fhir.r5.core", "5.0.0").await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Download from Simplifier (async)
//!
//! ```rust,no_run
//! use ferrum_registry_client::RegistryClient;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let client = RegistryClient::new(None);
//! let package = client.load_or_download_package("hl7.fhir.r4.core", "4.0.1").await?;
//! # Ok(())
//! # }
//! ```
//!
pub mod async_client;
pub mod async_simplifier;
pub mod cache;
pub mod error;
pub mod models;
pub mod version_resolver;

// Re-export main async types (default)
pub use async_client::RegistryClient;
pub use async_simplifier::SimplifierClient;
pub use cache::{FileSystemCache, PackageCache};
pub use error::{Error, Result};
pub use models::{SimplifierSearchParams, SimplifierSearchResult};
pub use version_resolver::select_version;

// Re-export fhir_package types for convenience
pub use ferrum_package::FhirPackage;
