# FHIR Package Registry Client

A Rust client for loading and caching FHIR packages from local cache and the Simplifier package registry.

## Features

- **Trait-Based Cache Architecture**: Implement custom cache backends (file system, database, Redis, etc.)
- **File System Cache**: Default implementation using the standard `.fhir/packages` directory
- **Simplifier Registry Integration**: Search, download, and cache packages from packages.simplifier.net
- **Version Resolution**: Automatic version resolution following FHIR package specification
- **Dependency Management**: Load packages with all transitive dependencies

## Usage

### Load from Cache

```rust
use zunder_registry_client::RegistryClient;

let client = RegistryClient::new(None);
let package = client.load_package("hl7.fhir.r5.core", "5.0.0")?;
```

### Search Simplifier Registry

```rust
use zunder_registry_client::{RegistryClient, SimplifierSearchParams};

let client = RegistryClient::new(None);
let params = SimplifierSearchParams {
    name: Some("hl7.fhir".to_string()),
    fhir_version: Some("4.0.1".to_string()),
    ..Default::default()
};

let results = client.search_packages(&params)?;
for result in results {
    println!("{} v{}", result.name, result.version);
}
```

### Download from Simplifier

```rust
use zunder_registry_client::RegistryClient;

let client = RegistryClient::new(None);

// Download and cache a package
let package = client.download_package("hl7.fhir.r4.core", "4.0.1")?;

// Or load from cache if available, download if not
let package = client.load_or_download_package("hl7.fhir.r4.core", "4.0.1")?;
```

### Get Package Versions

```rust
use zunder_registry_client::RegistryClient;

let client = RegistryClient::new(None);
let versions = client.get_package_versions("hl7.fhir.r4.core")?;

for version in versions {
    println!("Available: {}", version);
}
```

## Architecture

The `registry-client` crate provides a flexible, extensible system for loading and caching FHIR packages. It supports multiple cache backends through a trait-based architecture and integrates with the Simplifier package registry.

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    RegistryClient<C>                        │
│  (Generic over PackageCache trait)                          │
│                                                              │
│  ┌────────────────┐              ┌──────────────────────┐  │
│  │ Cache          │              │ Simplifier Client    │  │
│  │ (via trait C)  │              │ (Optional)           │  │
│  └────────────────┘              └──────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
            │                                   │
            │                                   │
            ▼                                   ▼
┌──────────────────────┐          ┌──────────────────────────┐
│  PackageCache Trait  │          │  Simplifier Registry API │
└──────────────────────┘          └──────────────────────────┘
            │
            │ Implementations
            ▼
┌──────────────────────┐          ┌──────────────────────────┐
│  FileSystemCache     │          │   Custom Implementations │
│  (Default)           │          │   - InMemoryCache        │
└──────────────────────┘          │   - DatabaseCache        │
                                  │   - RedisCache           │
                                  │   - S3Cache              │
                                  │   - HybridCache          │
                                  └──────────────────────────┘
```

### Components

#### PackageCache Trait

The core abstraction that allows different cache backends:

```rust
pub trait PackageCache: Send + Sync {
    fn has_package(&self, name: &str, version: &str) -> bool;
    fn get_package(&self, name: &str, version: &str) -> Result<CachedPackage>;
    fn store_package(&self, package: &FhirPackage) -> Result<()>;
    fn list_packages(&self) -> Vec<(String, String)>;
}
```

**Design Decisions:**

- `Send + Sync` bounds allow thread-safe usage
- Simple interface with 4 core operations
- Returns `Result` for error handling
- Uses standard types (String, Vec) for broad compatibility

#### FileSystemCache

Default implementation that stores packages in `~/.fhir/packages`:

```rust
pub struct FileSystemCache {
    cache_root: PathBuf,
}
```

**Features:**

- Follows FHIR package specification directory structure
- Stores packages as `{name}#{version}/package/`
- Thread-safe through file system operations
- Zero dependencies beyond standard library

#### RegistryClient<C>

Generic client that works with any cache implementation:

```rust
pub struct RegistryClient<C: PackageCache> {
    cache: Arc<C>,
    simplifier: Option<SimplifierClient>,
}
```

**Design Decisions:**

- Generic over `C: PackageCache` for flexibility
- Uses `Arc<C>` for cheap cloning and shared ownership
- Optional Simplifier client for remote package access
- Provides convenience constructors for common use cases

#### SimplifierClient

Handles interaction with the Simplifier package registry:

```rust
pub struct SimplifierClient {
    client: reqwest::Client,
    base_url: String,
}
```

**Features:**

- Search packages by name, canonical URL, FHIR version
- Get available versions for a package
- Download packages as tarballs
- Configurable base URL for testing/alternate registries

### Usage Patterns

#### Pattern 1: Default File System Cache

```rust
let client = RegistryClient::new(None);
let package = client.load_package("hl7.fhir.r4.core", "4.0.1")?;
```

