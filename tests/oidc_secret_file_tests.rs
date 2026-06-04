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
//! `OidcConfig::from_env` sources `OIDC_CLIENT_SECRET` through the secret
//! file-path chain; the non-secret fields stay plain env vars.

use riposte_social::auth::oidc::OidcConfig;
use std::io::Write;

#[test]
fn client_secret_loads_from_file() {
    let mut file = tempfile::NamedTempFile::new().expect("temp secret file");
    writeln!(file, "super-secret-value").expect("write secret file");

    // SAFETY: only test in this binary; nothing else mutates env here.
    unsafe {
        std::env::remove_var("OIDC_CLIENT_SECRET");
        std::env::set_var("OIDC_CLIENT_SECRET_FILE", file.path());
        std::env::set_var("OIDC_CLIENT_ID", "headscale");
    }

    let config = OidcConfig::from_env();

    // Trailing newline trimmed, value sourced from the file.
    assert_eq!(config.client_secret, "super-secret-value");
    assert_eq!(config.client_id, "headscale");
}
