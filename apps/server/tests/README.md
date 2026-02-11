# FHIR Server Test Suite

This directory contains comprehensive integration tests for the FHIR server, organized by FHIR specification sections.

## Quick Start

```bash
cd server
cargo test
```

Override parallelism:

```bash
cd server
RUST_TEST_THREADS=4 cargo test
# or:
cargo test -- --test-threads=4
```

## Test Organization

Tests are organized following the FHIR R4 specification structure:

```
tests/
├── support/              # Test infrastructure and helpers
│   ├── mod.rs           # TestApp setup, with_test_app helper
│   ├── builders.rs      # Fluent resource builders (PatientBuilder, etc.)
│   ├── assertions.rs    # Custom assertions for FHIR resources
│   └── fixtures.rs      # Common test data and constants
│
├── crud/                # CRUD operations (RESTful API)
│   ├── create.rs        # POST /{resourceType}
│   ├── read.rs          # GET /{resourceType}/{id}
│   ├── update.rs        # PUT /{resourceType}/{id}
│   └── delete.rs        # DELETE /{resourceType}/{id}
│
├── search/              # Search operations
│   ├── parameters/      # Search parameter types
│   │   ├── token.rs     # Token search (identifier, code, etc.)
│   │   ├── string.rs    # String search (name, address, etc.)
│   │   ├── reference.rs # Reference search (subject, patient, etc.)
│   │   ├── number.rs    # Number search
│   │   ├── date.rs      # Date/DateTime search
│   │   ├── quantity.rs  # Quantity search
│   │   └── uri.rs       # URI search
│   ├── modifiers/       # Search modifiers
│   │   ├── missing.rs   # :missing modifier
│   │   ├── exact.rs     # :exact modifier
│   │   ├── contains.rs  # :contains modifier
│   │   ├── text.rs      # :text modifier
│   │   ├── not.rs       # :not modifier
│   │   └── ...
│   ├── result_params/   # Result parameters
│   │   ├── sort.rs      # _sort
│   │   ├── count.rs     # _count
│   │   ├── include.rs   # _include / _revinclude
│   │   └── ...
│   └── ...
│
├── batch_transaction/   # Batch and transaction operations
├── history/            # History operations
├── operations/         # FHIR operations ($validate, etc.)
├── terminology/        # Terminology operations
└── conformance/        # Metadata and conformance

```

## Test Infrastructure

### TestApp

The `TestApp` struct provides:

- Isolated PostgreSQL schema per test (automatic cleanup)
- Configured with workers disabled and an inline job queue (deterministic indexing)
- Fast test execution with minimal pool size
- Automatic tracing setup

### Isolation Model

- Each test gets its own PostgreSQL schema (`test_{uuid}`).
- The DB connection uses `search_path` so unqualified table names hit the test schema.
- Cleanup always drops the schema (even if the test panics).

### Deterministic Indexing

Tests run with `JobQueueKind::Inline`, so indexing jobs execute immediately. Search tests should:

1. register a search parameter when needed (`register_search_parameter`)
2. create/update resources via HTTP
3. query via HTTP and assert results

Do not insert into `search_*` tables in tests.

### Helper Functions

**`with_test_app(f)`** - Runs a test with automatic setup/cleanup:

```rust
#[tokio::test]
async fn my_test() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Test code here
            Ok(())
        })
    }).await
}
```

**`with_test_app_with_config(configure, f)`** - Runs a test with a per-test config override:

```rust
#[tokio::test]
async fn hard_delete_behavior() -> anyhow::Result<()> {
    with_test_app_with_config(
        |cfg| cfg.fhir.hard_delete = true,
        |app| Box::pin(async move {
            // Test code here
            Ok(())
        }),
    )
    .await
}
```

### Resource Builders

Fluent builders for creating test resources:

```rust
use crate::support::PatientBuilder;

let patient = PatientBuilder::new()
    .family("Smith")
    .given("John")
    .gender("male")
    .identifier("http://example.org/mrn", "12345")
    .build();
```

Available builders:

- `PatientBuilder`
- `ObservationBuilder`
- `ConditionBuilder`
- More can be added as needed

### Assertions

Custom assertions for FHIR resources:

```rust
use crate::support::*;

// Assert Bundle structure
assert_bundle(&value)?;
assert_bundle_type(&bundle, "searchset")?;
assert_bundle_total(&bundle, 5)?;

// Assert resource properties
assert_resource_id(&resource, "expected-id")?;
assert_version_id(&resource, "2")?;

// Assert status codes
assert_status(status, StatusCode::OK, "read operation");
assert_success(status, "create");
assert_client_error(status, "invalid request");

// Extract data from bundles
let ids = extract_resource_ids(&bundle, "Patient")?;
let included_ids = extract_resource_ids_by_mode(&bundle, "Patient", "include")?;
```

### Fixtures

Common test data and constants:

```rust
use crate::support::*;

// Predefined resources
let patient = minimal_patient();
let patient = example_patient("Doe", "John");
let patient = patient_with_mrn("Smith", "12345");

// Constants
use crate::support::fixtures::constants::*;
MRN_SYSTEM, SNOMED_SYSTEM, LOINC_SYSTEM, etc.
```

## Writing Tests

### Test Structure

Each test should:

1. Use `with_test_app` for automatic setup/cleanup
2. Follow the Arrange-Act-Assert pattern
3. Test one specific behavior
4. Include descriptive test names
5. Reference FHIR spec sections in comments

