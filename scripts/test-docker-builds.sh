#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"

echo "=== Testing Docker builds ==="
echo ""

# Server (includes admin UI build stage)
echo "[1/1] Building server image (includes admin UI)..."
docker build \
  -f "${REPO_ROOT}/apps/server/Dockerfile" \
  -t ferrum-server:test \
  "${REPO_ROOT}"
echo "  Server image built successfully."
echo ""

echo "=== All Docker builds passed ==="