#### Pattern 2: Custom Cache Directory

```rust
let client = RegistryClient::new(Some(PathBuf::from("/custom/path")));
```

#### Pattern 3: Custom Cache Implementation

```rust
let cache = MyDatabaseCache::new(connection_string);
let client = RegistryClient::with_cache(cache);
```

#### Pattern 4: Cache-Only Mode (No Remote Access)

```rust
let client = RegistryClient::cache_only(None);
// Only loads from local cache, never hits network
```

#### Pattern 5: Load or Download

```rust
// Try cache first, download if not found
let package = client.load_or_download_package("hl7.fhir.r4.core", "4.0.1")?;
```

### Thread Safety

All components are designed for concurrent use:

- `PackageCache` trait requires `Send + Sync`
- `RegistryClient` wraps cache in `Arc<C>`
- File system operations are naturally thread-safe
- Custom implementations should use appropriate synchronization

### Error Handling

Errors follow Rust best practices:

```rust
pub enum Error {
    PackageNotFound { name: String, version: String },
    Http(reqwest::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
    // ... other variants
}
```

All operations return `Result<T, Error>` for explicit error handling.

### Performance Considerations

1. **Indexing**: `CachedPackage` pre-builds indices for fast lookups
2. **Lazy Loading**: Packages loaded only when requested
3. **Arc Sharing**: Cheap clones via `Arc<C>`
4. **Async Future**: Currently blocking, but trait could support async

## Simplifier API

The Simplifier integration supports the following API endpoints:

### Search

`https://packages.simplifier.net/catalog?<parameters>`

Parameters:

- `name` - Package name (can match any part)
- `canonical` - Canonical URL for a resource (exact match)
- `fhirversion` - FHIR version filter
- `prerelease` - Include unreleased packages (boolean)

### Versions

`https://packages.simplifier.net/<package-name>`

Returns a list of all available versions for a package.

### Download

`https://packages.simplifier.net/<package-name>/<version>`

Downloads a package as a tarball.

## Examples

Run the Simplifier demo:

```bash
cargo run --example simplifier_demo
```

## Cache-Only Mode

If you want to disable remote registry access:

```rust
use zunder_registry_client::RegistryClient;

let client = RegistryClient::cache_only(None);
// This client will only load from local cache
```

## Custom Cache Implementations

The `PackageCache` trait allows you to implement custom cache backends:

```rust
use fhir_package::FhirPackage;
use zunder_registry_client::{PackageCache, RegistryClient, Result};
use std::collections::HashMap;
use std::sync::RwLock;

struct InMemoryCache {
    packages: RwLock<HashMap<String, FhirPackage>>,
}

impl PackageCache for InMemoryCache {
    fn has_package(&self, name: &str, version: &str) -> bool {
        // Implementation
    }

    fn get_package(&self, name: &str, version: &str) -> Result<FhirPackage> {
        // Implementation
    }

    fn store_package(&self, package: &FhirPackage) -> Result<()> {
        // Implementation
    }

    fn list_packages(&self) -> Vec<(String, String)> {
        // Implementation
    }
}

// Use your custom cache
let cache = InMemoryCache::new();
let client = RegistryClient::with_cache(cache);
```

See `examples/custom_cache.rs` for a complete implementation.

### Custom Cache Use Cases

#### Database Cache

Store packages in PostgreSQL, MySQL, SQLite:

- Store packages as BLOBs or JSON
- Index by name and version for fast lookups
- Use transactions for consistency

#### Redis Cache

Distributed caching:

- Store packages as compressed JSON
- Use Redis keys: `fhir:package:{name}#{version}`
- Set expiration policies as needed

#### S3 Cache

Cloud-based package storage:

- Store tarballs in S3 buckets
- Use object metadata for package info
- Integrate with CDN for global distribution

#### Hybrid Cache

Multi-tier strategy:

- Check Redis first (fast)
- Fall back to file system (reliable)
- Background sync between layers

#### Read-Only Cache

Immutable package sets:

- Wrap existing package directories
- Prevent modifications
- Useful for reproducible builds

#### Monitoring Cache

Track cache hits/misses and package usage:

- Wrap existing cache implementation
- Add metrics collection
- Log package access patterns

### Custom Registry Implementations

Future registries can follow the `SimplifierClient` pattern:

1. Create a new module (e.g., `npm_registry.rs`)
2. Implement registry-specific API calls
3. Return `FhirPackage` instances
4. Optionally add to `RegistryClient`

The architecture is designed to support additional registries in the future. Each registry will have its own client module similar to `simplifier.rs`.

## Future Enhancements

1. **Async Support**: Make trait async-compatible
2. **Batch Operations**: Download multiple packages
3. **Delta Updates**: Incremental package updates
4. **Compression**: On-disk compression for file system cache
5. **Metrics**: Track cache hits/misses, download times
6. **Eviction**: LRU or TTL-based cache eviction policies
