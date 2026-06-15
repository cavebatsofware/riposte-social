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
use riposte_design_system::build_catalog;

/// Default email copy, owned by this application (the design system owns the
/// rendering mechanism, layout, and CSS, not the content). Embedded from this
/// module's `catalogs/` directory so the Rust builder copies them with `src/`.
/// Keep the locale set in sync with `crate::profile::locale::SUPPORTED_LOCALES`.
const DEFAULTS: [(&str, &str); 5] = [
    ("en", include_str!("catalogs/en.json")),
    ("es", include_str!("catalogs/es.json")),
    ("fr", include_str!("catalogs/fr.json")),
    ("zh", include_str!("catalogs/zh.json")),
    ("de", include_str!("catalogs/de.json")),
];

const FALLBACK_LOCALE: &str = "en";

/// The process-wide email string catalog, built once. Fallback locale is `en`.
///
/// Today this is the application defaults verbatim. Per-deployment operator
/// overrides (DB settings, especially the business order emails) will be
/// deep-merged over these via `build_catalog`; the merge seam is already in
/// place with an empty override set.
///
/// Panics on first use if any embedded default is not valid JSON; a build-time
/// content error caught by tests and never reachable in a shipped binary.
pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let defaults = DEFAULTS.into_iter().map(|(code, raw)| {
            let value = serde_json::from_str(raw)
                .unwrap_or_else(|e| panic!("invalid default email catalog for {code}: {e}"));
            (code.to_string(), value)
        });
        let overrides = Vec::<(String, serde_json::Value)>::new();
        build_catalog(defaults, overrides, FALLBACK_LOCALE)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::BTreeSet;

    fn collect_keys(value: &Value, prefix: &str, out: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    let path = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    collect_keys(v, &path, out);
                }
            }
            _ => {
                out.insert(prefix.to_string());
            }
        }
    }

    fn keys_for(raw: &str) -> BTreeSet<String> {
        let value: Value = serde_json::from_str(raw).expect("default catalog is valid JSON");
        let mut out = BTreeSet::new();
        collect_keys(&value, "", &mut out);
        out
    }

    #[test]
    fn defaults_parse_and_build() {
        // Same path as production: parse every default and build the catalog.
        let _ = catalog();
    }

    /// Cross-locale key parity for the default catalogs. Previously this rode on
    /// `check:i18n` scanning the frontend locales dir; now the catalogs live
    /// here, so the check lives here too.
    #[test]
    fn locales_have_the_same_keys_as_en() {
        assert_eq!(DEFAULTS[0].0, FALLBACK_LOCALE);
        let reference = keys_for(DEFAULTS[0].1);
        for (code, raw) in DEFAULTS.into_iter().skip(1) {
            let keys = keys_for(raw);
            let missing: Vec<_> = reference.difference(&keys).collect();
            let extra: Vec<_> = keys.difference(&reference).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "email catalog '{code}' diverges from '{FALLBACK_LOCALE}': missing={missing:?} extra={extra:?}",
            );
        }
    }
}
