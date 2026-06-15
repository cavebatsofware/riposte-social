# Frontend build stage. Builds both the admin SPA (admin-assets/) and the
# social SPA (social-assets/). The two share package.json + node_modules
# and are produced from one `bun run build` call (admin then social).
FROM oven/bun:1.3.14-slim AS frontend-builder

WORKDIR /app

# Copy lockfile + manifest and install dependencies.
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile

# Copy build scripts, tsconfig, and both frontend source trees.
COPY scripts ./scripts
COPY tsconfig.json ./
COPY admin-frontend ./admin-frontend
COPY social-frontend ./social-frontend

# SITE_DOMAIN is baked into both SPAs as VITE_SITE_DOMAIN at build time (e.g. the
# admin register page's "@<domain>" email-tier check). The Makefile passes it via
# --build-arg; declare the ARG and promote it to an env var so the build scripts'
# process.env.SITE_DOMAIN sees it. Without this the SPAs ship an empty domain.
ARG SITE_DOMAIN
ENV SITE_DOMAIN=$SITE_DOMAIN

# Build both frontends. `bun run build` chains build:admin and build:social
# per package.json scripts and emits to admin-assets/ and social-assets/.
RUN bun run build

# Rust build stage. Debian 13 (trixie) image so the runtime stage can
# share the same glibc; default target is x86_64-unknown-linux-gnu.
FROM rust:1.95-slim-trixie AS builder

WORKDIR /app

# Optional cargo features. Empty in production builds (no dev / test
# code compiled in). The test compose sets this to `e2e_testing` so
# the `seed-test-admin` and `hash-password` subcommands and the
# `DEV_MODE` runtime overrides are available in the test container.
ARG CARGO_FEATURES=""

# Copy manifest + source. The email catalogs and layout the binary embeds now
# come from the riposte-design-system crate (a git dependency cargo fetches),
# so this stage no longer needs any social-frontend files.
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
# Pinned by digest so base-image updates are deliberate (bump when the CVE
# audit calls for it); the tag stays for readability. Digest resolved
# 2026-06-09 for debian:trixie-slim.
FROM debian:trixie-slim@sha256:b6e2a152f22a40ff69d92cb397223c906017e1391a73c952b588e51af8883bf8

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
# Business storefront static export. Staged into ./shop-assets by the build
# tooling (see `make build-business`); empty (just .gitkeep) for non-business
# images. Served at the shop server's root (APP_ROLE=shop/both) by a binary
# built with --features business; see build_shop_app in src/main.rs.
COPY --chown=appuser:appuser shop-assets ./shop-assets
COPY --chown=appuser:appuser --chmod=0755 entrypoint.sh ./entrypoint.sh

USER appuser

EXPOSE 3000

ENTRYPOINT ["./entrypoint.sh"]
