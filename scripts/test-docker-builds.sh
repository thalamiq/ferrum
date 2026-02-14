#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"

echo "=== Testing Docker builds ==="
echo ""

# Server
echo "[1/2] Building server image..."
docker build \
  -f "${REPO_ROOT}/apps/server/Dockerfile" \
  -t ferrum-server:test \
  "${REPO_ROOT}"
echo "  Server image built successfully."
echo ""

# Admin UI
echo "[2/2] Building admin-ui image..."
docker build \
  -f "${REPO_ROOT}/apps/admin-ui/Dockerfile" \
  -t ferrum-ui:test \
  "${REPO_ROOT}/apps/admin-ui"
echo "  Admin UI image built successfully."
echo ""

echo "=== All Docker builds passed ==="
