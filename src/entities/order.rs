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
//! Generic customer order intake for the business module. The row carries a
//! customer identity, human-readable `title`/`summary` for notifications and
//! admin display, optional `estimate_total` and `deposit` fields, a `total`,
//! a `kind` tag naming the source form, and an opaque `details` JSON string 
//! holding form-specific fields. The schema is intentionally independent of
//! any particular front-end: new intake forms vary only `kind` and the
//! contents of `details`.
//!
//! `customer_name`, `customer_phone`, and `customer_email` are stored as
//! ciphertext produced by [`crate::crypto::encrypt_value`] (AES-256-GCM via
//! `SECURE_VALUES_KEY`); they are never persisted or logged in plaintext.
//! `details` must not contain personally identifying information.
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "orders")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Identifier of the intake form this order came from.
    pub kind: String,
    /// Short human-readable label for subject lines and list rows.
    pub title: String,
    /// Full human-readable order text shown in notifications and admin detail.
    pub summary: String,
    /// AES-256-GCM ciphertext.
    pub customer_name: String,
    /// AES-256-GCM ciphertext.
    pub customer_phone: String,
    /// AES-256-GCM ciphertext; absent when a form does not collect an email.
    pub customer_email: Option<String>,
    /// Quoted estimate shown to the customer, if any. Not an authoritative price.
    pub estimate_total: Option<f64>,
    /// Actual cost shown to the customer. The authoritative price.
    pub total: Option<f64>,
    /// Deposit quoted to customer, if any.
    pub deposit: Option<f64>,
    /// Form-specific fields as a JSON string. Contains no PII.
    pub details: String,
    pub status: String,
    /// Phone verification outcome at submit; `None` when verification was off.
    pub phone_verified: Option<bool>,
    /// Phone line type from the verification provider (mobile/landline/voip).
    pub phone_line_type: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}