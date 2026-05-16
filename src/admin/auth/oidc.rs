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
//! OIDC drift validators. Each check returns Err with a human-readable
//! message; the caller surfaces these as `AuthError` to the OIDC handler.

use crate::entities::user;
use anyhow::Result;

/// Reject login when a row has not been activated via invite acceptance.
pub(crate) fn ensure_activated(row: &user::Model) -> Result<()> {
    if row.activated_at.is_none() {
        anyhow::bail!("Account not yet activated. Please use your invite link to set it up.");
    }
    Ok(())
}

/// Reject login when the IdP-claimed tier disagrees with the DB-authoritative
/// role. Drift in either direction is a security event; admins reconcile
/// manually, never both at once.
pub(crate) fn ensure_role_match(idp_tier: &str, db_role: &str) -> Result<()> {
    if idp_tier != db_role {
        anyhow::bail!(
            "OIDC role does not match account role (idp={}, db={})",
            idp_tier,
            db_role
        );
    }
    Ok(())
}

/// Reject login when the IdP-attested email differs from the DB email.
/// Email changes in the IdP require an admin to re-align the DB row first.
pub(crate) fn ensure_email_match(idp_email: &str, db_email: &str) -> Result<()> {
    if idp_email != db_email {
        anyhow::bail!("OIDC email does not match account email");
    }
    Ok(())
}
