use anyhow::Result;
use basic_axum_rate_limit::{ActionChecker, OnBlocked, SecurityContext};
use chrono::{Duration, Utc};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use std::net::IpAddr;
use uuid::Uuid;

use crate::entities::{access_log, AccessLog};

pub struct AccessLogEvent {
    pub ip: Option<IpAddr>,
    pub user_agent: Option<String>,
    pub access_code: String,
    pub action: String,
    pub success: bool,
    pub tokens: f64,
    pub admin_user_id: Option<Uuid>,
    pub admin_user_email: Option<String>,
}

#[derive(Clone)]
pub struct AppRateLimitCallbacks {
    db: DatabaseConnection,
    enable_logging: bool,
    log_successful_attempts: bool,
}

impl AppRateLimitCallbacks {
    pub fn new(
        db: DatabaseConnection,
        enable_logging: bool,
        log_successful_attempts: bool,
    ) -> Self {
        Self {
            db,
            enable_logging,
            log_successful_attempts,
        }
    }

    pub async fn log_access_attempt(&self, event: AccessLogEvent) -> Result<()> {
        if !self.enable_logging {
            return Ok(());
        }

        if event.success && !self.log_successful_attempts {
            return Ok(());
        }

        tracing::debug!(
            "Logging access: ip={:?} action={} code={} success={}",
            event.ip,
            event.action,
            event.access_code,
            event.success
        );

        let now = Utc::now();

        let model = access_log::ActiveModel {
            id: Set(Uuid::new_v4()),
            access_code: Set(event.access_code.clone()),
            ip_address: Set(event.ip.map(|ip| ip.to_string())),
            user_agent: Set(event.user_agent),
            tokens: Set(Some(event.tokens)),
            last_access_time: Set(None),
            last_delta_access: Set(None),
            action: Set(event.action),
            success: Set(event.success),
            admin_user_id: Set(event.admin_user_id),
            admin_user_email: Set(event.admin_user_email),
            created_at: Set(now.into()),
        };

        // Direct insert - with properly sized connection pool this is fine
        if let Err(e) = access_log::Entity::insert(model).exec(&self.db).await {
            tracing::error!(
                "Failed to insert access log for {:?} on {}: {}",
                event.ip,
                event.access_code,
                e
            );
        }

        Ok(())
    }

    pub async fn has_recent_contact_submission(&self, ip: IpAddr) -> Result<bool> {
        self.check_recent_action_internal(ip, "contact_form_submit", Duration::hours(24))
            .await
    }

    pub async fn has_recent_subscription(&self, ip: IpAddr) -> Result<bool> {
        self.check_recent_action_internal(ip, "subscribe_submit", Duration::hours(24))
            .await
    }

    async fn check_recent_action_internal(
        &self,
        ip: IpAddr,
        action: &str,
        within: Duration,
    ) -> Result<bool> {
        let ip_str = ip.to_string();
        let cutoff = Utc::now() - within;

        let recent_attempt = AccessLog::find()
            .filter(access_log::Column::IpAddress.eq(ip_str))
            .filter(access_log::Column::Action.eq(action))
            .filter(access_log::Column::CreatedAt.gte(cutoff))
            .one(&self.db)
            .await?;

        Ok(recent_attempt.is_some())
    }

    pub async fn cleanup_database_logs(&self, retention_days: i64) -> Result<()> {
        let cutoff = Utc::now() - Duration::days(retention_days);
        let cleanup_batch_size = 1_000u64;
        let mut total_deleted = 0u64;

        loop {
            let delete_result = AccessLog::delete_many()
                .filter(access_log::Column::CreatedAt.lt(cutoff))
                .exec(&self.db)
                .await?;

            let rows_deleted = delete_result.rows_affected;
            total_deleted += rows_deleted;

            if rows_deleted == 0 {
                break;
            }

            if rows_deleted < cleanup_batch_size {
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        if total_deleted > 0 {
            tracing::info!(
                "Cleaned up {} old access log entries from database (retention: {} days)",
                total_deleted,
                retention_days
            );
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl OnBlocked for AppRateLimitCallbacks {
    async fn on_blocked(&self, ip: &str, path: &str, _context: &SecurityContext) {
        let ip_addr = ip.parse::<IpAddr>().ok();
        let rate_limit_key = format!("{}:{}", ip, path);

        if let Err(e) = self
            .log_access_attempt(AccessLogEvent {
                ip: ip_addr,
                user_agent: Some(_context.user_agent.clone()),
                access_code: rate_limit_key,
                action: "rate_limited_blocked".to_string(),
                success: false,
                tokens: 0.0, // Already blocked, no tokens remaining
                admin_user_id: None,
                admin_user_email: None,
            })
            .await
        {
            // Log the error but don't propagate it - we don't want database issues
            // to break rate limiting functionality
            tracing::error!(
                "Failed to log rate limit block for {} on {}: {}",
                ip,
                path,
                e
            );
        }
    }
}

#[async_trait::async_trait]
impl ActionChecker for AppRateLimitCallbacks {
    async fn check_recent_action(
        &self,
        ip: &str,
        action: &str,
        within: std::time::Duration,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let ip_addr = ip
            .parse::<IpAddr>()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let chrono_duration = Duration::from_std(within)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        match self
            .check_recent_action_internal(ip_addr, action, chrono_duration)
            .await
        {
            Ok(result) => Ok(result),
            Err(e) => Err(format!("{}", e).into()),
        }
    }
}
