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
/// Empty values leave the design-system fallback in effect.
///
/// Both the social SPA handler (`serve_spa` in main.rs) and the Open Graph
/// shells (`og.rs`) call this so the token substitution can never drift
/// between the two serving paths.
pub fn inject_theme_defaults(html: String, colorway: &str, shade: &str) -> String {
    html.replace("\"__RS_DEFAULT_COLORWAY__\"", &format!("\"{colorway}\""))
        .replace("\"__RS_DEFAULT_SHADE__\"", &format!("\"{shade}\""))
}
