# CLAUDE.md - FHIR Server Documentation for AI Assistants

This document provides architectural context and implementation details for AI assistants working with this FHIR server codebase.

## Project Overview

**Name**: fhir-server (Zunder)
**Language**: Rust (Edition 2024)
**Framework**: Axum (async web framework)
**Database**: PostgreSQL with SQLx
**Standard**: FHIR R4/R5 specification compliance

This is a production-grade FHIR (Fast Healthcare Interoperability Resources) server implementation that provides REST API endpoints for healthcare data interoperability.

## Architecture Overview

### Core components (deployables)

Zunder is easiest to reason about as four parts:

- **API**: Rust HTTP service that serves the FHIR REST surface area (CRUD, Search, Batch/Transaction, $operations) and reads/writes Postgres.
- **Worker**: Rust background processor that performs non-interactive work (indexing, long-running jobs) outside the request path.
- **Admin UI**: Next.js app used by operators to browse resources, monitor jobs, and troubleshoot server behavior.
- **DB**: PostgreSQL as the system of record (resources + history), plus search index tables and queue/ops metadata.

Related doc: `docs/concepts/architecture.mdx`

### High-Level Structure

```
src/
├── api/          # HTTP handlers and routing
├── db/           # Database layer (store, search, transactions)
├── services/     # Business logic layer
├── models/       # Domain models
├── hooks/        # Event hooks (search params, conformance)
├── queue/        # Job queue (async/background processing)
├── workers/      # Background worker processes
├── config.rs     # Configuration management
├── state.rs      # Application state (dependency injection)
├── startup.rs    # Server initialization
└── error.rs      # Error types
```

### Key Binaries

1. **fhir-server** (`src/main.rs`) - Main HTTP API server
2. **fhir-worker** (`src/worker.rs`) - Background job processor
3. **bench-indexing** - Performance benchmarking tool

## Core Components

### 1. Application State (`src/state.rs`)

**Purpose**: Centralized dependency injection container shared across all request handlers.

**Key Fields**:

```rust
pub struct AppState {
    pub config: Arc<Config>,
    pub db_pool: PgPool,
    pub job_queue: Arc<dyn JobQueue>,
    pub search_engine: Arc<SearchEngine>,  // ⭐ Shared across all operations
    pub crud_service: Arc<CrudService>,
    pub batch_service: Arc<BatchService>,
    pub transaction_service: Arc<TransactionService>,
    pub search_service: Arc<SearchService>,
    // ... other services
}
```

**Important Design Decision**:

- The `SearchEngine` is created ONCE and shared via `Arc` across all services
- This provides cache sharing for search parameter definitions
- Reduces database queries and memory usage
- See "Search Engine Architecture" section below

### 2. Service Layer (`src/services/`)

Services implement FHIR business logic and coordinate between the database layer and HTTP handlers.

**Key Services**:

- `CrudService` - Create, Read, Update, Delete operations
- `BatchService` - FHIR batch bundle processing (independent operations)
- `TransactionService` - FHIR transaction bundles (atomic, interdependent)
- `SearchService` - Resource searching with summary filtering
- `IndexingService` - Search index management
- `HistoryService` - Resource version history
- `MetadataService` - CapabilityStatement generation
- `PackageService` - FHIR package management
- `OperationExecutor` - FHIR $operations

**Service Dependencies**:

- Services receive dependencies via constructor injection
- Most services need: `store`, `hooks`, `job_queue`
- Batch/Transaction services also need `search_engine` for conditional operations

**IMPORTANT: Clean Architecture Boundary**

**RULE: Services NEVER have direct database access (PgPool). Only repositories do.**

This project follows a strict separation between business logic (services) and data access (repositories):

```rust
// ✓ CORRECT - Service uses repository
pub struct TerminologyService {
    repo: TerminologyRepository,  // Repository handles all SQL
}

impl TerminologyService {
    pub async fn expand(&self, ...) -> Result<JsonValue> {
        // Business logic here
        let cached = self.repo.fetch_cached_expansion(...).await?;  // ✓ Repository call
        // More business logic
    }
}

// ✗ WRONG - Service with direct database access
pub struct BadService {
    pool: PgPool,  // ✗ Services should NEVER have PgPool
}

impl BadService {
    pub async fn some_method(&self) -> Result<()> {
        sqlx::query("SELECT ...").fetch_all(&self.pool).await?;  // ✗ SQL in service
    }
}
```

**Why This Matters**:

1. **Clear Responsibilities**: Repositories handle SQL/data mapping, services handle business rules
2. **Testability**: Mock repositories easily without database
3. **Maintainability**: Change database queries without touching business logic
4. **Discoverability**: "Where's the SQL?" → Always in `src/db/`. "Where's the logic?" → Always in `src/services/`

