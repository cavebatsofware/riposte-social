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
use crate::entities::{access_log, AccessLog};
use crate::tests::test_db_from_pool;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use uuid::Uuid;

#[sqlx::test(migrations = false)]
async fn test_access_log_create_and_read(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;

    let access_log = access_log::ActiveModel {
        id: Set(Uuid::new_v4()),
        access_code: Set("hashed-code".to_string()),
        ip_address: Set(Some("hashed-ip".to_string())),
        user_agent: Set(Some("test-user-agent".to_string())),
        tokens: Set(Some(10.0)),
        last_access_time: Set(Some(Utc::now().into())),
        last_delta_access: Set(Some(1000)), // 1 second
        action: Set("view".to_string()),
        success: Set(true),
        admin_user_id: Set(None),
        admin_user_email: Set(None),
        created_at: Set(Utc::now().into()),
    };

    // Insert the record
    let inserted = access_log.insert(&db).await;
    assert!(inserted.is_ok(), "Should be able to insert access log");

    let saved_log = inserted.unwrap();

    // Verify the record was saved correctly
    assert_eq!(saved_log.access_code, "hashed-code");
    assert_eq!(saved_log.action, "view");
    assert!(saved_log.success);
    assert_eq!(saved_log.tokens, Some(10.0));

    // Find the record by ID
    let found_log = AccessLog::find_by_id(saved_log.id).one(&db).await;
    assert!(found_log.is_ok(), "Should be able to find access log by ID");

    let found = found_log.unwrap();
    assert!(found.is_some(), "Record should exist");

    let found_record = found.unwrap();
    assert_eq!(found_record.id, saved_log.id);
    assert_eq!(found_record.access_code, saved_log.access_code);
}

#[sqlx::test(migrations = false)]
async fn test_access_log_query_by_ip(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;

    let test_ip_hash = "test-ip-hash";

    // Create multiple access logs for the same IP
    for i in 1..=3 {
        let access_log = access_log::ActiveModel {
            id: Set(Uuid::new_v4()),
            access_code: Set(format!("code-{}", i)),
            ip_address: Set(Some(test_ip_hash.to_string())),
            user_agent: Set(Some("test-user-agent".to_string())),

            tokens: Set(Some(i as f64)),
            last_access_time: Set(Some(Utc::now().into())),
            last_delta_access: Set(Some(i as i64 * 1000)),
            action: Set("view".to_string()),
            success: Set(true),
            admin_user_id: Set(None),
            admin_user_email: Set(None),
            created_at: Set(Utc::now().into()),
        };

        access_log.insert(&db).await.expect("Should insert");
    }

    // Query by IP address
    use sea_orm::{ColumnTrait, QueryFilter};

    let logs_for_ip = AccessLog::find()
        .filter(access_log::Column::IpAddress.eq(test_ip_hash))
        .all(&db)
        .await;

    assert!(logs_for_ip.is_ok(), "Should be able to query by IP");

    let logs = logs_for_ip.unwrap();
    assert_eq!(logs.len(), 3, "Should find 3 logs for the IP");

    // Verify all logs have the correct IP hash
    for log in logs {
        assert_eq!(log.ip_address, Some(test_ip_hash.to_string()));
        assert!(log.success);
    }
}

#[sqlx::test(migrations = false)]
async fn test_access_log_failed_attempts(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;

    // Create some failed attempts
    for i in 1..=2 {
        let access_log = access_log::ActiveModel {
            id: Set(Uuid::new_v4()),
            access_code: Set("invalid-code".to_string()),
            ip_address: Set(Some("attacker-ip-hash".to_string())),
            user_agent: Set(Some("suspicious-agent".to_string())),

            tokens: Set(Some(i as f64)),
            last_access_time: Set(Some(Utc::now().into())),
            last_delta_access: Set(Some(500)), // Quick succession
            action: Set("view".to_string()),
            success: Set(false), // Failed attempt
            admin_user_id: Set(None),
            admin_user_email: Set(None),
            created_at: Set(Utc::now().into()),
        };

        access_log
            .insert(&db)
            .await
            .expect("Should insert failed attempt");
    }

    // Query failed attempts
    use sea_orm::{ColumnTrait, QueryFilter};

    let failed_attempts = AccessLog::find()
        .filter(access_log::Column::Success.eq(false))
        .all(&db)
        .await;

    assert!(
        failed_attempts.is_ok(),
        "Should be able to query failed attempts"
    );

    let failed = failed_attempts.unwrap();
    assert_eq!(failed.len(), 2, "Should find 2 failed attempts");

    for attempt in failed {
        assert!(!attempt.success);
        assert_eq!(attempt.access_code, "invalid-code");
    }
}

#[sqlx::test(migrations = false)]
async fn test_access_log_count_operations(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;

    let test_ip_hash = "count-test-ip";

    // Create several logs
    for i in 1..=5 {
        let access_log = access_log::ActiveModel {
            id: Set(Uuid::new_v4()),
            access_code: Set("test-code".to_string()),
            ip_address: Set(Some(test_ip_hash.to_string())),
            user_agent: Set(Some("test-agent".to_string())),

            tokens: Set(Some(i as f64)),
            last_access_time: Set(Some(Utc::now().into())),
            last_delta_access: Set(Some(1000)),
            action: Set("view".to_string()),
            success: Set(true),
            admin_user_id: Set(None),
            admin_user_email: Set(None),
            created_at: Set(Utc::now().into()),
        };

        access_log.insert(&db).await.expect("Should insert");
    }

    // Count total logs for this IP
    use sea_orm::{ColumnTrait, PaginatorTrait, QueryFilter};

    let count = AccessLog::find()
        .filter(access_log::Column::IpAddress.eq(test_ip_hash))
        .filter(access_log::Column::Success.eq(true))
        .count(&db)
        .await;

    assert!(count.is_ok(), "Should be able to count logs");
    assert_eq!(count.unwrap(), 5, "Should count 5 successful logs");
}

#[sqlx::test(migrations = false)]
async fn test_access_log_ordering(pool: sqlx::PgPool) {
    let db = test_db_from_pool(pool).await;

    let test_ip_hash = "order-test-ip";

    // Create logs with different timestamps
    let base_time = Utc::now();
    for i in 1..=3 {
        let access_log = access_log::ActiveModel {
            id: Set(Uuid::new_v4()),
            access_code: Set("test-code".to_string()),
            ip_address: Set(Some(test_ip_hash.to_string())),
            user_agent: Set(Some("test-agent".to_string())),

            tokens: Set(Some(i as f64)),
            last_access_time: Set(Some(
                (base_time + chrono::Duration::seconds(i as i64)).into(),
            )),
            last_delta_access: Set(Some(i as i64 * 1000)),
            action: Set("view".to_string()),
            success: Set(true),
            admin_user_id: Set(None),
            admin_user_email: Set(None),
            created_at: Set((base_time + chrono::Duration::seconds(i as i64)).into()),
        };

        access_log.insert(&db).await.expect("Should insert");
    }

    // Query with ordering
    use sea_orm::{ColumnTrait, QueryFilter, QueryOrder};

    let ordered_logs = AccessLog::find()
        .filter(access_log::Column::IpAddress.eq(test_ip_hash))
        .order_by_desc(access_log::Column::CreatedAt)
        .all(&db)
        .await;

    assert!(ordered_logs.is_ok(), "Should be able to order logs");

    let logs = ordered_logs.unwrap();
    assert_eq!(logs.len(), 3, "Should find 3 logs");

    // Verify descending order (most recent first)
    assert!(logs[0].tokens > logs[1].tokens);
    assert!(logs[1].tokens > logs[2].tokens);
}
