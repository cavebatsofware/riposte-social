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
//! File-based credential resolver for runtime secrets.
//!
//! Each deployment topology riposte-social targets has a native mechanism
//! that delivers a secret as a file at a known path: Docker `secrets:`
//! mounts under `/run/secrets/`, systemd `LoadCredentialEncrypted` under
//! `$CREDENTIALS_DIRECTORY`, and mounted cloud secrets. `load` resolves a
//! secret through those paths, falling back to a raw environment variable
//! so existing env-based dev and CI setups keep working unchanged.

use std::env;
use std::path::PathBuf;
use zeroize::Zeroizing;

/// Resolve a secret named `name` (e.g. `SECURE_VALUES_KEY`), in order:
///
/// 1. `<NAME>_FILE` env var, read as a file path (explicit override).
/// 2. `$CREDENTIALS_DIRECTORY/<name_lower>` (systemd `LoadCredentialEncrypted`).
/// 3. `/run/secrets/<name_lower>` (Docker `secrets:` default).
/// 4. `<NAME>` env var (legacy fallback).
///
/// File contents have a single trailing line ending (`\n` or `\r\n`)
/// stripped, so `openssl rand -hex 32 > keyfile` works as written. Any
/// other bytes, including interior or leading whitespace, are preserved.
/// Returns `None` when no source yields a value.
pub fn load(name: &str) -> Option<Zeroizing<String>> {
    let lower = name.to_ascii_lowercase();

    if let Ok(path) = env::var(format!("{name}_FILE")) {
        if !path.is_empty() {
            return read_file(&PathBuf::from(path), name);
        }
    }

    if let Ok(dir) = env::var("CREDENTIALS_DIRECTORY") {
        let path = PathBuf::from(dir).join(&lower);
        if path.exists() {
            return read_file(&path, name);
        }
    }

    let run_secret = PathBuf::from("/run/secrets").join(&lower);
    if run_secret.exists() {
        return read_file(&run_secret, name);
    }

    env::var(name).ok().map(Zeroizing::new)
}

/// Resolve a secret that must be present; panic with a message naming the
/// resolution chain when it is missing.
pub fn load_required(name: &str) -> Zeroizing<String> {
    load(name).unwrap_or_else(|| {
        panic!(
            "{name} is not set. Provide it via {name}_FILE, \
            $CREDENTIALS_DIRECTORY/{lower}, /run/secrets/{lower}, or the \
            {name} environment variable.",
            lower = name.to_ascii_lowercase(),
        )
    })
}

fn read_file(path: &std::path::Path, name: &str) -> Option<Zeroizing<String>> {
    let mut bytes = match std::fs::read(path).map(Zeroizing::new) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("Failed to read {name} from {}: {e}", path.display());
            return None;
        }
    };

    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }

    match String::from_utf8(bytes.to_vec()).map(Zeroizing::new) {
        Ok(value) => Some(value),
        Err(_) => {
            tracing::error!("{name} from {} is not valid UTF-8", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    /// Serializes env mutation across this module's tests; cargo runs
    /// `#[test]` functions on multiple threads in one process, so two
    /// `set_var` calls would race on the C `environ` block.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const NAME: &str = "RIPOSTE_TEST_SECRET";

    /// Clear every precedence key so each test starts from a known state.
    /// SAFETY: the caller holds `ENV_LOCK` for the whole mutation window,
    /// so no other test in this binary is concurrently in `set_var` /
    /// `remove_var`, and no library code reads these keys from another
    /// thread here.
    fn clear_env() {
        unsafe {
            env::remove_var(format!("{NAME}_FILE"));
            env::remove_var("CREDENTIALS_DIRECTORY");
            env::remove_var(NAME);
        }
    }

    #[test]
    fn raw_env_is_the_last_resort() {
        let _guard = ENV_LOCK.lock().expect("env mutex poisoned");
        clear_env();
        unsafe { env::set_var(NAME, "from-env") };

        assert_eq!(load(NAME).as_deref().map(String::as_str), Some("from-env"));

        clear_env();
    }

    #[test]
    fn file_path_overrides_env_and_trims_one_newline() {
        let _guard = ENV_LOCK.lock().expect("env mutex poisoned");
        clear_env();

        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        writeln!(file, "from-file").expect("write temp file");

        unsafe {
            env::set_var(NAME, "from-env");
            env::set_var(format!("{NAME}_FILE"), file.path());
        }

        // _FILE wins over the raw env var, and the trailing newline is gone.
        assert_eq!(load(NAME).as_deref().map(String::as_str), Some("from-file"));

        clear_env();
    }

    #[test]
    fn credentials_directory_is_resolved_by_lowercase_name() {
        let _guard = ENV_LOCK.lock().expect("env mutex poisoned");
        clear_env();

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join(NAME.to_ascii_lowercase());
        std::fs::write(&path, "from-creds-dir\r\n").expect("write secret");

        unsafe { env::set_var("CREDENTIALS_DIRECTORY", dir.path()) };

        // Resolves the lowercase basename and strips the CRLF line ending.
        assert_eq!(
            load(NAME).as_deref().map(String::as_str),
            Some("from-creds-dir")
        );

        clear_env();
    }

    #[test]
    fn missing_secret_resolves_to_none() {
        let _guard = ENV_LOCK.lock().expect("env mutex poisoned");
        clear_env();

        assert!(load(NAME).is_none());

        clear_env();
    }

    #[test]
    fn interior_and_leading_whitespace_is_preserved() {
        let _guard = ENV_LOCK.lock().expect("env mutex poisoned");
        clear_env();

        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        writeln!(file, " pa ss\tword ").expect("write temp file");
        unsafe { env::set_var(format!("{NAME}_FILE"), file.path()) };

        assert_eq!(
            load(NAME).as_deref().map(String::as_str),
            Some(" pa ss\tword ")
        );

        clear_env();
    }
}
