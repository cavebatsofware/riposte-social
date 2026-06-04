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
use sea_orm::*;
use std::env;
use std::time::Duration;
use tracing::log::LevelFilter;
use url::Url;

pub async fn establish_connection() -> Result<DatabaseConnection, DbErr> {
    establish_connection_with_retry(5, Duration::from_secs(5)).await
}

/// Establish database connection with retry logic for production resilience
pub async fn establish_connection_with_retry(
    max_retries: u32,
    retry_delay: Duration,
) -> Result<DatabaseConnection, DbErr> {
    let database_url = database_url();

    let mut opt = ConnectOptions::new(database_url.clone());
    opt.max_connections(20)
        .min_connections(5)
        .connect_timeout(Duration::from_secs(5))
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(8))
        .max_lifetime(Duration::from_secs(1800))
        .sqlx_logging(true) // Enable query logging in debug builds
        .sqlx_logging_level(LevelFilter::Debug);

    let mut attempts = 0;
    let mut last_error = None;

    while attempts < max_retries {
        attempts += 1;

        match Database::connect(opt.clone()).await {
            Ok(conn) => {
                tracing::info!(
                    "✅ Database connected successfully on attempt {}/{}",
                    attempts,
                    max_retries
                );

                // Verify connection with a ping
                if let Err(e) = conn.ping().await {
                    tracing::warn!("Database ping failed after connection: {}", e);
                    last_error = Some(e);

                    if attempts < max_retries {
                        tracing::info!("⏳ Retrying in {:?}...", retry_delay);
                        tokio::time::sleep(retry_delay).await;
                        continue;
                    }
                } else {
                    return Ok(conn);
                }
            }
            Err(e) => {
                tracing::warn!(
                    "❌ Database connection attempt {}/{} failed: {}",
                    attempts,
                    max_retries,
                    e
                );
                last_error = Some(e);

                if attempts < max_retries {
                    tracing::info!("⏳ Retrying in {:?}...", retry_delay);
                    tokio::time::sleep(retry_delay).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        DbErr::Conn(sea_orm::RuntimeErr::Internal(
            "Failed to connect to database after maximum retries".to_string(),
        ))
    }))
}

/// Resolve the Postgres connection string.
///
/// A literal `DATABASE_URL` (set by local dev, CI, and docker-compose) is
/// used verbatim, preserving any operator-supplied query params such as
/// `sslmode`. When it is absent, the URL is assembled from the non-secret
/// connection parts plus the password resolved through the
/// [`crate::secret`] file chain (the `POSTGRES_PASSWORD_FILE` convention),
/// so one Docker/systemd secret feeds both the postgres container and the
/// app. `url::Url` percent-encodes the username and password.
pub fn database_url() -> String {
    if let Ok(url) = env::var("DATABASE_URL") {
        if !url.is_empty() {
            return url;
        }
    }

    let host = env::var("DATABASE_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = env::var("DATABASE_PORT").unwrap_or_else(|_| "5432".to_string());
    let user = env::var("POSTGRES_USER").unwrap_or_else(|_| "riposte_social_user".to_string());
    let db = env::var("POSTGRES_DB").unwrap_or_else(|_| "riposte_social".to_string());
    let password = crate::secret::load("POSTGRES_PASSWORD").unwrap_or_else(|| {
        panic!(
            "Neither DATABASE_URL nor a POSTGRES_PASSWORD secret is set. \
            Provide DATABASE_URL, or POSTGRES_PASSWORD via POSTGRES_PASSWORD_FILE, \
            $CREDENTIALS_DIRECTORY/postgres_password, /run/secrets/postgres_password, \
            or the POSTGRES_PASSWORD environment variable."
        )
    });

    let mut url = Url::parse(&format!("postgresql://{host}:{port}/{db}"))
        .expect("failed to build database URL from connection parts");
    url.set_username(&user)
        .expect("database host has no authority component for the username");
    url.set_password(Some(password.as_str()))
        .expect("database host has no authority component for the password");

    if let Ok(sslmode) = env::var("DATABASE_SSLMODE") {
        if !sslmode.is_empty() {
            url.query_pairs_mut().append_pair("sslmode", &sslmode);
        }
    }

    url.to_string()
}

/// Close database connection gracefully
pub async fn close_connection(db: DatabaseConnection) -> Result<(), DbErr> {
    db.close().await
}

/// Run pending migrations
pub async fn run_migrations(db: &DatabaseConnection) -> Result<(), DbErr> {
    use crate::migration::{Migrator, MigratorTrait};

    // Apply all pending migrations
    Migrator::up(db, None).await
}