**Repository Pattern**:

All database access goes through repositories in `src/db/`:

- `PostgresResourceStore` - FHIR resource CRUD (implements `ResourceStore` trait)
- `TerminologyRepository` - Terminology operations (CodeSystem, ValueSet, expansions, closure tables)
- `AdminRepository` - Statistics, diagnostics, search parameter management
- `MetadataRepository` - CapabilityStatement data (search parameters, resource types)
- `MetricsRepository` - Application metrics and monitoring
- `PackageRepository` - FHIR package management
- `SearchEngine` - Search parameter resolution and query building
- `IndexingRepository` - Search index management (locking, status, deletion)
- `FhirResourceResolver` - Reference resolution for FHIRPath (moved from services/)

Services receive repositories via dependency injection:

```rust
// In src/state.rs
let terminology_repo = TerminologyRepository::new(db_pool.clone());
let terminology_service = Arc::new(TerminologyService::new(terminology_repo));

let metadata_repo = MetadataRepository::new(db_pool.clone());
let metadata_service = Arc::new(MetadataService::new(config_arc, metadata_repo));

let metrics_repo = MetricsRepository::new(db_pool.clone());
let metrics_service = Arc::new(MetricsService::new(metrics_repo));
```

**Infrastructure Services**:

`IndexingService` is a hybrid infrastructure service that combines:
- **Business Logic**: FHIRPath extraction, value normalization (stays in service)
- **Data Access**: Uses `IndexingRepository` for SQL operations

This service still retains `PgPool` due to the complexity of bulk insert operations tightly coupled with extraction logic. However, it uses `IndexingRepository` for discrete operations (locking, status updates, deletions). This is documented as infrastructure rather than pure business logic.

### 3. Database Layer (`src/db/`)

**Structure**:

```
db/
├── mod.rs              # PostgresResourceStore (main CRUD)
├── transaction.rs      # Transaction context for atomicity
├── admin/              # Administrative operations
├── packages/           # Package repository
└── search/             # Search implementation
    ├── engine/         # Search execution engine
    │   ├── api.rs      # Public search API
    │   ├── execute.rs  # SQL execution
    │   ├── resolve.rs  # Parameter resolution
    │   ├── filter.rs   # _filter implementation
    │   └── includes.rs # _include/_revinclude
    ├── params.rs       # Search parameter parsing
    ├── query_builder.rs# SQL query construction
    └── parameter_lookup.rs  # Cached parameter definitions
```

**Key Pattern**: The database layer uses the repository pattern. `PostgresResourceStore` is the primary data access object.

### 4. Search Engine Architecture ⭐

**Recent Refactoring (2025-01-02)**: Migrated from per-request SearchEngine instantiation to a single shared instance.

**Location**: `src/db/search/engine/`

**Design**:

```rust
pub struct SearchEngine {
    db_pool: PgPool,
    param_cache: Arc<SearchParamCache>,  // Cached search parameter definitions
    computed_hooks: HookRegistry,
    enable_text_search: bool,
    enable_content_search: bool,
}
```

**SearchParamCache**:

- Thread-safe cache (`RwLock<HashMap>`) for search parameter metadata
- Populated lazily from the `search_parameters` table
- Key: `(resource_type, parameter_code)`
- Value: Parameter type, expression, modifiers, comparators, etc.

**Why Shared**:

- Cache is expensive to populate (DB queries)
- Same parameters queried repeatedly across requests
- Thread-safe design allows concurrent access
- Reduces memory footprint

**Usage Pattern**:

```rust
// Create once in AppState::new
let search_engine = Arc::new(SearchEngine::new(
    db_pool.clone(),
    config.fhir.search.enable_text,
    config.fhir.search.enable_content,
));

// Pass to services
let batch_service = BatchService::new(
    store,
    hooks,
    job_queue,
    search_engine.clone(),  // ⭐ Share the instance
    allow_update_create,
    hard_delete,
);

// Use in handlers
state.search_engine.search(Some(&resource_type), &params, base_url).await?;
```

**Search Index Tables**:

- `search_string` - String parameters (e.g., name, identifier)
- `search_token` - Token parameters (e.g., code, system|code)
- `search_reference` - Reference parameters (e.g., patient, subject)
- `search_date` - Date/DateTime parameters
- `search_number` - Numeric parameters
- `search_quantity` - Quantity with units
- `search_uri` - URI parameters
- `search_text` - Full-text search (PostgreSQL tsvector)
- `search_content` - Narrative content search

### 5. Transaction Processing

**Batch vs Transaction**:

- **Batch** (`BatchService`): Entries processed independently, no interdependencies
- **Transaction** (`TransactionService`): Atomic all-or-nothing, supports interdependencies

