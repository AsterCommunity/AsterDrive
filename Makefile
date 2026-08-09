SHELL := /bin/sh

.DEFAULT_GOAL := help

CARGO ?= cargo
BUN ?= bun
DOCKER_COMPOSE ?= docker compose

FRONTEND_DIR := frontend-panel
DOCS_DIR := docs

FILTER ?=
TEST ?=
BACKEND ?=
TEST_ARGS ?=
E2E_ARGS ?=
RUN_ARGS ?=

TEST_ENV = $(if $(strip $(BACKEND)),ASTER_TEST_DATABASE_BACKEND=$(BACKEND),)

.PHONY: \
	help setup setup-all backend-deps frontend-deps docs-deps \
	dev dev-backend dev-frontend run \
	build build-backend build-frontend build-release \
	check check-backend check-frontend typecheck \
	format format-check clippy \
	test test-backend test-frontend test-lib test-integration test-e2e \
	coverage coverage-backend coverage-frontend \
	openapi openapi-check \
	storage-docs storage-docs-check docs-dev docs-build docs-preview \
	docker-build docker-up docker-down docker-logs \
	ci

help: ## Show available targets
	@awk 'BEGIN {FS = ":.*## "; printf "Usage: make <target> [VARIABLE=value]\n\nTargets:\n"} /^[a-zA-Z0-9_-]+:.*## / {printf "  %-20s %s\n", $$1, $$2}' $(MAKEFILE_LIST)
	@printf '\nExamples:\n'
	@printf '  make test-lib FILTER=services::auth\n'
	@printf '  make test-integration TEST=test_auth FILTER=test_login\n'
	@printf '  make test-integration TEST=test_database_backends BACKEND=postgres\n'

setup: backend-deps frontend-deps ## Fetch backend and frontend dependencies

setup-all: setup docs-deps ## Fetch all dependencies, including documentation

backend-deps: ## Fetch Rust dependencies
	$(CARGO) fetch

frontend-deps: ## Install frontend dependencies from the lockfile
	cd $(FRONTEND_DIR) && $(BUN) install --frozen-lockfile

docs-deps: ## Install documentation dependencies from the lockfile
	cd $(DOCS_DIR) && $(BUN) install --frozen-lockfile

dev: ## Run backend and frontend development servers
	@set -eu; \
	$(CARGO) run & backend_pid=$$!; \
	(cd $(FRONTEND_DIR) && $(BUN) run dev) & frontend_pid=$$!; \
	trap 'kill $$backend_pid $$frontend_pid 2>/dev/null || true' INT TERM EXIT; \
	wait

dev-backend: ## Run the backend development server
	$(CARGO) run

dev-frontend: ## Run the frontend development server
	cd $(FRONTEND_DIR) && $(BUN) run dev

run: build-frontend ## Build the embedded frontend and run AsterDrive
	$(CARGO) run -- $(RUN_ARGS)

build: build-frontend build-backend ## Build frontend and backend debug artifacts

build-backend: ## Build the backend
	$(CARGO) build

build-frontend: ## Type-check and build the frontend
	cd $(FRONTEND_DIR) && $(BUN) run build

build-release: build-frontend ## Build the frontend and optimized backend binary
	$(CARGO) build --release

check: check-backend check-frontend ## Run standard backend and frontend checks

check-backend: format-check ## Check backend compilation and Rust formatting
	$(CARGO) check

check-frontend: ## Run frontend type checking and Biome checks
	cd $(FRONTEND_DIR) && $(BUN) run check

typecheck: ## Run frontend TypeScript checks
	cd $(FRONTEND_DIR) && $(BUN) run typecheck

format: ## Format Rust and frontend sources
	$(CARGO) fmt --all
	cd $(FRONTEND_DIR) && $(BUN) run format

format-check: ## Verify Rust formatting
	$(CARGO) fmt --all -- --check

clippy: ## Run Clippy with the same strict settings as CI
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

test: test-backend test-frontend ## Run the complete backend and frontend test suites

test-backend: ## Run all Rust workspace tests
	$(TEST_ENV) $(CARGO) test --workspace --no-fail-fast $(TEST_ARGS)

test-frontend: ## Run frontend unit tests
	cd $(FRONTEND_DIR) && $(BUN) run test

test-lib: ## Run a targeted Rust library test (requires FILTER)
	@test -n "$(strip $(FILTER))" || { echo "FILTER is required, for example: make test-lib FILTER=services::auth"; exit 2; }
	$(TEST_ENV) $(CARGO) test --lib $(FILTER) $(TEST_ARGS)

test-integration: ## Run a targeted integration test (requires TEST; optional FILTER/BACKEND)
	@test -n "$(strip $(TEST))" || { echo "TEST is required, for example: make test-integration TEST=test_auth"; exit 2; }
	$(TEST_ENV) $(CARGO) test --test $(TEST) $(FILTER) $(TEST_ARGS)

test-e2e: ## Run Playwright end-to-end tests (use E2E_ARGS for extra arguments)
	cd $(FRONTEND_DIR) && $(BUN) run test:e2e -- $(E2E_ARGS)

coverage: coverage-backend coverage-frontend ## Generate backend and frontend coverage reports

coverage-backend: ## Generate Rust LCOV and HTML coverage reports
	mkdir -p coverage/rust
	$(CARGO) llvm-cov --workspace --no-fail-fast --lcov --output-path coverage/rust/lcov.info
	$(CARGO) llvm-cov report --html --output-dir coverage/rust/html

coverage-frontend: ## Generate frontend coverage reports
	cd $(FRONTEND_DIR) && $(BUN) run test:coverage

openapi: ## Regenerate the OpenAPI document and TypeScript SDK
	$(CARGO) test --features openapi --test generate_openapi
	cd $(FRONTEND_DIR) && $(BUN) run generate-api

openapi-check: openapi ## Verify generated OpenAPI and SDK files have no drift
	git diff --exit-code -- \
		$(FRONTEND_DIR)/generated/openapi.json \
		$(FRONTEND_DIR)/src/services/api.generated.ts

storage-docs: ## Regenerate the built-in storage connector manifest and documentation blocks
	ASTER_UPDATE_STORAGE_CONNECTOR_DOCS=1 $(CARGO) test --test storage_connector_docs generated_storage_connector_docs_are_current -- --exact

storage-docs-check: ## Verify storage connector documentation has no descriptor drift
	env -u ASTER_UPDATE_STORAGE_CONNECTOR_DOCS $(CARGO) test --test storage_connector_docs generated_storage_connector_docs_are_current -- --exact

docs-dev: ## Run the documentation development server
	cd $(DOCS_DIR) && $(BUN) run docs:dev

docs-build: ## Build the documentation site
	cd $(DOCS_DIR) && $(BUN) run docs:build

docs-preview: ## Preview the built documentation site
	cd $(DOCS_DIR) && $(BUN) run docs:preview

docker-build: ## Build the local Docker image
	$(DOCKER_COMPOSE) build

docker-up: ## Start the Docker Compose services
	$(DOCKER_COMPOSE) up -d

docker-down: ## Stop the Docker Compose services
	$(DOCKER_COMPOSE) down

docker-logs: ## Follow AsterDrive Docker Compose logs
	$(DOCKER_COMPOSE) logs -f asterdrive

ci: ## Run the main local CI verification bundle
	$(MAKE) check
	$(MAKE) clippy
	$(MAKE) test
	$(MAKE) openapi-check
	$(MAKE) build-frontend
