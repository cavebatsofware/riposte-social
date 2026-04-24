# Frontend build stage (admin only - public frontend is pre-built by Makefile)
FROM node:25.7-alpine3.23 AS frontend-builder

WORKDIR /app

# Copy package files and install dependencies
COPY package*.json ./
RUN npm ci

# Copy vite config and admin frontend source
COPY vite.config.js ./
COPY admin-frontend ./admin-frontend

# Build admin frontend
RUN npm run build

# Rust build stage
FROM rust:alpine3.23.3 AS builder

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

# Copy built admin frontend from frontend-builder stage
COPY --from=frontend-builder /app/admin-assets ./admin-assets

# Copy static assets
COPY assets ./assets
COPY landing.html ./landing.html
COPY entrypoint.sh ./entrypoint.sh

# Create non-root user (Alpine style)
RUN adduser -D -s /bin/false appuser && \
    chown -R appuser:appuser /app && \
    chmod +x ./entrypoint.sh

USER appuser

EXPOSE 3000

# Start the application
ENTRYPOINT ["./entrypoint.sh"]
