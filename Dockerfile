# Frontend build stage. Builds both the admin SPA (admin-assets/) and the
# social SPA (social-assets/). The two share package.json + node_modules
# and are produced from one `npm run build` call (admin then social).
FROM node:25.7-alpine3.23 AS frontend-builder

WORKDIR /app

# Copy package files and install dependencies. The project carries a
# few intentional peer-dep mismatches (eslint 10 vs jsx-a11y peer cap
# at 9, etc.) that legacy-peer-deps resolves. Same flag the local
# install uses.
COPY package*.json ./
RUN npm ci --legacy-peer-deps

# Copy vite configs + both frontend source trees.
COPY vite.config.js ./
COPY vite.social.config.js ./
COPY admin-frontend ./admin-frontend
COPY social-frontend ./social-frontend

# Build both frontends. `npm run build` chains build:admin and build:social
# per package.json scripts and emits to admin-assets/ and social-assets/.
RUN npm run build

# Rust build stage
FROM rust:alpine3.23 AS builder

WORKDIR /app

# Install build dependencies for Alpine/musl compatibility
RUN apk update
RUN apk add --no-cache musl-dev

# Copy manifest files
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the release binary
RUN cargo build --release

# Runtime stage
FROM alpine:3.23.3

WORKDIR /app

# Install minimal runtime dependencies
RUN apk add --no-cache ca-certificates

# Copy the binary from builder stage
COPY --from=builder /app/target/release/riposte-social ./riposte-social

# Copy built admin and social frontends from frontend-builder stage.
COPY --from=frontend-builder /app/admin-assets ./admin-assets
COPY --from=frontend-builder /app/social-assets ./social-assets

# Static template files (assets/, landing.html) are no longer served by
# the SPA root; the social-frontend handles `/`. Drop the COPY lines that
# referenced them; revisit later if a bare landing page becomes useful.
COPY entrypoint.sh ./entrypoint.sh

# Create non-root user (Alpine style)
RUN adduser -D -s /bin/false appuser && \
    chown -R appuser:appuser /app && \
    chmod +x ./entrypoint.sh

USER appuser

EXPOSE 3000

# Start the application
ENTRYPOINT ["./entrypoint.sh"]
