#!/bin/bash
# wait-for-db.sh - Wait for PostgreSQL to be ready before starting the application

set -e

host="${DATABASE_HOST:-localhost}"
port="${DATABASE_PORT:-5432}"
user="${POSTGRES_USER:-riposte_social_user}"
db="${POSTGRES_DB:-riposte_social}"
max_attempts="${DB_WAIT_MAX_ATTEMPTS:-30}"
attempt=1

# Mirror the app's POSTGRES_PASSWORD_FILE convention so the same Docker /
# systemd secret feeds this wait check when the password is delivered as a
# file rather than an env var.
if [ -z "${POSTGRES_PASSWORD:-}" ] && [ -n "${POSTGRES_PASSWORD_FILE:-}" ]; then
    POSTGRES_PASSWORD="$(cat "$POSTGRES_PASSWORD_FILE")"
    # Command substitution drops trailing newlines but leaves a CRLF '\r';
    # strip one to match the app resolver's '\r\n' trimming in src/secret.rs.
    POSTGRES_PASSWORD="${POSTGRES_PASSWORD%$'\r'}"
fi

echo "⏳ Waiting for PostgreSQL at $host:$port..."

until PGPASSWORD="$POSTGRES_PASSWORD" psql -h "$host" -p "$port" -U "$user" -d "$db" -c '\q' 2>/dev/null; do
    if [ $attempt -eq $max_attempts ]; then
        echo "❌ PostgreSQL did not become ready in time"
        exit 1
    fi

    echo "⏳ Attempt $attempt/$max_attempts: PostgreSQL is unavailable - sleeping"
    attempt=$((attempt + 1))
    sleep 2
done

echo "✅ PostgreSQL is ready at $host:$port!"
