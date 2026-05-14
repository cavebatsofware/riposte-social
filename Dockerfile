# Frontend build stage. Builds both the admin SPA (admin-assets/) and the
# social SPA (social-assets/). The two share package.json + node_modules
# and are produced from one `npm run build` call (admin then social).
FROM node:26-trixie-slim AS frontend-builder

WORKDIR /app

# Copy package files and install dependencies.
COPY package*.json ./
RUN npm ci

# Copy vite configs + both frontend source trees.
COPY vite.config.js ./
COPY vite.social.config.js ./
COPY admin-frontend ./admin-frontend
COPY social-frontend ./social-frontend

# Build both frontends. `npm run build` chains build:admin and build:social
# per package.json scripts and emits to admin-assets/ and social-assets/.
RUN npm run build

# Rust build stage. Debian 13 (trixie) image so the runtime stage can
# share the same glibc; default target is x86_64-unknown-linux-gnu.
FROM rust:slim-trixie AS builder

WORKDIR /app

# Optional cargo features. Empty in production builds (no dev / test
# code compiled in). The test compose sets this to `e2e_testing` so
# the `seed-test-admin` and `hash-password` subcommands and the
# `DEV_MODE` runtime overrides are available in the test container.
ARG CARGO_FEATURES=""

# Copy manifest + source.
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Cargo.toml is rustls-only across sea-orm / sqlx / reqwest, so the
# binary needs only ca-certificates at runtime; no libssl link.
RUN if [ -n "$CARGO_FEATURES" ]; then \
        cargo build --release --features "$CARGO_FEATURES"; \
    else \
        cargo build --release; \
    fi

# Runtime stage. debian:trixie-slim is the minimal Debian 13 base
# (~75MB before our additions); we install ca-certificates for
# outbound TLS and create a dedicated non-root user.
FROM debian:trixie-slim

WORKDIR /app

# Install ca-certificates and create the non-root runtime user in one
# layer; clean apt caches so the final image stays small.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system appuser \
 && useradd --system --gid appuser --no-create-home \
            --shell /usr/sbin/nologin appuser

# Copy artifacts with explicit ownership + executable bits so we never
# need a runtime chown / chmod. The binary and entrypoint must be
# executable; the SPA assets are read-only.
COPY --from=builder --chown=appuser:appuser --chmod=0755 \
     /app/target/release/riposte-social ./riposte-social
COPY --from=frontend-builder --chown=appuser:appuser \
     /app/admin-assets ./admin-assets
COPY --from=frontend-builder --chown=appuser:appuser \
     /app/social-assets ./social-assets
COPY --chown=appuser:appuser --chmod=0755 entrypoint.sh ./entrypoint.sh

USER appuser

EXPOSE 3000

ENTRYPOINT ["./entrypoint.sh"]