### Basic Pattern

```rust
use crate::support::*;
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn create_patient() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            let patient = json!({ "resourceType": "Patient", "active": true });
            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");
            Ok(())
        })
    })
    .await
}
```

### Search Pattern

```rust
#[tokio::test]
async fn search_by_identifier() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            register_search_parameter(
                &app.state.db_pool,
                "identifier",
                "Patient",
                "token",
                "Patient.identifier",
                &[],
            )
            .await?;

            let patient = patient_with_mrn("Smith", "12345");
            let (status, _headers, _body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;
            assert_status(status, StatusCode::CREATED, "create");

            // Indexing happens automatically (inline job queue).
            let (status, _headers, body) = app
                .request(Method::GET, "/fhir/Patient?identifier=12345", None)
                .await?;
            assert_status(status, StatusCode::OK, "search");

            let bundle: serde_json::Value = serde_json::from_slice(&body)?;
            let ids = extract_resource_ids(&bundle, "Patient")?;
            assert_eq!(ids.len(), 1);
            Ok(())
        })
    })
    .await
}
```

### Example Test

```rust
/// Test CREATE operation assigns server-generated UUID
/// FHIR Spec: https://hl7.org/fhir/R4/http.html#create
#[tokio::test]
async fn create_assigns_server_id() -> anyhow::Result<()> {
    with_test_app(|app| {
        Box::pin(async move {
            // Arrange
            let patient = minimal_patient();

            // Act
            let (status, headers, body) = app
                .request(Method::POST, "/fhir/Patient", Some(to_json_body(&patient)?))
                .await?;

            // Assert
            assert_status(status, StatusCode::CREATED, "create");
            let created: serde_json::Value = serde_json::from_slice(&body)?;

            let id = created["id"].as_str().expect("must have id");
            assert!(uuid::Uuid::parse_str(id).is_ok(), "should be UUID");

            Ok(())
        })
    })
    .await
}
```

### Best Practices

1. **Isolation**: Each test should be independent and not rely on other tests
2. **Cleanup**: Use `with_test_app` which automatically cleans up schemas
3. **Clear names**: Test names should describe what they test
4. **Comments**: Reference FHIR spec sections
5. **Assertions**: Use helper assertions for better error messages
6. **Indexing**: Register needed search params and rely on inline indexing (don’t write to `search_*` tables)

## Running Tests

```bash
# Run all tests
cargo test

# Run tests in a specific module
cargo test crud::create

# Run a specific test
cargo test create_assigns_server_id

# Run with output
cargo test -- --nocapture

# Run tests in parallel (default)
cargo test -- --test-threads=4
```

## Configuration

### Test Threads

Default cap is set in `server/.cargo/config.toml` via `RUST_TEST_THREADS`.

### Test Database

`server/tests/support/shared.rs` loads `Config` and, when present, uses `database.test_database_url` as the test database.

Set via config file (`server/config.yaml`) or env override:

- `database.test_database_url` (env: `FHIR__DATABASE__TEST_DATABASE_URL`)

## Troubleshooting

### Tests Hang / Deadlock

- Reduce threads: `RUST_TEST_THREADS=2 cargo test`
- Check Postgres `max_connections`
- Look for long-running locks (schema drop/create can block if connections leak)

### Search Returns 0 Results

- Ensure the test registered the needed search parameter (`register_search_parameter`)
- Ensure the resource actually contains the field referenced by the search parameter expression
- Avoid custom FHIRPath expressions that require unsupported functions (keep expressions simple)

## FHIR Spec Compliance

Each test module includes a header comment linking to the relevant FHIR specification section.

### Key Spec Requirements

**CREATE**:

- Server assigns ID (UUID)
- meta.versionId = "1"
- meta.lastUpdated populated
- Ignores client-provided id/versionId/lastUpdated
- Returns 201 Created with Location header
- Conditional create (If-None-Exist)

**READ**:

- Returns current version only
- 404 Not Found for non-existent
- 410 Gone for deleted
- ETag header with versionId

**UPDATE**:

- Increments versionId
- Updates lastUpdated
- Update-as-create (client IDs)
- Conditional update (If-Match)
- ID in body must match URL

**DELETE**:

- Soft delete (default): creates deleted version
- Hard delete (optional): removes all versions
- Idempotent (deleting twice succeeds)
- Returns 204 No Content
- Deleted resources return 410 Gone

## Test Coverage Status

- ✅ **Core CRUD operations: 100% SPEC COMPLIANT** (60 tests passing)
  - CREATE: 13 tests
  - READ: 6 tests
  - UPDATE: 10 tests
  - DELETE: 7 tests
  - Spec Compliance: 21 tests
  - Configurable Behaviors: 3 tests
  - **See [HTTP_SPEC_COMPLIANCE.md](HTTP_SPEC_COMPLIANCE.md) for detailed report**
- ⏳ Search parameters (all types) - Next priority
- ⏳ Search modifiers
- ⏳ Search result parameters
- ⏳ Conditional operations (documented, not yet implemented)
- ⏳ Batch/Transaction
- ⏳ History
- ⏳ Operations
- ⏳ Terminology
- ⏳ Conformance

## Contributing

When adding tests:

1. Place them in the appropriate module
2. Update this README if adding new modules
3. Use existing helpers and builders
4. Add new helpers/builders if needed
5. Follow existing patterns
6. Reference FHIR spec
