# riposte-social Deployment Makefile
#
# DEPRECATED. The bun CLI (tooling/cli.ts) replaces this Makefile and is the
# documented interface. This file is kept temporarily and will be removed.

$(info )
$(info ====================================================================)
$(info  DEPRECATED: do not use the Makefile. Use the bun CLI instead:)
$(info    bun tooling/cli.ts <verb> <site>    (run with no args for help))
$(info  The Makefile is kept temporarily and will be deleted.)
$(info ====================================================================)
$(info )

# Load environment. SITE picks a frontend; what it does depends on the target:
#   - dev targets (dev, dev-no-watch, shop-build): SITE selects which per-site
#     DEV env loads, sites/<name>.env, so you run/stage that site locally:
#       make dev SITE=cavebatsoftware          # loads sites/cavebatsoftware.env
#       make shop-build SITE=cavebatsoftware
#   - image-build/deploy targets (build, deploy-ghcr, ...): SITE only TAGS the
#     image so the right one uploads (GHCR_IMAGE_TAG defaults to the SITE name);
#     the build values come from the CLI on the build machine, e.g.
#       SITE_DOMAIN=cavebatsoftware.com SHOP_SRC=../cavebatsoftware-site ... \
#         make deploy-ghcr SITE=cavebatsoftware
# sites/<name>.env are dev files (gitignored, like .env). With no SITE: dev/test/
# db read .env; prod deploy targets read deploy/.env.prod.
PROD_TARGETS := deploy-ghcr deploy deploy-ocir
DEV_SITE_TARGETS := dev dev-no-watch shop-build

