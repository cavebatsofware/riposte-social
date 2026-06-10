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
//! Phone verification via Twilio Lookup (business module), with an HMAC-keyed
//! dedupe cache so a repeat number skips the paid provider call.
use crate::entities::{phone_verification, PhoneVerification};
use anyhow::Result;
use sea_orm::{
    prelude::DateTimeWithTimeZone, sea_query::OnConflict, ColumnTrait, DatabaseConnection,
    EntityTrait, QueryFilter, Set,
};
use uuid::Uuid;

/// Re-verify a cached number after this many days (ownership / line data drifts).
const TTL_DAYS: i64 = 90;

pub struct PhoneCheck {
    pub valid: bool,
    pub line_type: Option<String>,
}

/// Normalize a free-form phone to E.164, defaulting to NANP (US/Canada): a
/// 10-digit number becomes +1XXXXXXXXXX, an 11-digit starting with 1 keeps its
/// country code, and 8-15 digits are assumed to already carry a country code.
/// Returns `None` if it can't plausibly be a phone number.
pub fn normalize_e164(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        10 => Some(format!("+1{digits}")),
        11 if digits.starts_with('1') => Some(format!("+{digits}")),
        8..=15 => Some(format!("+{digits}")),
        _ => None,
    }
}

/// A cached verification for this HMAC that is still within the TTL, if any.
pub async fn cached_fresh(
    db: &DatabaseConnection,
    hmac: &str,
) -> Result<Option<phone_verification::Model>> {
    let cutoff: DateTimeWithTimeZone =
        (chrono::Utc::now() - chrono::Duration::days(TTL_DAYS)).into();
    let row = PhoneVerification::find()
        .filter(phone_verification::Column::PhoneHmac.eq(hmac))
        .filter(phone_verification::Column::VerifiedAt.gte(cutoff))
        .one(db)
        .await?;
    Ok(row)
}

/// Upsert the verification result for this HMAC. A single INSERT ... ON CONFLICT
/// keeps concurrent submissions of the same number from racing into a duplicate
/// insert on the unique `phone_hmac`.
pub async fn cache_result(db: &DatabaseConnection, hmac: &str, check: &PhoneCheck) -> Result<()> {
    let now = chrono::Utc::now();
    PhoneVerification::insert(phone_verification::ActiveModel {
        id: Set(Uuid::new_v4()),
        phone_hmac: Set(hmac.to_string()),
        valid: Set(check.valid),
        line_type: Set(check.line_type.clone()),
        verified_at: Set(now.into()),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
    })
    .on_conflict(
        OnConflict::column(phone_verification::Column::PhoneHmac)
            .update_columns([
                phone_verification::Column::Valid,
                phone_verification::Column::LineType,
                phone_verification::Column::VerifiedAt,
                phone_verification::Column::UpdatedAt,
            ])
            .to_owned(),
    )
    .exec(db)
    .await?;
    Ok(())
}

/// Call Twilio Lookup v2 for a single E.164 number, returning whether it's valid
/// and its line type. Any transport/parse error is an `Err` so the caller can
/// fail open (a Twilio outage must not drop a real order).
pub async fn verify_phone(
    http: &reqwest::Client,
    account_sid: &str,
    auth_token: &str,
    e164: &str,
) -> Result<PhoneCheck> {
    let url =
        format!("https://lookups.twilio.com/v2/PhoneNumbers/{e164}?Fields=line_type_intelligence");
    let resp = http
        .get(&url)
        .basic_auth(account_sid, Some(auth_token))
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        anyhow::bail!("Twilio lookup HTTP {}: {}", status, body);
    }
    let v: serde_json::Value = serde_json::from_str(&body)?;
    let valid = v
        .get("valid")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let line_type = v
        .get("line_type_intelligence")
        .and_then(|lt| lt.get("type"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    Ok(PhoneCheck { valid, line_type })
}
