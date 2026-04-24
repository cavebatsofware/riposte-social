/*  This file is part of riposte-social
 *  Copyright (C) 2026 Grant DeFayette
 *
 *  riposte-social is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, version 3 of the License (GPL-3.0-only).
 *
 *  riposte-social is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with riposte-social.  If not, see <https://www.gnu.org/licenses/gpl-3.0.html>.
 */
#[cfg(test)]
pub mod auth_tests;
#[cfg(test)]
pub mod database_tests;

use crate::migration::{Migrator, MigratorTrait};
use sea_orm::DatabaseConnection;

/// Create a test email address using the configured SITE_DOMAIN.
pub fn test_email(username: &str) -> String {
    dotenvy::dotenv().ok();
    let domain = std::env::var("SITE_DOMAIN").expect("SITE_DOMAIN must be set");
    format!("{}@{}", username, domain)
}

/// Bridge an sqlx PgPool (provided by `#[sqlx::test]`) to a SeaORM
/// `DatabaseConnection` and run all pending migrations.
pub async fn test_db_from_pool(pool: sqlx::PgPool) -> DatabaseConnection {
    let db = sea_orm::SqlxPostgresConnector::from_sqlx_postgres_pool(pool);
    Migrator::up(&db, None)
        .await
        .expect("Failed to run migrations");
    db
}
