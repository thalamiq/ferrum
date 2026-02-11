# CRUD Tests

This directory contains comprehensive tests for FHIR RESTful CRUD operations.

## Test Files

- **`create.rs`** - CREATE operation tests (POST /{resourceType})
- **`read.rs`** - READ operation tests (GET /{resourceType}/{id})
- **`update.rs`** - UPDATE operation tests (PUT /{resourceType}/{id})
- **`delete.rs`** - DELETE operation tests (DELETE /{resourceType}/{id})
- **`spec_compliance.rs`** - HTTP spec compliance verification
- **`configurable_behaviors.rs`** - Tests for configurable server options

## Total: 60 Tests ✅

## Quick Reference

### CREATE (13 tests)
- Server-assigned IDs (UUID)
- Version ID = 1
- Metadata population
- Location header
- Error handling

### READ (6 tests)
- Current version retrieval
- 404 Not Found
- 410 Gone for deleted
- ETag headers

### UPDATE (10 tests)
- Version increments
- Update-as-create
- ID validation
- Metadata updates

### DELETE (7 tests)
- Soft delete (default)
- 410 Gone on read
- Idempotent behavior

### Spec Compliance (21 tests)
- All SHALL requirements
- Header validation
- Status code verification
- ETag management

### Configurable Behaviors (3 tests)
- allow_update_create
- hard_delete
- default_prefer_return

## Running Tests

```bash
# All CRUD tests
cargo test crud

# Specific module
cargo test crud::create
cargo test crud::spec_compliance

# Single test
cargo test create_assigns_server_id
```

## Spec References

Each test references the relevant FHIR specification section.
See `/spec/http/` for local spec files.