**Transaction Features**:

- Full URL rewriting (urn:uuid: → Patient/123)
- Reference resolution across entries
- Conditional operations (If-None-Exist, If-Match, If-None-Match)
- Proper ordering: DELETE → POST → PUT/PATCH → GET
- Rollback on any error

**Database Transaction**:

```rust
// TransactionService uses PostgresTransactionContext
let tx = self.store.begin_transaction().await?;
// ... operations ...
tx.commit().await?;  // or rollback on error
```

### 6. Conditional Operations

**Types**:

1. **Conditional Create** (If-None-Exist): Search for existing, return if found, create if not
2. **Conditional Update** (PUT with search params): Update if single match, else create
3. **Conditional Patch/Delete**: Operate on search result (must be single match)

**Implementation**:

- Builds search parameters from query string
- Uses `SearchEngine` to find matches
- Validates match count (0, 1, or multiple)
- Returns appropriate HTTP status/error

**Key Files**:

- `src/services/conditional.rs` - Shared conditional logic
- Used by: `batch.rs`, `transaction.rs`, `crud.rs` handlers

### 7. Indexing System

**Purpose**: Populate search index tables for efficient FHIR parameter searching.

**Flow**:

1. Resource created/updated → CRUD service queues indexing job
2. Background worker picks up job (or inline in tests)
3. `IndexingService::index_resources()` extracts search values via FHIRPath
4. Inserts into appropriate `search_*` tables

**Job Queue**:

- **Production**: `PostgresJobQueue` - persists jobs, processed by workers
- **Tests**: `InlineJobQueue` - executes immediately in-process

**Configuration**:

```rust
pub enum JobQueueKind {
    Postgres,  // Background workers
    Inline,    // Immediate execution (tests)
}
```

### 8. Hook System (`src/hooks/`)

**Purpose**: React to resource lifecycle events.

**Hook Types**:

- `ResourceHook` - on_created, on_updated, on_deleted
- `SearchParameterHook` - Rebuilds index when SearchParameter resources change
- `ComputedHook` - Calculate derived values (e.g., \_text, \_content)

**Registration**:

```rust
let resource_hooks: Vec<Arc<dyn ResourceHook>> = vec![
    Arc::new(SearchParameterHook::new(db_pool, indexing_service)),
];
```

## Database Schema

**Key Tables**:

- `resources` - Main resource storage (id, resource_type, resource JSONB, version_id, deleted)
- `resource_history` - All versions of all resources
- `search_parameters` - FHIR SearchParameter definitions
- `search_*` tables - Indexed search values (one per parameter type)
- `compartment_memberships` - Patient/Encounter compartment access control
- `jobs` - Background job queue
- `packages` - FHIR package metadata
- `package_resources` - Resources from installed packages

**Important**: Resources are stored as JSONB in PostgreSQL. Search parameters are extracted and indexed separately.

## API Layer (`src/api/`)

**Structure**:

```
api/
├── handlers/
│   ├── crud.rs       # Resource CRUD endpoints
│   ├── batch.rs      # Batch/Transaction processing
│   ├── search.rs     # Search endpoints
│   ├── metadata.rs   # /metadata (CapabilityStatement)
│   └── operations.rs # FHIR $operations
├── headers.rs        # HTTP header parsing (Prefer, If-Match, etc.)
├── content_negotiation.rs  # Accept/Content-Type handling
└── resource_formatter.rs   # Resource serialization
```

**Content Types Supported**:

- `application/fhir+json`
- `application/json`
- `application/fhir+xml` (if enabled)

**Important Headers**:

- `Prefer: return=representation|minimal` - Control response body
- `If-Match: W/"3"` - Conditional update (version check)
- `If-None-Match: *` - Conditional create (must not exist)
- `If-Modified-Since` - Conditional read

## Configuration (`src/config.rs`)

**Sources** (priority order):

1. Environment variables (highest priority)
2. Config file (config.yaml, config.yml, or config.json)
3. Defaults (lowest priority)

**Example Configuration** (see `config.example.yaml` for full documentation):

```yaml
server:
  host: "0.0.0.0"
  port: 8080

database:
  url: "postgresql://..."
  pool_min_size: 4
  pool_max_size: 40
  statement_timeout_seconds: 300

fhir:
  version: "R4"
  allow_update_create: true
  hard_delete: false

  search:
    enable_text: true
    enable_content: true
    default_count: 20
    max_count: 1000
```

**Environment Variable Override**:

Environment variables use `FHIR__` prefix + double underscore (`__`) to map to nested config structure:
- `FHIR__DATABASE__URL=postgresql://...`
- `FHIR__SERVER__PORT=9090`
- `FHIR__LOGGING__LEVEL=debug`

## Testing

**Structure**:

