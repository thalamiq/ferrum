.PHONY: help dev-server dev-server-debug dev-worker dev-ui dev build-server lint lint-fix fmt fmt-check
.PHONY: docker-dev docker-prod docker-minimal docker-full docker-db-monitoring docker-stop docker-logs docker-clean
.PHONY: docker-build-server docker-build-ui

# Default target
help:
	@echo "Available targets:"
	@echo ""
	@echo "Development (local, no Docker):"
	@echo "  dev-server       - Run FHIR server with auto-reload"
	@echo "  dev-server-debug - Run FHIR server with debug logging"
	@echo "  dev-worker       - Run background worker with auto-reload"
	@echo "  dev-ui           - Run Next.js UI in dev mode"
	@echo "  dev              - Run all services locally"
	@echo ""
	@echo "Docker Deployment:"
	@echo "  docker-dev           - Start development environment (all services)"
	@echo "  docker-prod          - Start production environment (server + monitoring)"
	@echo "  docker-minimal       - Start minimal environment (server + db only)"
	@echo "  docker-db-monitoring - Start database + monitoring (for local server dev)"
	@echo "  docker-full          - Start all services in production mode"
	@echo "  docker-stop          - Stop all Docker services"
	@echo "  docker-logs          - Follow Docker logs"
	@echo "  docker-clean         - Stop services and remove volumes"
	@echo ""
	@echo "Build:"
	@echo "  build-server          - Build server release binary"
	@echo "  docker-build-server   - Build server Docker image"
	@echo "  docker-build-ui       - Build UI Docker image"
	@echo ""
	@echo "Code Quality:"
	@echo "  lint             - Run clippy linter"
	@echo "  lint-fix         - Fix clippy warnings automatically"
	@echo "  fmt              - Format code"
	@echo "  fmt-check        - Check code formatting"

# Local Development (no Docker)
dev-server:
	cd server && cargo watch -x 'run --bin fhir-server'

dev-server-debug:
	RUST_LOG=fhir_server=debug,tower_http=debug cd server && cargo watch -x 'run --bin fhir-server'

dev-worker:
	cd server && cargo watch -x 'run --bin fhir-worker'

dev-ui:
	cd ui && pnpm dev

dev:
	make dev-server & make dev-worker & make dev-ui

# Build
build-server:
	cd server && cargo build --release

# Docker Deployment
docker-dev:
	docker-compose --env-file .env.development \
		-f docker/docker-compose.yaml \
		-f docker/docker-compose.monitoring.yaml \
		-f docker/docker-compose.ui.yaml \
		-f docker/docker-compose.dev.yaml \
		up -d

docker-prod:
	docker-compose --env-file .env.production \
		-f docker/docker-compose.yaml \
		-f docker/docker-compose.monitoring.yaml \
		up -d

docker-minimal:
	docker-compose --env-file .env.minimal \
		-f docker/docker-compose.yaml \
		up -d

docker-db-monitoring:
	docker-compose --env-file .env.development \
		-f docker/docker-compose.yaml \
		-f docker/docker-compose.monitoring.yaml \
		up db -d postgres-exporter prometheus grafana --build

docker-full:
	docker-compose --env-file .env.production \
		-f docker/docker-compose.yaml \
		-f docker/docker-compose.monitoring.yaml \
		-f docker/docker-compose.ui.yaml \
		up -d

docker-stop:
	docker-compose \
		-f docker/docker-compose.yaml \
		-f docker/docker-compose.monitoring.yaml \
		-f docker/docker-compose.ui.yaml \
		down

docker-logs:
	docker-compose \
		-f docker/docker-compose.yaml \
		-f docker/docker-compose.monitoring.yaml \
		-f docker/docker-compose.ui.yaml \
		logs -f

docker-clean:
	docker-compose \
		-f docker/docker-compose.yaml \
		-f docker/docker-compose.monitoring.yaml \
		-f docker/docker-compose.ui.yaml \
		down -v

# Docker Build
docker-build-server:
	@echo "Building server image from parent directory..."
	cd .. && docker build -f zunder/docker/server/Dockerfile -t fhir-server:latest .

docker-build-ui:
	docker build -f docker/ui/Dockerfile -t fhir-ui:latest .

# Linting
lint:
	cd server && cargo clippy --all-targets --all-features -- -D warnings

lint-fix:
	cd server && cargo clippy --fix --allow-dirty --allow-staged

# Format code
fmt:
	cd server && cargo fmt

fmt-check:
	cd server && cargo fmt --check