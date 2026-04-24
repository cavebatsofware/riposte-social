#!/bin/sh
set -e

# If MIGRATE_DB is true, run migrations and then start the app.
# The app is designed to exit after migrations, so we run it as a separate step.
if [ "$MIGRATE_DB" = "true" ]; then
  echo "Running database migrations..."
  /app/riposte-social migrate
  echo "Migrations complete."
fi

export RUST_LOG=warn

# Start the main application
echo "Starting application..."
exec /app/riposte-social
