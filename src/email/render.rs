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

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use i18n_md_email_templates::{inline_css, render, Cta, EmailTemplate};

use super::catalog::catalog;

/// The Riposte-branded shared email shell (`{{content}}` / `{{footer}}` slots
/// and the single inline-safe `<style>` block all emails share). Owned by the
/// design system so the branded layout has one home.
const LAYOUT: &str = riposte_design_system::EMAIL_LAYOUT;

/// Subject, HTML body, and plaintext body for one outbound email.
pub struct EmailParts {
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

/// Look up a single localized string by dotted key (e.g. a role label or the
/// password-change source phrase), with the catalog's own fallback locale.
pub fn tr(locale: &str, key: &str) -> Option<String> {
    catalog().get(locale, key)
}

/// Render one email from the `email` catalog.
///
/// `key` is the per-email object name (e.g. `"invite"`); its `subject`, `heading`,
/// `body`, optional `cta`, and optional `footer` strings are pulled for `locale`. The
/// recipient's heading becomes the layout title, and `{{token}}` placeholders in any of
/// those strings are filled from `extra_vars`. `cta_url` supplies the button target for
/// emails that have a `cta` string.
pub fn build(
    locale: &str,
    key: &str,
    cta_url: Option<&str>,
    extra_vars: &[(&'static str, String)],
) -> Result<EmailParts> {
    let cat = catalog();
    let get = |field: &str| cat.get(locale, &format!("{key}.{field}"));

    let subject = get("subject").ok_or_else(|| anyhow!("missing email string {key}.subject"))?;
    let body = get("body").ok_or_else(|| anyhow!("missing email string {key}.body"))?;
    let heading = get("heading").unwrap_or_default();
    let footer = get("footer");
    let cta_label = get("cta");

    let mut vars: BTreeMap<&str, String> = BTreeMap::new();
    vars.insert("lang", locale.to_string());
    // The recipient heading drives the layout's title/header. It uses a
    // dedicated `heading` token so an email whose `extra_vars` carries a
    // content `title` (e.g. the order emails) cannot clobber the header.
    vars.insert("heading", heading);
    for (k, v) in extra_vars {
        vars.insert(k, v.clone());
    }

    let cta = match (cta_label, cta_url) {
        (Some(label), Some(url)) => Some(Cta {
            label,
            url: url.to_string(),
        }),
        _ => None,
    };

    let tmpl = EmailTemplate {
        layout: LAYOUT,
        subject: &subject,
        body_md: &body,
        cta,
        footer_md: footer.as_deref(),
    };
    let rendered = render(&tmpl, &vars);

    // Inline the layout's CSS so the styling survives email clients that ignore
    // embedded <style>. Fall back to the un-inlined HTML if inlining ever fails,
    // logging so a regression surfaces instead of silently degrading rendering.
    let html_body = match inline_css(&rendered.html_body) {
        Ok(inlined) => inlined,
        Err(err) => {
            tracing::warn!("CSS inlining failed for email '{key}', sending un-inlined HTML: {err}");
            rendered.html_body
        }
    };

    Ok(EmailParts {
        subject: rendered.subject,
        html_body,
        text_body: rendered.text_body,
    })
}
