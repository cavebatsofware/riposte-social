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
// Library interface for riposte-social
// This allows tests to access internal modules

pub mod admin;
pub mod albums;
pub mod app;
pub mod articles;
pub mod auth;
pub mod categories;
pub mod contact;
pub mod crypto;
pub mod database;
pub mod docx;
pub mod email;
pub mod engagement;
pub mod entities;
pub mod errors;
pub mod follows;
pub mod imports;
pub mod invites;
pub mod metrics;
pub mod middleware;
pub mod migration;
pub mod posts;
pub mod profile;
pub mod s3;
pub mod settings;
pub mod subscriptions;
pub mod visibility;

#[cfg(any(test, feature = "e2e_testing"))]
pub mod tests;
