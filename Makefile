# riposte-social Deployment Makefile
# Follows the deployment instructions from README.md

# Load environment variables from .env file if it exists
ifneq (,$(wildcard .env))
include .env
export
endif

# Configuration - Uses values from .env or environment variables
DOCKER_IMAGE := riposte-social
ECR_REGISTRY ?= $(if $(ECR_REGISTRY_URL),$(ECR_REGISTRY_URL),$(error ECR_REGISTRY_URL not found. Create .env file or set environment variable))
ECR_REPOSITORY ?= $(if $(ECR_REPO_NAME),$(ECR_REPO_NAME),$(error ECR_REPO_NAME not found. Create .env file or set environment variable))
ECR_REGION ?= us-east-2
ECR_IMAGE := $(ECR_REGISTRY)/$(ECR_REPOSITORY):latest

# OCI Container Registry (OCIR) Configuration (optional)
OCIR_REGISTRY ?= $(OCIR_REGISTRY_URL)
OCIR_REPOSITORY ?= $(OCIR_REPO_NAME)
OCIR_REGION ?= $(if $(OCIR_REGION_NAME),$(OCIR_REGION_NAME),us-ashburn-1)
OCIR_IMAGE = $(if $(OCIR_REGISTRY),$(OCIR_REGISTRY)/$(OCIR_REPOSITORY):latest,)

# Default target
.PHONY: help
help:
	@echo "riposte-social - Development & Deployment Commands"
	@echo ""
	@echo "🗄️  Database Commands:"
	@echo "  make db-up          - Start PostgreSQL database"
	@echo "  make db-down        - Stop PostgreSQL database"
	@echo "  make db-logs        - View database logs"
	@echo "  make db-shell       - Open PostgreSQL shell"
	@echo "  make db-migrate     - Run database migrations"
	@echo "  make db-reset       - Reset database (WARNING: deletes all data)"
	@echo "  make db-backup      - Backup database to ./backups/"
	@echo "  make db-restore     - Restore database from backup"
	@echo ""
	@echo "🧪 Test Commands:"
	@echo "  make test               - Run tests with test database"
	@echo "  make test-db-up         - Start test database"
	@echo "  make test-db-down       - Stop test database"
	@echo "  make test-db-reset      - Reset test database"
	@echo "  make test-app-up        - Bring up containerized test stack (db + app)"
	@echo "  make test-app-down      - Stop containerized test stack"
	@echo "  make test-app-reset     - Reset containerized test stack to a fresh DB"
	@echo "  make cypress-feature    - Run Cypress feature specs against test stack"
	@echo "  make cypress-a11y       - Run Cypress a11y smoke against test stack"
	@echo "  make cypress-a11y-strict- Run Cypress a11y smoke (every impact level)"
	@echo "  make cypress-all        - Run every Cypress spec against test stack"
	@echo ""
	@echo "🛠️  Development Commands:"
	@echo "  make dev            - Start with hot reload (requires cargo-watch)"
	@echo "  make dev-no-watch   - Start without hot reload"
	@echo "  make dev-logs       - Tail application and database logs"
	@echo "  make clippy         - Run clippy linter"
	@echo ""
	@echo "🐳 Docker Commands:"
	@echo "  make build          - Build Docker image locally"
	@echo "  make run            - Run container locally (requires ACCESS_CODES env var)"
	@echo "  make deploy         - Complete deployment: build + push to ECR"
	@echo "  make push-ecr       - Push to ECR (after build)"
	@echo "  make login-ecr      - Login to ECR"
	@echo "  make deploy-ocir    - Complete deployment: build + push to OCIR"
	@echo "  make push-ocir      - Push to OCIR (after build)"
	@echo "  make login-ocir     - Login to OCIR"
	@echo "  make clean          - Remove local Docker images"
	@echo ""
	@echo "📋 Configuration:"
	@echo "  make show-config    - Display current configuration"
	@echo "  make check-prereqs  - Check for required tools"
	@echo ""
	@echo "Quick start:"
	@echo "  cp .env.example .env"
	@echo "  # Edit .env with your values"
	@echo "  make db-up          # Start database"
	@echo "  make db-migrate     # Run migrations"
	@echo "  make dev            # Start development server"

