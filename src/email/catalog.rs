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

use std::sync::OnceLock;

use i18n_md_email_templates::Catalog;

// The `email` i18next namespace, embedded at compile time. These are the same
// catalogs the frontend serves; the build is only needed for frontend serving,
// not for sending email. Keep the locale set in sync with
// `crate::profile::locale::SUPPORTED_LOCALES`.
const EN: &str = include_str!("../../social-frontend/public/locales/en/email.json");
const ES: &str = include_str!("../../social-frontend/public/locales/es/email.json");
const FR: &str = include_str!("../../social-frontend/public/locales/fr/email.json");
const ZH: &str = include_str!("../../social-frontend/public/locales/zh/email.json");
const DE: &str = include_str!("../../social-frontend/public/locales/de/email.json");

/// The process-wide email string catalog, parsed once. Fallback locale is "en".
pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let locales = [("en", EN), ("es", ES), ("fr", FR), ("zh", ZH), ("de", DE)]
            .into_iter()
            .map(|(code, raw)| {
                let value = serde_json::from_str(raw)
                    .unwrap_or_else(|e| panic!("invalid email catalog for {code}: {e}"));
                (code.to_string(), value)
            })
            .collect();
        Catalog::new(locales, "en")
    })
}
