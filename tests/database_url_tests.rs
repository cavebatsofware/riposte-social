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
//! Covers `database::database_url`: a literal `DATABASE_URL` wins verbatim,
//! and otherwise the URL is assembled from the connection parts plus a
//! password resolved from `POSTGRES_PASSWORD_FILE`, with special characters
//! percent-encoded.

use riposte_social::database;
use std::io::Write;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// SAFETY: callers hold `ENV_LOCK`, so no other test in this binary is
/// concurrently mutating these keys.
fn clear() {
    unsafe {
        std::env::remove_var("DATABASE_URL");
        std::env::remove_var("POSTGRES_PASSWORD");
        std::env::remove_var("POSTGRES_PASSWORD_FILE");
        std::env::remove_var("DATABASE_SSLMODE");
    }
}

#[test]
fn verbatim_database_url_wins() {
    let _guard = ENV_LOCK.lock().expect("env mutex poisoned");
    clear();
    let literal = "postgresql://u:pw@host:5432/d?sslmode=require";
    unsafe { std::env::set_var("DATABASE_URL", literal) };

    assert_eq!(database::database_url(), literal);

    clear();
}

#[test]
fn assembles_from_password_file_and_encodes_specials() {
    let _guard = ENV_LOCK.lock().expect("env mutex poisoned");
    clear();

    let mut file = tempfile::NamedTempFile::new().expect("temp password file");
    // Special chars plus a trailing space (the resolver trims only the
    // newline, so the space must survive).
    writeln!(file, "p@ss:w/rd ").expect("write password file");

    unsafe {
        std::env::set_var("DATABASE_HOST", "db.internal");
        std::env::set_var("DATABASE_PORT", "5432");
        std::env::set_var("POSTGRES_USER", "riposte_social_user");
        std::env::set_var("POSTGRES_DB", "riposte_social");
        std::env::set_var("POSTGRES_PASSWORD_FILE", file.path());
        std::env::set_var("DATABASE_SSLMODE", "require");
    }

    let url_str = database::database_url();
    let parsed = url::Url::parse(&url_str).expect("assembled URL parses");

    assert_eq!(parsed.scheme(), "postgresql");
    assert_eq!(parsed.username(), "riposte_social_user");
    assert_eq!(parsed.host_str(), Some("db.internal"));
    assert_eq!(parsed.port(), Some(5432));
    assert_eq!(parsed.path(), "/riposte_social");
    assert_eq!(parsed.query(), Some("sslmode=require"));

    let decoded = urlencoding::decode(parsed.password().expect("password present"))
        .expect("password decodes");
    assert_eq!(decoded, "p@ss:w/rd ");

    clear();
}
