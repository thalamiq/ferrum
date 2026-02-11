//! Database layer - repositories and data access

pub mod admin;
pub mod indexing;
pub mod metadata;
pub mod metrics;
pub mod packages;
pub mod resolver;
pub mod runtime_config;
pub mod search;
pub mod store;
pub mod terminology;
pub mod traits;
pub mod transaction;

pub use indexing::IndexingRepository;
pub use metadata::MetadataRepository;
pub use metrics::MetricsRepository;
pub use resolver::{FhirResourceResolver, ResolutionContext};
pub use runtime_config::RuntimeConfigRepository;
pub use search::engine::{SearchEngine, SearchParameters};
pub use store::PostgresResourceStore;
pub use terminology::TerminologyRepository;
pub use traits::{ResourceStore, ResourceTransaction, TransactionContext};
pub use transaction::PostgresTransactionContext;
