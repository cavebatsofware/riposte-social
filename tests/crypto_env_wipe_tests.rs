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
//! Each `tests/<name>.rs` integration test compiles to a separate
//! binary, so wiping `SECURE_VALUES_KEY` here can't race with the
//! parallel unit tests in `src/crypto.rs` that still read it.

use riposte_social::crypto;
use std::env;

const ENV_VAR: &str = "SECURE_VALUES_KEY";

#[test]
fn wipe_removes_env_var_and_crypto_keeps_working() {
    // CI / docker-compose.test.yml seeds this var before the test
    // binary launches.
    assert!(
        env::var(ENV_VAR).is_ok(),
        "{} must be set in the test environment",
        ENV_VAR
    );

    // Force the LazyLock to cache the key, then wipe.
    crypto::validate_encryption_key();
    crypto::wipe_encryption_key_from_env();

    assert!(
        env::var(ENV_VAR).is_err(),
        "{} should be unset after wipe_encryption_key_from_env",
        ENV_VAR
    );

    // After the wipe the cached key in the LazyLock keeps encryption
    // working; only env-dumping observers lose visibility.
    let ciphertext = crypto::encrypt_value("plaintext-after-wipe").unwrap();
    let decrypted = crypto::decrypt_value(&ciphertext).unwrap();
    assert_eq!(decrypted, "plaintext-after-wipe");
}