# Build the Docker image
.PHONY: build
build: frontend-build
	@echo "🔨 Building Docker image..."
	docker build \
		--build-arg SITE_DOMAIN=$(SITE_DOMAIN) \
		-t $(DOCKER_IMAGE) .
	@echo "✅ Build complete: $(DOCKER_IMAGE)"

# Login to ECR
.PHONY: login-ecr
login-ecr:
	@echo "🔐 Logging into ECR..."
	aws ecr get-login-password --region $(ECR_REGION) | docker login --username AWS --password-stdin $(ECR_REGISTRY)
	@echo "✅ ECR login successful"

# Tag and push to ECR
.PHONY: push-ecr
push-ecr: login-ecr
	@echo "🏷️  Tagging image for ECR..."
	docker tag $(DOCKER_IMAGE):latest $(ECR_IMAGE)
	@echo "📤 Pushing to ECR..."
	docker push $(ECR_IMAGE)
	@echo "✅ Push complete: $(ECR_IMAGE)"

# Complete deployment (build + push)
.PHONY: deploy
deploy: build push-ecr
	@echo ""
	@echo "🚀 Deployment complete!"
	@echo "📋 Image pushed to: $(ECR_IMAGE)"
	@echo ""
	@echo "Next steps:"
	@echo "1. The image is now available in ECR"
	@echo "2. The vpn-server docker-compose will pull this image automatically"
	@echo "3. Deploy infrastructure changes if needed via vpn-server project"

# Login to OCI Container Registry (OCIR)
.PHONY: login-ocir
login-ocir:
	@if [ -z "$(OCIR_REGISTRY)" ]; then echo "Error: OCIR_REGISTRY_URL not set"; exit 1; fi
	@if [ -z "$(OCIR_USERNAME)" ]; then echo "Error: OCIR_USERNAME not set"; exit 1; fi
	@echo "Logging into OCIR..."
	@echo "$(OCIR_AUTH_TOKEN)" | docker login $(OCIR_REGISTRY) -u '$(OCIR_USERNAME)' --password-stdin
	@echo "OCIR login successful"

# Tag and push to OCIR
.PHONY: push-ocir
push-ocir: login-ocir
	@echo "Tagging image for OCIR..."
	docker tag $(DOCKER_IMAGE):latest $(OCIR_IMAGE)
	@echo "Pushing to OCIR..."
	docker push $(OCIR_IMAGE)
	@echo "Push complete: $(OCIR_IMAGE)"

# Complete OCIR deployment (build + push)
.PHONY: deploy-ocir
deploy-ocir: build push-ocir
	@echo ""
	@echo "OCIR Deployment complete!"
	@echo "Image pushed to: $(OCIR_IMAGE)"

# Run locally for testing
.PHONY: run
run:
ifndef ACCESS_CODES
	$(error ACCESS_CODES environment variable is required. Example: make run ACCESS_CODES="test123,demo456")
endif
	@echo "🏃 Running container locally..."
	docker run -p 3000:3000 -e ACCESS_CODES="$(ACCESS_CODES)" $(DOCKER_IMAGE)

# Clean up local images and build artifacts
.PHONY: clean
clean:
	@echo "🧹 Cleaning up..."
	-docker rmi $(DOCKER_IMAGE):latest
	-docker rmi $(ECR_IMAGE)
	-if [ -n "$(OCIR_IMAGE)" ]; then docker rmi $(OCIR_IMAGE) 2>/dev/null; fi
	cargo clean
	@echo "✅ Cleanup complete"

# Check prerequisites
.PHONY: check-prereqs
check-prereqs:
	@echo "🔍 Checking prerequisites..."
	@command -v docker >/dev/null 2>&1 || { echo "❌ Docker is required but not installed"; exit 1; }
	@command -v aws >/dev/null 2>&1 || { echo "❌ AWS CLI is required but not installed"; exit 1; }
	@aws sts get-caller-identity >/dev/null 2>&1 || { echo "❌ AWS CLI not configured or no permissions"; exit 1; }
	@echo "✅ All prerequisites met"

