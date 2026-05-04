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
//! Locale validation. Single source of truth for the set of UI locales
//! the social-frontend supports. The frontend mirror lives in
//! `social-frontend/src/i18n.js::SUPPORTED_LOCALES` — keep both in sync
//! when adding a new language. Mismatch surfaces as a 400 from
//! `PATCH /api/me/locale` if the client somehow sends an unsupported
//! code; the server is the authoritative gate.

/// BCP-47 base codes accepted by `PATCH /api/me/locale`. Region subtags
/// (`en-US`, `zh-CN`, etc.) are not stored — the frontend strips them
/// to base codes via i18next's `load: "languageOnly"` setting before
/// posting.
pub const SUPPORTED_LOCALES: &[&str] = &["en", "es", "fr", "zh", "de"];

pub fn is_supported(locale: &str) -> bool {
    SUPPORTED_LOCALES.contains(&locale)
}
