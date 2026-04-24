#!/bin/bash
set -e

echo "🔧 Initializing riposte-social database..."

# Create extensions if needed
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    -- Enable UUID extension for UUID generation
    CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

    -- Enable pg_stat_statements for query performance monitoring
    CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

    -- Grant necessary permissions
    GRANT ALL PRIVILEGES ON DATABASE $POSTGRES_DB TO $POSTGRES_USER;

    -- Create schema for application if it doesn't exist
    CREATE SCHEMA IF NOT EXISTS public;
    GRANT ALL ON SCHEMA public TO $POSTGRES_USER;
EOSQL

echo "✅ Database initialization complete!"
echo "📊 Database: $POSTGRES_DB"
echo "👤 User: $POSTGRES_USER"
