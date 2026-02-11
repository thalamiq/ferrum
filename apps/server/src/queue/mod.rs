//! Job queue abstraction for background processing
//!
//! Provides a trait-based interface for job queues with PostgreSQL implementation.
//! Uses LISTEN/NOTIFY for instant job pickup with minimal overhead.

mod helpers;
mod inline;
mod models;
mod postgres;
mod traits;

pub use helpers::*;
pub use inline::InlineJobQueue;
pub use models::*;
pub use postgres::PostgresJobQueue;
pub use traits::JobQueue;
