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
//! Each `tests/<name>.rs` integration test compiles to a separate binary,
//! so this file owns the only `ENCRYPTION_KEY` LazyLock in its process and
//! can point it at a credential file before the first crypto call forces
//! the lock.

use riposte_social::crypto;
use std::io::Write;

#[test]
fn encryption_key_loads_from_file() {
    let mut file = tempfile::NamedTempFile::new().expect("temp key file");
    // A distinct non-zero key (0x11 * 32), so a successful round-trip can
    // only come from the file, not CI's all-zero `SECURE_VALUES_KEY`.
    let key_hex = "11".repeat(32);
    writeln!(file, "{key_hex}").expect("write key file");

    // SAFETY: set before any crypto call forces the LazyLock. This is the
    // only test in this binary, so no other thread mutates env here. The
    // raw `SECURE_VALUES_KEY` is removed so the file is the only source.
    unsafe {
        std::env::remove_var("SECURE_VALUES_KEY");
        std::env::set_var("SECURE_VALUES_KEY_FILE", file.path());
    }

    crypto::validate_encryption_key();

    let ciphertext = crypto::encrypt_value("plaintext-from-file-key").expect("encrypt");
    let decrypted = crypto::decrypt_value(&ciphertext).expect("decrypt");
    assert_eq!(decrypted, "plaintext-from-file-key");

    // With a valid key loaded, malformed input exercises the real decrypt
    // path rather than a missing-key error. Invalid base64, and valid
    // base64 that is shorter than the 12-byte nonce, both fail.
    assert!(crypto::decrypt_value("not-valid-base64!!!").is_err());
    assert!(crypto::decrypt_value("AAAA").is_err());
}