ENV_FILE := .env
ifneq (,$(filter $(DEV_SITE_TARGETS),$(MAKECMDGOALS)))
ifneq (,$(SITE))
ENV_FILE := sites/$(SITE).env
ifeq (,$(wildcard $(ENV_FILE)))
$(error SITE env not found: $(ENV_FILE). Available: $(patsubst sites/%.env,%,$(wildcard sites/*.env)))
endif
endif
else ifneq (,$(filter $(PROD_TARGETS),$(MAKECMDGOALS)))
ENV_FILE := deploy/.env.prod
endif

ifneq (,$(wildcard $(ENV_FILE)))
include $(ENV_FILE)
export
endif

# Configuration - Uses values from .env or environment variables
DOCKER_IMAGE := riposte-social
ECR_REGISTRY ?= $(if $(ECR_REGISTRY_URL),$(ECR_REGISTRY_URL),$(error ECR_REGISTRY_URL not found. Create .env file or set environment variable))
ECR_REPOSITORY ?= $(if $(ECR_REPO_NAME),$(ECR_REPO_NAME),$(error ECR_REPO_NAME not found. Create .env file or set environment variable))
ECR_REGION ?= us-east-2
# Lazy: only ECR targets (push-ecr/deploy/clean) need ECR config, so don't force
# the ECR_REGISTRY_URL check at parse time for GHCR / SITE-tagged builds.
ECR_IMAGE = $(ECR_REGISTRY)/$(ECR_REPOSITORY):latest

# OCI Container Registry (OCIR) Configuration (optional)
OCIR_REGISTRY ?= $(OCIR_REGISTRY_URL)
OCIR_REPOSITORY ?= $(OCIR_REPO_NAME)
OCIR_REGION ?= $(if $(OCIR_REGION_NAME),$(OCIR_REGION_NAME),us-ashburn-1)
OCIR_IMAGE = $(if $(OCIR_REGISTRY),$(OCIR_REGISTRY)/$(OCIR_REPOSITORY):latest,)

# GitHub Container Registry (GHCR) Configuration. Manual deploy path: build
# locally, push here, server pulls (see deploy/README.md). GHCR_TOKEN is a
# GitHub token with write:packages (PAT or `gh auth token`).
GHCR_REGISTRY ?= ghcr.io
GHCR_OWNER ?= cavebatsofware
# For image builds, SITE tags the image so the right one uploads (override with
# an explicit GHCR_IMAGE_TAG for a versioned release). No SITE -> latest.
GHCR_IMAGE_TAG ?= $(if $(SITE),$(SITE),latest)
GHCR_IMAGE := $(GHCR_REGISTRY)/$(GHCR_OWNER)/$(DOCKER_IMAGE):$(GHCR_IMAGE_TAG)

# Build mode is keyed off APP_ROLE (the same indicator used at runtime): a
# `shop` or `both` role compiles, runs, migrates, and packages with the
# `business` feature and stages the storefront; `social` (or unset) is
# social-only. Set APP_ROLE in .env so every job adapts with no extra flags.
ifneq (,$(filter shop both,$(APP_ROLE)))
SHOP_FEATURE := business
else
SHOP_FEATURE :=
endif

# Render a `--features a,b` flag from a space-separated list ($1); empty if none.
empty :=
space := $(empty) $(empty)
comma := ,
features_flag = $(if $(strip $(1)),--features $(subst $(space),$(comma),$(strip $(1))),)

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
	@echo "  make shop-build     - Build + stage a storefront export (SHOP_SRC, SHOP_DIST)"
	@echo "  SITE=<name>         - dev: load sites/<name>.env; image build: tag the image"
	@echo "  make build-business - Build the business image (orders + /shop + SMS)"
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

# Build the Docker image. When APP_ROLE is shop/both this is a business image:
# it stages the storefront and compiles with the business feature. Otherwise
# it's a social-only image. Driven entirely by APP_ROLE (.env), no extra flags.
.PHONY: build
build: frontend-build $(if $(SHOP_FEATURE),shop-build)
	@echo "🔨 Building Docker image (role=$(if $(APP_ROLE),$(APP_ROLE),social))..."
	docker build \
		--build-arg SITE_DOMAIN=$(SITE_DOMAIN) \
		$(if $(SHOP_FEATURE),--build-arg CARGO_FEATURES=$(SHOP_FEATURE)) \
		-t $(DOCKER_IMAGE) .
	@echo "✅ Build complete: $(DOCKER_IMAGE)"

# Build a storefront's static assets with bun and stage them into ./shop-assets
# so the Dockerfile can COPY them. Framework-neutral: any project whose build
# emits static html/js/css works. SHOP_SRC is the project directory; SHOP_DIST
# is the directory its build emits.
#   make shop-build SHOP_SRC=../storefront SHOP_DIST=../storefront/out
.PHONY: shop-build
shop-build:
ifndef SHOP_SRC
	$(error SHOP_SRC is required. Example: make shop-build SHOP_SRC=../storefront SHOP_DIST=../storefront/out)
endif
ifndef SHOP_DIST
	$(error SHOP_DIST is required. Example: make shop-build SHOP_SRC=../storefront SHOP_DIST=../storefront/out)
endif
	@test -d "$(SHOP_SRC)" || { echo "❌ storefront source not found: $(SHOP_SRC)"; exit 1; }
	@echo "🔨 Building storefront in $(SHOP_SRC)..."
	cd "$(SHOP_SRC)" && bun install && bun run build
	@test -d "$(SHOP_DIST)" || { echo "❌ build output not found: $(SHOP_DIST)"; exit 1; }
	@echo "📦 Staging $(SHOP_DIST) into ./shop-assets..."
	@find shop-assets -mindepth 1 ! -name .gitkeep -delete
	cp -r "$(SHOP_DIST)/." shop-assets/
	@echo "✅ Storefront staged to shop-assets/"

# Convenience: force a business image regardless of the ambient APP_ROLE.
# Equivalent to `make build APP_ROLE=both`. Still needs SHOP_SRC / SHOP_DIST.
#   make build-business SHOP_SRC=../storefront SHOP_DIST=../storefront/out
.PHONY: build-business
build-business:
	@$(MAKE) build APP_ROLE=both

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

# Login to GitHub Container Registry. Needs GHCR_TOKEN (PAT / `gh auth token`
# with write:packages).
.PHONY: login-ghcr
login-ghcr:
	@if [ -z "$(GHCR_TOKEN)" ]; then echo "Error: GHCR_TOKEN not set (GitHub token with write:packages)"; exit 1; fi
	@echo "Logging into GHCR..."
	@echo "$(GHCR_TOKEN)" | docker login $(GHCR_REGISTRY) -u '$(GHCR_OWNER)' --password-stdin
	@echo "GHCR login successful"

# Tag and push to GHCR
.PHONY: push-ghcr
push-ghcr: login-ghcr
	@echo "Tagging image for GHCR..."
	docker tag $(DOCKER_IMAGE):latest $(GHCR_IMAGE)
	@echo "Pushing to GHCR..."
	docker push $(GHCR_IMAGE)
	@echo "Push complete: $(GHCR_IMAGE)"

# Complete GHCR deployment (build + push). Build values come from the CLI on the
# build machine; SITE tags the image so the right one uploads (GHCR_IMAGE_TAG
# defaults to the SITE name). SHOP_URL is the storefront's public origin, baked
# into its metadataBase, robots.txt, and sitemap.xml:
#   GHCR_TOKEN=... TURNSTILE_SITE_KEY=... SOCIAL_URL=... SHOP_URL=... \
#     make deploy-ghcr SITE=cavebatsoftware APP_ROLE=both \
#       SITE_DOMAIN=cavebatsoftware.com \
#       SHOP_SRC=../cavebatsoftware-site SHOP_DIST=../cavebatsoftware-site/out
# Then on the server: docker compose -f docker-compose.prod.yml pull && up -d
.PHONY: deploy-ghcr
deploy-ghcr: build push-ghcr
	@echo ""
	@echo "GHCR deployment complete!"
	@echo "Image pushed to: $(GHCR_IMAGE)"

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
	docker compose up -d postgres
	@echo "⏳ Waiting for database to be ready..."
	@sleep 5
	@docker compose exec postgres pg_isready -U $${POSTGRES_USER:-riposte_social_user} || echo "Waiting..."
	@echo "✅ Database is ready!"
	@echo "📍 Connection: postgresql://$${POSTGRES_USER:-riposte_social_user}:****@localhost:$${POSTGRES_PORT:-5432}/$${POSTGRES_DB:-riposte_social}"

# Stop PostgreSQL database
.PHONY: db-down
db-down:
	@echo "🛑 Stopping PostgreSQL database..."
	docker compose down
	@echo "✅ Database stopped"

# View database logs
.PHONY: db-logs
db-logs:
	docker compose logs -f postgres

# Open PostgreSQL shell
.PHONY: db-shell
db-shell:
	@echo "🐘 Opening PostgreSQL shell..."
	docker compose exec postgres psql -U $${POSTGRES_USER:-riposte_social_user} -d $${POSTGRES_DB:-riposte_social}

# Run database migrations
.PHONY: db-migrate
db-migrate:
	@echo "🔄 Running database migrations..."
	MIGRATE_DB=true cargo run $(call features_flag,$(SHOP_FEATURE)) -- migrate
	@echo "✅ Migrations complete!"

# Reset database (WARNING: deletes all data)
.PHONY: db-reset
db-reset:
	@echo "⚠️  WARNING: This will delete all data in the database!"
	@read -p "Are you sure? Type 'yes' to continue: " confirm; \
	if [ "$$confirm" = "yes" ]; then \
		echo "🗑️  Resetting database..."; \
		docker compose down -v; \
		docker compose up -d postgres; \
		sleep 5; \
		MIGRATE_DB=true cargo run $(call features_flag,$(SHOP_FEATURE)) -- migrate; \
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
	docker compose exec -T postgres pg_dump -U $${POSTGRES_USER:-riposte_social_user} $${POSTGRES_DB:-riposte_social} > $$BACKUP_FILE; \
	echo "✅ Backup created: $$BACKUP_FILE"

# Restore database from backup
.PHONY: db-restore
db-restore:
	@echo "📂 Available backups:"
	@ls -lh backups/*.sql 2>/dev/null || echo "No backups found"
	@read -p "Enter backup filename (e.g., backups/riposte_social_20250119_120000.sql): " backup; \
	if [ -f "$$backup" ]; then \
		echo "♻️  Restoring from $$backup..."; \
		docker compose exec -T postgres psql -U $${POSTGRES_USER:-riposte_social_user} $${POSTGRES_DB:-riposte_social} < $$backup; \
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
	docker compose -f docker-compose.test.yml up -d
	@echo "⏳ Waiting for test database to be ready..."
	@sleep 5
	@docker compose -f docker-compose.test.yml exec postgres-test pg_isready -U $${TEST_POSTGRES_USER:-riposte_social_test_user} || echo "Waiting..."
	@echo "✅ Test database is ready!"
	@echo "📍 Connection: postgresql://$${TEST_POSTGRES_USER:-riposte_social_test_user}:****@localhost:$${TEST_POSTGRES_PORT:-5433}/$${TEST_POSTGRES_DB:-riposte_social_test}"

# Stop test database
.PHONY: test-db-down
test-db-down:
	@echo "🛑 Stopping test database..."
	docker compose -f docker-compose.test.yml down
	@echo "✅ Test database stopped"

# Reset test database
.PHONY: test-db-reset
test-db-reset:
	@echo "🗑️  Resetting test database..."
	docker compose -f docker-compose.test.yml down -v
	docker compose -f docker-compose.test.yml up -d
	@sleep 5
	@echo "✅ Test database reset complete!"

# Run tests with test database
.PHONY: test
test: test-db-up
	@echo "🧪 Running tests..."
	DATABASE_URL="postgresql://$${TEST_POSTGRES_USER:-riposte_social_test_user}:$${TEST_POSTGRES_PASSWORD:-test_password}@localhost:$${TEST_POSTGRES_PORT:-5433}/$${TEST_POSTGRES_DB:-riposte_social_test}" \
	TEST_DATABASE_URL="postgresql://$${TEST_POSTGRES_USER:-riposte_social_test_user}:$${TEST_POSTGRES_PASSWORD:-test_password}@localhost:$${TEST_POSTGRES_PORT:-5433}/$${TEST_POSTGRES_DB:-riposte_social_test}" \
	SECURE_VALUES_KEY="$${SECURE_VALUES_KEY:-0000000000000000000000000000000000000000000000000000000000000000}" \
	cargo test $(call features_flag,e2e_testing $(SHOP_FEATURE))

# Bring up the containerized test stack: postgres-test + the
# riposte-social app-test container that runs migrations and seeds a
# known-credential admin. The app is exposed on port 3001 so it can
# run alongside `make dev` (port 3000).
.PHONY: test-app-up
test-app-up:
	@echo "🚀 Starting test stack (db + app)..."
	docker compose -f docker-compose.test.yml --profile app up -d --build
	@echo "⏳ Waiting for test app to be ready..."
	@for i in $$(seq 1 30); do \
		if curl -fs http://localhost:3001/health >/dev/null 2>&1; then \
			echo "✅ Test app is up at http://localhost:3001"; \
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
	docker compose -f docker-compose.test.yml --profile app down
	@echo "✅ Test stack stopped"

# Reset the containerized test stack to a fresh DB. Brings the stack
# down with `-v` so the named postgres_test_data volume is destroyed,
# then rebuilds. Use this when a previous run mutated the DB and you
# want a deterministic seed/migration starting point.
.PHONY: test-app-reset
test-app-reset:
	@echo "🗑️  Resetting test stack (drops postgres_test_data volume)..."
	docker compose -f docker-compose.test.yml --profile app down -v
	@$(MAKE) test-app-up

# Tail logs from the test app container.
.PHONY: test-app-logs
test-app-logs:
	docker compose -f docker-compose.test.yml --profile app logs -f app-test

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
	CYPRESS_BASE_URL=http://localhost:3001 bun run e2e:feature:docker
	@echo "✅ Feature specs complete"

# Run the a11y smoke at the gate level (serious + critical impacts).
.PHONY: cypress-a11y
cypress-a11y: test-app-up
	@echo "♿ Running Cypress a11y smoke (ci strictness)..."
	CYPRESS_BASE_URL=http://localhost:3001 bun run a11y:smoke:docker
	@echo "✅ a11y smoke complete"

# Run the a11y smoke at strict (every axe impact level surfaced).
# Useful during development to see the full violation picture.
.PHONY: cypress-a11y-strict
cypress-a11y-strict: test-app-up
	@echo "♿ Running Cypress a11y smoke (strict strictness)..."
	CYPRESS_BASE_URL=http://localhost:3001 bun run a11y:smoke:strict:docker
	@echo "✅ a11y smoke (strict) complete"

# Run every Cypress spec against the test stack.
.PHONY: cypress-all
cypress-all: test-app-up
	@echo "🧪 Running all Cypress specs..."
	CYPRESS_BASE_URL=http://localhost:3001 bun run e2e:docker
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
	@cargo watch -x 'run --release $(call features_flag,e2e_testing $(SHOP_FEATURE))'

# Run Rust server without watching
.PHONY: rust-run
rust-run:
	@echo "🦀 Starting Rust server..."
	@cargo run --release $(call features_flag,e2e_testing $(SHOP_FEATURE))

# Development without watch (manual restart required for changes)
.PHONY: dev-no-watch
dev-no-watch: db-up
	@echo "🔧 Starting development server (no watch)..."
	@echo "📝 Logs will appear below. Press Ctrl+C to stop."
	@echo ""
	make -j3 frontend-build rust-run

# Tail development logs
.PHONY: dev-logs
dev-logs:
	@echo "📋 Tailing logs (Ctrl+C to exit)..."
	docker compose logs -f postgres

# Run clippy
.PHONY: clippy
clippy:
	@echo "📎 Running clippy..."
	cargo clippy $(call features_flag,$(SHOP_FEATURE)) -- -D warnings

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
	NODE_ENV=production bun run build:admin
	@echo "✅ Admin frontend built to admin-assets/"

# Build social frontend for production
.PHONY: social-build
social-build:
	@echo "🔨 Building social frontend..."
	@if [ ! -d "node_modules" ]; then \
		echo "📦 Installing dependencies first..."; \
		bun install; \
	fi
	NODE_ENV=production bun run build:social
	@echo "✅ Social frontend built to social-assets/"

# Build all frontends
.PHONY: frontend-build
frontend-build: admin-build social-build
	@echo "✅ All frontends built!"
