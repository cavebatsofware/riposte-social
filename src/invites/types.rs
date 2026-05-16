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
use crate::entities::invite_code;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateInviteRequest {
    /// Optional name/email hint for the recipient. Surfaces in the admin list
    /// and in the splash to help the visitor confirm the invite is for them.
    pub email_hint: Option<String>,
    /// Lifetime in hours. Capped at `MAX_INVITE_LIFETIME_DAYS`; defaults to one week.
    pub expires_in_hours: Option<i64>,
}

/// Admin-facing view of an invite. The plaintext `code` is `Some` only on
/// the creation response (where the admin/auto-issuer needs to deliver it
/// out-of-band). Listing and revoke responses leave it `None` because the
/// DB only stores the hash and there is no way to recover the plaintext
/// after issuance.
#[derive(Serialize)]
pub struct InviteResponse {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub email_hint: Option<String>,
    pub created_by: Uuid,
    pub expires_at: String,
    pub created_at: String,
    pub used_at: Option<String>,
    pub used_by_user_id: Option<Uuid>,
    pub revoked_at: Option<String>,
    pub status: &'static str,
}

impl InviteResponse {
    /// Build a response that exposes the plaintext code. Used at issuance time.
    pub fn issued(m: invite_code::Model, plaintext: String) -> Self {
        let mut r = Self::from(m);
        r.code = Some(plaintext);
        r
    }
}

impl From<invite_code::Model> for InviteResponse {
    fn from(m: invite_code::Model) -> Self {
        let now = Utc::now();
        let status = if m.revoked_at.is_some() {
            "revoked"
        } else if m.used_at.is_some() {
            "used"
        } else if m.expires_at.with_timezone(&Utc) <= now {
            "expired"
        } else {
            "active"
        };
        Self {
            id: m.id,
            code: None,
            email_hint: m.email_hint,
            created_by: m.created_by,
            expires_at: m.expires_at.with_timezone(&Utc).to_rfc3339(),
            created_at: m.created_at.with_timezone(&Utc).to_rfc3339(),
            used_at: m.used_at.map(|t| t.with_timezone(&Utc).to_rfc3339()),
            used_by_user_id: m.used_by_user_id,
            revoked_at: m.revoked_at.map(|t| t.with_timezone(&Utc).to_rfc3339()),
            status,
        }
    }
}

#[derive(Serialize)]
pub struct CurrentInviteResponse {
    pub code: String,
    pub email_hint: Option<String>,
    pub expires_at: String,
}

#[derive(Deserialize)]
pub struct ConfirmInviteRequest {
    pub code: String,
}

#[derive(Deserialize)]
pub struct AcceptInvitePasswordRequest {
    pub code: String,
    pub email: String,
    pub new_password: String,
}

#[derive(Serialize)]
pub struct AcceptInvitePasswordResponse {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub message: String,
}
