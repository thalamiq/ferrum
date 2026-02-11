# FHIR Search Tests

These are integration tests for the server’s FHIR search implementation. They are organized by parameter type and spec area under `server/tests/search/`.

## Key Rule

Search tests must **not** insert into `search_*` tables.

Tests run with an **inline job queue**, so indexing happens immediately when resources are created/updated via HTTP. This makes tests realistic and deterministic.

## Adding A Search Test

1. Register the search parameter (when packages are disabled and the server won’t load it for you):
   - `register_search_parameter(&app.state.db_pool, ...)`
2. Create resources via the API (`app.request(...)`).
3. Perform the search via the API and assert the Bundle.

## Running

```bash
cd server

# All integration tests
cargo test

# Search-only
cargo test --test search_tests
```

