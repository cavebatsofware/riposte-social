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
//! Shared helpers for serving the social SPA shell (`social-assets/index.html`).

/// Substitute the shell's theme-default placeholders so the pre-paint
/// no-flash script resolves the site theme without a round-trip. Only the
/// quoted placeholder string literals are replaced; the same tokens also
/// appear unquoted as the `window.__RS_DEFAULT_*` globals the SPA reads,
/// and matching the surrounding quotes leaves those identifiers intact.
/// Empty (or rejected) values leave the design-system fallback in effect.
///
/// The substituted value lands inside a `<script>` string literal, so it
/// is passed through `theme_token`: theme identifiers are plain slugs, and
/// blanking anything else keeps an admin-set value from carrying a `"`,
/// `\`, or `</script>` that would break out of the literal (a `<script>`
/// context, where JSON-encoding alone would not neutralize `</script>`).
///
/// Both the social SPA handler (`serve_spa` in main.rs) and the Open Graph
/// shells (`og.rs`) call this so the token substitution can never drift
/// between the two serving paths.
pub fn inject_theme_defaults(html: String, colorway: &str, shade: &str) -> String {
    html.replace(
        "\"__RS_DEFAULT_COLORWAY__\"",
        &format!("\"{}\"", theme_token(colorway)),
    )
    .replace(
        "\"__RS_DEFAULT_SHADE__\"",
        &format!("\"{}\"", theme_token(shade)),
    )
}

/// Accept a theme identifier only if it is a plain slug
/// (`[A-Za-z0-9_-]`); otherwise return empty so the design-system default
/// applies. This is the guard that keeps the value safe to drop into the
/// shell's `<script>` string literal.
fn theme_token(value: &str) -> &str {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        value
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::inject_theme_defaults;

    #[test]
    fn substitutes_valid_slug_tokens() {
        let shell = "a=\"__RS_DEFAULT_COLORWAY__\";b=\"__RS_DEFAULT_SHADE__\";";
        let out = inject_theme_defaults(shell.to_string(), "forest", "dark");
        assert_eq!(out, "a=\"forest\";b=\"dark\";");
    }

    #[test]
    fn blanks_script_context_breakout() {
        let shell = "a=\"__RS_DEFAULT_COLORWAY__\";";
        let out = inject_theme_defaults(
            shell.to_string(),
            "\"</script><script>alert(1)</script>",
            "dark",
        );
        assert_eq!(out, "a=\"\";");
        assert!(!out.contains("<script>"));
    }

    #[test]
    fn blanks_empty_value() {
        let shell = "a=\"__RS_DEFAULT_COLORWAY__\";";
        assert_eq!(
            inject_theme_defaults(shell.to_string(), "", "dark"),
            "a=\"\";"
        );
    }
}