# Show current configuration
.PHONY: show-config
show-config:
	@echo "📋 Current Configuration:"
	@echo "  Docker Image: $(DOCKER_IMAGE)"
	@echo "  ECR Registry: $(if $(ECR_REGISTRY_URL),$(ECR_REGISTRY_URL),❌ Not set)"
	@echo "  ECR Repository: $(if $(ECR_REPO_NAME),$(ECR_REPO_NAME),❌ Not set)"
	@echo "  ECR Region: $(ECR_REGION)"
	@echo "  Full ECR Image: $(if $(ECR_REGISTRY_URL),$(if $(ECR_REPO_NAME),$(ECR_IMAGE),❌ Missing repo name),❌ Missing registry)"
	@echo ""
	@echo "  OCIR Registry: $(if $(OCIR_REGISTRY_URL),$(OCIR_REGISTRY_URL),Not set)"
	@echo "  OCIR Repository: $(if $(OCIR_REPO_NAME),$(OCIR_REPO_NAME),Not set)"
	@echo "  OCIR Region: $(OCIR_REGION)"
	@echo "  Full OCIR Image: $(if $(OCIR_REGISTRY_URL),$(if $(OCIR_REPO_NAME),$(OCIR_IMAGE),Not set),Not set)"
	@echo ""
	@echo "  Database URL: $(if $(DATABASE_URL),✅ Set,❌ Not set)"
	@echo "  Access Codes: $(if $(ACCESS_CODES),✅ Set,❌ Not set)"

#
# Database Management Commands
#

# Start PostgreSQL database
.PHONY: db-up
db-up:
	@echo "🚀 Starting PostgreSQL database..."
	docker-compose up -d postgres
	@echo "⏳ Waiting for database to be ready..."
	@sleep 5
	@docker-compose exec postgres pg_isready -U $${POSTGRES_USER:-riposte_social_user} || echo "Waiting..."
	@echo "✅ Database is ready!"
	@echo "📍 Connection: postgresql://$${POSTGRES_USER:-riposte_social_user}:****@localhost:$${POSTGRES_PORT:-5432}/$${POSTGRES_DB:-riposte_social}"

# Stop PostgreSQL database
.PHONY: db-down
db-down:
	@echo "🛑 Stopping PostgreSQL database..."
	docker-compose down
	@echo "✅ Database stopped"

# View database logs
.PHONY: db-logs
db-logs:
	docker-compose logs -f postgres

# Open PostgreSQL shell
.PHONY: db-shell
db-shell:
	@echo "🐘 Opening PostgreSQL shell..."
	docker-compose exec postgres psql -U $${POSTGRES_USER:-riposte_social_user} -d $${POSTGRES_DB:-riposte_social}

# Run database migrations
.PHONY: db-migrate
db-migrate:
	@echo "🔄 Running database migrations..."
	MIGRATE_DB=true cargo run -- migrate
	@echo "✅ Migrations complete!"

# Reset database (WARNING: deletes all data)
.PHONY: db-reset
db-reset:
	@echo "⚠️  WARNING: This will delete all data in the database!"
	@read -p "Are you sure? Type 'yes' to continue: " confirm; \
	if [ "$$confirm" = "yes" ]; then \
		echo "🗑️  Resetting database..."; \
		docker-compose down -v; \
		docker-compose up -d postgres; \
		sleep 5; \
		MIGRATE_DB=true cargo run -- migrate; \
		echo "✅ Database reset complete!"; \
	else \
		echo "❌ Reset cancelled"; \
	fi

# Backup database
.PHONY: db-backup
db-backup:
	@echo "💾 Creating database backup..."
	@mkdir -p backups
	@BACKUP_FILE="backups/riposte_social_$$(date +%Y%m%d_%H%M%S).sql"; \
	docker-compose exec -T postgres pg_dump -U $${POSTGRES_USER:-riposte_social_user} $${POSTGRES_DB:-riposte_social} > $$BACKUP_FILE; \
	echo "✅ Backup created: $$BACKUP_FILE"

