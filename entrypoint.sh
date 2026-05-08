#!/bin/sh
set -e

# If MIGRATE_DB is true, run migrations and then start the app.
# The app is designed to exit after migrations, so we run it as a separate step.
if [ "$MIGRATE_DB" = "true" ]; then
  echo "Running database migrations..."
  /app/riposte-social migrate
  echo "Migrations complete."
fi

# If SEED_TEST_ADMIN is true, idempotently provision a test admin with a
# known email + password after migrations. Used by docker-compose.test.yml
# so Cypress and humans poking the test container can sign in against a
# deterministic fixture. The subcommand itself also requires this env
# var as a defense-in-depth gate.
if [ "$SEED_TEST_ADMIN" = "true" ]; then
  : "${TEST_ADMIN_EMAIL:?TEST_ADMIN_EMAIL must be set when SEED_TEST_ADMIN=true}"
  : "${TEST_ADMIN_PASSWORD:?TEST_ADMIN_PASSWORD must be set when SEED_TEST_ADMIN=true}"
  echo "Seeding test admin..."
  /app/riposte-social seed-test-admin "$TEST_ADMIN_EMAIL" "$TEST_ADMIN_PASSWORD"
  echo "Seed complete."
fi

# Honor any RUST_LOG passed in via the environment (e.g. the test
# compose's TEST_APP_LOG_LEVEL); fall back to `warn` only when no
# value is set.
export RUST_LOG="${RUST_LOG:-warn}"

# Start the main application
echo "Starting application..."
exec /app/riposte-social