- Unit tests: Inline with modules (`#[cfg(test)]`)
- Integration tests: `tests/` directory
- Test fixtures: `tests/fixtures/`

**Test Database**:

```rust
// Tests use AppStateOptions for fast setup
AppState::new_with_options(config, AppStateOptions {
    run_migrations: true,
    install_packages: false,  // Skip in tests
    load_operation_definitions: false,
    job_queue: JobQueueKind::Inline,  // Immediate execution
}).await
```

**Running Tests**:

```bash
cargo test --lib           # Unit tests only
cargo test --test crud     # Specific integration test
cargo test                 # All tests
```

## Common Patterns

### 1. Error Handling

```rust
pub enum Error {
    ResourceNotFound { resource_type: String, id: String },
    VersionConflict { expected: i32, actual: i32 },
    Validation(String),
    Database(sqlx::Error),
    // ... etc
}

pub type Result<T> = std::result::Result<T, Error>;
```

All functions return `Result<T>` and use `?` for propagation.

### 2. Resource Storage

```rust
pub struct Resource {
    pub id: String,
    pub resource_type: String,
    pub resource: JsonValue,  // Full FHIR JSON
    pub version_id: i32,
    pub last_updated: DateTime<Utc>,
    pub deleted: bool,
}
```

### 3. Service Construction

Services use the builder pattern:

```rust
CrudService::new(store)
CrudService::with_hooks(store, hooks)
CrudService::with_hooks_and_indexing(store, hooks, queue, indexing, ...)
```

### 4. Async Patterns

- All I/O is async (Tokio runtime)
- Database queries use `sqlx::query!()` macro for compile-time SQL checking
- Handlers are async functions: `async fn handler(State(state): State<AppState>) -> Result<Response>`

## Development Guidelines

### Adding a New Service

1. Create in `src/services/new_service.rs`
2. Add struct with dependencies
3. Implement methods
4. Add to `AppState` in `src/state.rs`
5. Export from `src/services/mod.rs`

### Adding a New Search Parameter Type

1. Define table in `sql/schema.sql`
2. Add migration in `migrations/`
3. Update `SearchParamType` enum in `src/db/search/parameter_lookup.rs`
4. Implement indexing in `src/services/indexing/mod.rs`
5. Add query building in `src/db/search/query_builder.rs`

### Adding a New FHIR Operation

1. Create operation handler in `src/services/operations/`
2. Register in `OperationRegistry`
3. Add route in `src/api/handlers/operations.rs`

## Performance Considerations

### Database

- Use connection pooling (configured in `database.pool_max_size`)
- Search queries use indexes on `search_*` tables
- Avoid N+1 queries (use batch loading where possible)
- Statement timeout prevents runaway queries

### Caching

- `SearchParamCache` caches parameter definitions
- FHIR context caches structure definitions
- Consider adding resource-level caching for read-heavy workloads

### Indexing

- Background job queue prevents blocking request handlers
- Batch indexing reduces database round-trips
- Configurable batch size: `database.indexing_batch_size`

## Troubleshooting

### Common Issues

1. **SearchEngine not found**: Ensure you're using `state.search_engine`, not creating new instances
2. **Transaction deadlocks**: Check operation ordering in transactions
3. **Missing search results**: Verify indexing job ran successfully
4. **Slow searches**: Check `search_*` table indexes, consider EXPLAIN ANALYZE

### Debugging

```rust
// Enable query logging
RUST_LOG=sqlx=debug,fhir_server=debug cargo run

// Profile slow queries
EXPLAIN ANALYZE SELECT ... ;
```

### Migration Issues

```bash
# Revert migration
sqlx migrate revert

# Check current version
sqlx migrate info
```

## Recent Changes

### 2025-01-02: SearchEngine Refactoring

- **Changed**: SearchEngine instantiation from per-request to shared singleton
- **Location**: `src/state.rs`, `src/services/batch.rs`, `src/services/transaction.rs`, `src/api/handlers/crud.rs`
- **Benefit**: Shared parameter cache, reduced memory, fewer DB queries
- **Breaking**: Services now require `Arc<SearchEngine>` in constructor

## External Dependencies

**Key Crates**:

- `axum` - Web framework
- `sqlx` - Async SQL with compile-time checking
- `tokio` - Async runtime
- `serde_json` - JSON serialization
- `fhir-models` - FHIR resource type definitions
- `fhir-context` - FHIR metadata and validation
- `fhirpath` - FHIRPath expression evaluation

## Resources

- FHIR Specification: https://hl7.org/fhir/
- Project Tests: `tests/README.md`
- Benchmarking: `BENCHMARKING.md`

---

**Last Updated**: 2025-01-02
**Maintained for**: Claude and other AI assistants working with this codebase
