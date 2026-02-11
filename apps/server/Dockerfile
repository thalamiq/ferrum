# syntax=docker/dockerfile:1.7
#
# Multi-stage build for FHIR Server
# Build context: repository root (not apps/server/)

# Build arguments for versioning and reproducibility
ARG RUST_VERSION=1.83
ARG BUILD_DATE
ARG VCS_REF
ARG VERSION=0.1.0

############################
# Builder
############################
FROM rust:${RUST_VERSION}-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# ---- Better layer caching: copy manifests first ----
# Copy workspace Cargo.toml and lockfile
COPY Cargo.toml Cargo.lock ./

# Copy all member Cargo.toml files (for dependency resolution)
COPY apps/server/Cargo.toml apps/server/Cargo.toml
COPY apps/cli/Cargo.toml apps/cli/Cargo.toml
COPY libs/fhir-models/Cargo.toml libs/fhir-models/Cargo.toml
COPY libs/fhirpath/Cargo.toml libs/fhirpath/Cargo.toml
COPY libs/fhir-context/Cargo.toml libs/fhir-context/Cargo.toml
COPY libs/fhir-format/Cargo.toml libs/fhir-format/Cargo.toml
COPY libs/fhir-snapshot/Cargo.toml libs/fhir-snapshot/Cargo.toml
COPY libs/fhir-package/Cargo.toml libs/fhir-package/Cargo.toml
COPY libs/fhir-registry-client/Cargo.toml libs/fhir-registry-client/Cargo.toml
COPY libs/fhir-codegen/Cargo.toml libs/fhir-codegen/Cargo.toml
COPY libs/fhir-validator/Cargo.toml libs/fhir-validator/Cargo.toml
COPY libs/ucum/Cargo.toml libs/ucum/Cargo.toml

# Create dummy src files so cargo fetch can resolve the workspace
RUN mkdir -p apps/server/src apps/cli/src && \
    echo "fn main() {}" > apps/server/src/main.rs && \
    echo "fn main() {}" > apps/cli/src/main.rs && \
    for lib in libs/fhir-models libs/fhirpath libs/fhir-context libs/fhir-format \
               libs/fhir-snapshot libs/fhir-package libs/fhir-registry-client \
               libs/fhir-codegen libs/fhir-validator libs/ucum; do \
        mkdir -p "$lib/src" && echo "" > "$lib/src/lib.rs"; \
    done

# Pre-fetch deps (cache-friendly)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo fetch

# Copy the full source
COPY libs/ libs/
COPY apps/server/ apps/server/

# Build with BuildKit cache mounts
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    bash -c "set -euxo pipefail; \
    echo '=== Starting cargo build for fhir-server and fhir-worker ===' && \
    cargo build --release --locked --bin fhir-server --bin fhir-worker && \
    echo '=== Build completed, verifying binaries ===' && \
    test -f target/release/fhir-server && echo '✓ fhir-server found' || \
    (echo '✗ ERROR: fhir-server binary not found' && exit 1) && \
    test -f target/release/fhir-worker && echo '✓ fhir-worker found' || \
    (echo '✗ ERROR: fhir-worker binary not found' && exit 1) && \
    echo '=== Both binaries verified successfully ==='"

# Strip binaries to reduce size
RUN strip target/release/fhir-server target/release/fhir-worker

############################
# Runtime
############################
FROM debian:bookworm-slim

# Build arguments for labels
ARG BUILD_DATE
ARG VCS_REF
ARG VERSION

# Install runtime deps
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    curl \
    tini \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user and app dirs
RUN useradd -m -u 1001 -s /usr/sbin/nologin fhir && \
    mkdir -p /app/migrations /app/data && \
    chown -R fhir:fhir /app

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /build/target/release/fhir-server /usr/local/bin/fhir-server
COPY --from=builder /build/target/release/fhir-worker /usr/local/bin/fhir-worker

# Copy migrations if they exist
COPY --chown=fhir:fhir apps/server/migrations ./migrations

# Copy internal FHIR packages (loaded at startup when `fhir.install_internal_packages=true`)
COPY --chown=fhir:fhir apps/server/fhir_packages ./fhir_packages

# Copy start script for combined server+worker mode
COPY --chown=root:root apps/server/scripts/start-all.sh /usr/local/bin/start-all
RUN chmod 0755 /usr/local/bin/start-all

# Security hardening
RUN chown root:root /usr/local/bin/fhir-server /usr/local/bin/fhir-worker && \
    chmod 0755 /usr/local/bin/fhir-server /usr/local/bin/fhir-worker

# Add Docker labels
LABEL org.opencontainers.image.title="FHIR Server" \
      org.opencontainers.image.description="FHIR compliant server implementation" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.vendor="TLQ" \
      maintainer="TLQ Team"

# Run as non-root
USER fhir

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD curl -fsS http://localhost:8080/health || exit 1

ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["fhir-server"]
