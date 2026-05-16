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
use totp_rs::{Algorithm, Secret, TOTP};

const ISSUER: &str = "PersonalSite";

/// Result of generating a new TOTP secret
pub struct TotpSetup {
    /// Base32-encoded secret for storage
    pub secret_base32: String,
    /// QR code as base64-encoded PNG data URL
    pub qr_code_base64: String,
    /// otpauth:// URL for manual entry
    pub otpauth_url: String,
}

/// Generate a new TOTP secret and QR code for a user
pub fn generate_secret(email: &str) -> Result<TotpSetup, String> {
    let secret = Secret::generate_secret();
    let secret_bytes = secret
        .to_bytes()
        .map_err(|e| format!("Failed to convert secret to bytes: {}", e))?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,  // digits
        1,  // skew (±1 time window)
        30, // step (30 seconds)
        secret_bytes,
        Some(ISSUER.to_string()),
        email.to_string(),
    )
    .map_err(|e| format!("Failed to create TOTP: {}", e))?;

    let qr_code_base64 = totp
        .get_qr_base64()
        .map_err(|e| format!("Failed to generate QR code: {}", e))?;

    let otpauth_url = totp.get_url();
    let secret_base32 = secret.to_encoded().to_string();

    Ok(TotpSetup {
        secret_base32,
        qr_code_base64,
        otpauth_url,
    })
}

/// Verify a TOTP code against a stored secret
pub fn verify_code(secret_base32: &str, code: &str, email: &str) -> Result<bool, String> {
    verify_code_with_skew(secret_base32, code, email, 1)
}

/// Verify a TOTP code with zero grace period (strict mode)
/// Used for sensitive operations like password reset where timing must be exact
pub fn verify_code_strict(secret_base32: &str, code: &str, email: &str) -> Result<bool, String> {
    verify_code_with_skew(secret_base32, code, email, 0)
}

/// Internal: Verify a TOTP code with configurable skew (time window tolerance)
fn verify_code_with_skew(
    secret_base32: &str,
    code: &str,
    email: &str,
    skew: u8,
) -> Result<bool, String> {
    let secret = Secret::Encoded(secret_base32.to_string());
    let secret_bytes = secret
        .to_bytes()
        .map_err(|e| format!("Failed to decode secret: {}", e))?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        6, // digits
        skew,
        30, // step (30 seconds)
        secret_bytes,
        Some(ISSUER.to_string()),
        email.to_string(),
    )
    .map_err(|e| format!("Failed to create TOTP: {}", e))?;

    Ok(totp.check_current(code).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_secret() {
        let setup = generate_secret("test@example.com").unwrap();

        // Secret should be a valid base32 string
        assert!(!setup.secret_base32.is_empty());

        // QR code should be base64 encoded
        assert!(!setup.qr_code_base64.is_empty());

        // URL should contain the email (URL-encoded) and issuer
        // Email is URL-encoded: @ becomes %40
        assert!(
            setup.otpauth_url.contains("test%40example.com")
                || setup.otpauth_url.contains("test@example.com")
        );
        assert!(setup.otpauth_url.contains("PersonalSite"));
    }

    #[test]
    fn test_verify_code_does_not_error() {
        let setup = generate_secret("test@example.com").unwrap();

        // Verify that code verification doesn't error (result may be true or false)
        let result = verify_code(&setup.secret_base32, "000000", "test@example.com");
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_code_with_valid_code() {
        let email = "test@example.com";
        let setup = generate_secret(email).unwrap();

        // Generate a valid code from the same secret
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            Secret::Encoded(setup.secret_base32.clone())
                .to_bytes()
                .unwrap(),
            Some("PersonalSite".to_string()),
            email.to_string(),
        )
        .unwrap();
        let code = totp.generate_current().unwrap();

        assert!(
            verify_code(&setup.secret_base32, &code, email).unwrap(),
            "Valid TOTP code should verify successfully"
        );
    }
}