# Restore database from backup
.PHONY: db-restore
db-restore:
	@echo "📂 Available backups:"
	@ls -lh backups/*.sql 2>/dev/null || echo "No backups found"
	@read -p "Enter backup filename (e.g., backups/riposte_social_20250119_120000.sql): " backup; \
	if [ -f "$$backup" ]; then \
		echo "♻️  Restoring from $$backup..."; \
		docker-compose exec -T postgres psql -U $${POSTGRES_USER:-riposte_social_user} $${POSTGRES_DB:-riposte_social} < $$backup; \
		echo "✅ Restore complete!"; \
	else \
		echo "❌ Backup file not found: $$backup"; \
	fi

#
# Test Database Commands
#

# Start test database
.PHONY: test-db-up
test-db-up:
	@echo "🚀 Starting test database..."
	docker-compose -f docker-compose.test.yml up -d
	@echo "⏳ Waiting for test database to be ready..."
	@sleep 5
	@docker-compose -f docker-compose.test.yml exec postgres-test pg_isready -U $${TEST_POSTGRES_USER:-riposte_social_test_user} || echo "Waiting..."
	@echo "✅ Test database is ready!"
	@echo "📍 Connection: postgresql://$${TEST_POSTGRES_USER:-riposte_social_test_user}:****@localhost:$${TEST_POSTGRES_PORT:-5433}/$${TEST_POSTGRES_DB:-riposte_social_test}"

# Stop test database
.PHONY: test-db-down
test-db-down:
	@echo "🛑 Stopping test database..."
	docker-compose -f docker-compose.test.yml down
	@echo "✅ Test database stopped"

# Reset test database
.PHONY: test-db-reset
test-db-reset:
	@echo "🗑️  Resetting test database..."
	docker-compose -f docker-compose.test.yml down -v
	docker-compose -f docker-compose.test.yml up -d
	@sleep 5
	@echo "✅ Test database reset complete!"

# Run tests with test database
.PHONY: test
test: test-db-up
	@echo "🧪 Running tests..."
	DATABASE_URL="postgresql://$${TEST_POSTGRES_USER:-riposte_social_test_user}:$${TEST_POSTGRES_PASSWORD:-test_password}@localhost:$${TEST_POSTGRES_PORT:-5433}/$${TEST_POSTGRES_DB:-riposte_social_test}" \
	TEST_DATABASE_URL="postgresql://$${TEST_POSTGRES_USER:-riposte_social_test_user}:$${TEST_POSTGRES_PASSWORD:-test_password}@localhost:$${TEST_POSTGRES_PORT:-5433}/$${TEST_POSTGRES_DB:-riposte_social_test}" \
	cargo test

# Bring up the containerized test stack: postgres-test + the
# riposte-social app-test container that runs migrations and seeds a
# known-credential admin. The app is exposed on TEST_APP_PORT (3001
# by default) so it can run alongside `make dev` (port 3000).
.PHONY: test-app-up
test-app-up:
	@echo "🚀 Starting test stack (db + app)..."
	docker-compose -f docker-compose.test.yml --profile app up -d --build
	@echo "⏳ Waiting for test app to be ready..."
	@for i in $$(seq 1 30); do \
		if curl -fs http://localhost:$${TEST_APP_PORT:-3001}/health >/dev/null 2>&1; then \
			echo "✅ Test app is up at http://localhost:$${TEST_APP_PORT:-3001}"; \
			exit 0; \
		fi; \
		sleep 2; \
	done; \
	echo "⚠️  Test app did not respond within 60s. See: docker logs riposte-social-test-app"; \
	exit 1

# Stop the containerized test stack.
.PHONY: test-app-down
test-app-down:
	@echo "🛑 Stopping test stack..."
	docker-compose -f docker-compose.test.yml --profile app down
	@echo "✅ Test stack stopped"

# Reset the containerized test stack to a fresh DB. Brings the stack
# down with `-v` so the named postgres_test_data volume is destroyed,
# then rebuilds. Use this when a previous run mutated the DB and you
# want a deterministic seed/migration starting point.
.PHONY: test-app-reset
test-app-reset:
	@echo "🗑️  Resetting test stack (drops postgres_test_data volume)..."
	docker-compose -f docker-compose.test.yml --profile app down -v
	@$(MAKE) test-app-up

# Tail logs from the test app container.
.PHONY: test-app-logs
test-app-logs:
	docker-compose -f docker-compose.test.yml --profile app logs -f app-test

#
# Cypress Commands
#
# Each target ensures the containerized test stack is up and points
# Cypress at it via CYPRESS_BASE_URL. They use the dockerized cypress
# runner (cypress/included) so the host doesn't need a local cypress
# install. Stack stays up after the run; use `make test-app-down` to
# stop it or `make test-app-reset` to wipe the DB.
#
# Capture is opt-in. Prefix any target to record:
#   CYPRESS_SCREENSHOTS=true make cypress-feature   (failure screenshots)
#   CYPRESS_VIDEO=true make cypress-feature         (per-spec mp4)
# Artifacts land in cypress/{screenshots,videos}/ (both gitignored).

# Run the feature-track Cypress specs (text-only, S3-free). Currently
# the unified posts/albums kind discriminator coverage. Use this for
# fast feedback on backend route behavior without spinning up MinIO.
.PHONY: cypress-feature
cypress-feature: test-app-up
	@echo "🧪 Running Cypress feature specs..."
	CYPRESS_BASE_URL=http://localhost:$${TEST_APP_PORT:-3001} bun run e2e:feature:docker
	@echo "✅ Feature specs complete"

# Run the a11y smoke at the gate level (serious + critical impacts).
.PHONY: cypress-a11y
cypress-a11y: test-app-up
	@echo "♿ Running Cypress a11y smoke (ci strictness)..."
	CYPRESS_BASE_URL=http://localhost:$${TEST_APP_PORT:-3001} bun run a11y:smoke:docker
	@echo "✅ a11y smoke complete"

# Run the a11y smoke at strict (every axe impact level surfaced).
# Useful during development to see the full violation picture.
.PHONY: cypress-a11y-strict
cypress-a11y-strict: test-app-up
	@echo "♿ Running Cypress a11y smoke (strict strictness)..."
	CYPRESS_BASE_URL=http://localhost:$${TEST_APP_PORT:-3001} bun run a11y:smoke:strict:docker
	@echo "✅ a11y smoke (strict) complete"

# Run every Cypress spec against the test stack.
.PHONY: cypress-all
cypress-all: test-app-up
	@echo "🧪 Running all Cypress specs..."
	CYPRESS_BASE_URL=http://localhost:$${TEST_APP_PORT:-3001} bun run e2e:docker
	@echo "✅ All Cypress specs complete"

#
# Development Commands
#

# Start development environment with hot reload
.PHONY: dev
dev: db-up frontend-build
	@echo "🔧 Starting development servers with hot reload..."
	@echo "⚛️  Admin + social frontends will watch for changes"
	@echo "🦀 Cargo will watch for Rust changes"
	@echo "📝 Press Ctrl+C to stop all servers"
	@echo ""
	DEV_MODE=true \
	make -j3 admin-watch social-watch rust-watch

# Admin frontend watch mode (auto-rebuild on changes)
.PHONY: admin-watch
admin-watch:
	@echo "⚛️  Starting Admin frontend in watch mode..."
	@bun run build:watch

# Social frontend watch mode (auto-rebuild on changes)
.PHONY: social-watch
social-watch:
	@echo "⚛️  Starting Social frontend in watch mode..."
	@bun run build:watch:social

# Rust watch mode (auto-reload on changes using cargo-watch). Builds
# with the `e2e_testing` feature so the DEV_MODE runtime override
# (socket-address IP extraction, non-Secure invite cookies) is
# compiled in. Production release builds intentionally strip those
# overrides; a localhost dev server needs them.
.PHONY: rust-watch
rust-watch:
	@echo "🦀 Starting Rust in watch mode..."
	@command -v cargo-watch >/dev/null 2>&1 || { echo "Installing cargo-watch..."; cargo install cargo-watch; }
	@cargo watch -x 'run --release --features e2e_testing'

# Run Rust server without watching
.PHONY: rust-run
rust-run:
	@echo "🦀 Starting Rust server..."
	@cargo run --release --features e2e_testing

# Development without watch (manual restart required for changes)
.PHONY: dev-no-watch
dev-no-watch: db-up frontend-build
	@echo "🔧 Starting development server (no watch)..."
	@echo "📝 Logs will appear below. Press Ctrl+C to stop."
	@echo ""
	cargo run --features e2e_testing

# Tail development logs
.PHONY: dev-logs
dev-logs:
	@echo "📋 Tailing logs (Ctrl+C to exit)..."
	docker-compose logs -f postgres

# Run clippy
.PHONY: clippy
clippy:
	@echo "📎 Running clippy..."
	cargo clippy -- -D warnings

# Full development setup
.PHONY: setup
setup:
	@echo "🚀 Setting up development environment..."
	@if [ ! -f .env ]; then \
		echo "📝 Creating .env from .env.example..."; \
		cp .env.example .env; \
		echo "⚠️  Please edit .env with your configuration"; \
	else \
		echo "✅ .env file already exists"; \
	fi
	@echo "📦 Installing admin frontend dependencies..."
	bun install
	@echo "🔄 Running migrations..."
	@$(MAKE) db-migrate
	@echo ""
	@echo "✅ Setup complete! Run 'make dev' to start the server"

#
# Frontend Build Commands
#

# Build admin frontend for production
.PHONY: admin-build
admin-build:
	@echo "🔨 Building admin frontend..."
	@if [ ! -d "node_modules" ]; then \
		echo "📦 Installing dependencies first..."; \
		bun install; \
	fi
	bun run build:admin
	@echo "✅ Admin frontend built to admin-assets/"

# Build social frontend for production
.PHONY: social-build
social-build:
	@echo "🔨 Building social frontend..."
	@if [ ! -d "node_modules" ]; then \
		echo "📦 Installing dependencies first..."; \
		bun install; \
	fi
	bun run build:social
	@echo "✅ Social frontend built to social-assets/"

# Build public Astro site
.PHONY: public-build
public-build:
	@echo "🔨 Building public Astro site..."
	@if [ -n "$(PUBLIC_FRONTEND_PATH)" ] && [ -d "$(PUBLIC_FRONTEND_PATH)" ]; then \
		echo "📂 Using external frontend: $(PUBLIC_FRONTEND_PATH)"; \
		if [ ! -d "$(PUBLIC_FRONTEND_PATH)/node_modules" ]; then \
			echo "📦 Installing Astro dependencies..."; \
			(cd "$(PUBLIC_FRONTEND_PATH)" && npm install); \
		fi && \
		(cd "$(PUBLIC_FRONTEND_PATH)" && SITE_URL="$(SITE_URL)" npm run build) && \
		if [ -d "$(PUBLIC_FRONTEND_PATH)/dist" ]; then \
			rm -rf public-assets && cp -r "$(PUBLIC_FRONTEND_PATH)/dist" public-assets; \
		else \
			echo "❌ ERROR: External site did not output to dist/"; \
			echo "   Your astro.config.mjs must use the default outDir (dist/)"; \
			exit 1; \
		fi; \
	else \
		echo "📂 Using template frontend"; \
		if [ ! -d "public-frontend/node_modules" ]; then \
			echo "📦 Installing Astro dependencies first..."; \
			(cd public-frontend && npm install); \
		fi && \
		(cd public-frontend && SITE_URL="$(SITE_URL)" npm run build); \
	fi
	@echo "✅ Public site built to public-assets/"

# Build all frontends
.PHONY: frontend-build
frontend-build: admin-build social-build
	@echo "✅ All frontends built!"
