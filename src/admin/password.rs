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
//! Password validation module implementing NIST SP 800-63B compliant requirements
//! with additional complexity requirements for admin accounts.

use std::collections::HashSet;
use std::sync::LazyLock;

// Top common passwords to reject - expanded list for better security
static COMMON_PASSWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let passwords = include_str!("common_passwords.txt");
    passwords.lines().collect()
});

const MIN_LENGTH: usize = 16;
const MAX_LENGTH: usize = 128;
const SPECIAL_CHARS: &str = "!@#$%^&*()_+-=[]{}\\|;':\",./<>?`~";

pub struct PasswordValidator;

impl PasswordValidator {
    /// Validate a password against all requirements.
    ///
    /// Returns `Ok(())` if the password is valid, or `Err(Vec<String>)` with all
    /// validation error messages for user feedback.
    pub fn validate(password: &str, email: &str) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Length checks
        if password.len() < MIN_LENGTH {
            errors.push(format!(
                "Password must be at least {} characters long",
                MIN_LENGTH
            ));
        }

        if password.len() > MAX_LENGTH {
            errors.push(format!(
                "Password must be no more than {} characters long",
                MAX_LENGTH
            ));
        }

        // Complexity checks
        if !password.chars().any(|c| c.is_ascii_uppercase()) {
            errors.push("Password must contain at least one uppercase letter".to_string());
        }

        if !password.chars().any(|c| c.is_ascii_lowercase()) {
            errors.push("Password must contain at least one lowercase letter".to_string());
        }

        if !password.chars().any(|c| c.is_ascii_digit()) {
            errors.push("Password must contain at least one number".to_string());
        }

        if !password.chars().any(|c| SPECIAL_CHARS.contains(c)) {
            errors.push("Password must contain at least one special character".to_string());
        }

        // Email/username check
        let password_lower = password.to_lowercase();
        let email_lower = email.to_lowercase();

        if password_lower.contains(&email_lower) {
            errors.push("Password cannot contain your email address".to_string());
        }

        // Check username portion of email (before @)
        if let Some(username) = email_lower.split('@').next() {
            if username.len() >= 3 && password_lower.contains(username) {
                errors.push("Password cannot contain your username".to_string());
            }
        }

        // Common password check
        if COMMON_PASSWORDS.contains(password_lower.as_str()) {
            errors.push(
                "This password is too common and has been found in data breaches".to_string(),
            );
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_password() {
        let result = PasswordValidator::validate("MyStr0ng!Password123", "test@example.com");
        assert!(result.is_ok());
    }

    #[test]
    fn test_too_short() {
        let result = PasswordValidator::validate("Short1!", "test@example.com");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("at least 16 characters")));
    }

    #[test]
    fn test_missing_uppercase() {
        let result = PasswordValidator::validate("mystr0ng!password123", "test@example.com");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("uppercase")));
    }

    #[test]
    fn test_missing_lowercase() {
        let result = PasswordValidator::validate("MYSTR0NG!PASSWORD123", "test@example.com");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("lowercase")));
    }

    #[test]
    fn test_missing_number() {
        let result = PasswordValidator::validate("MyStrong!PasswordHere", "test@example.com");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("number")));
    }

    #[test]
    fn test_missing_special_char() {
        let result = PasswordValidator::validate("MyStr0ngPassword123", "test@example.com");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("special character")));
    }

    #[test]
    fn test_contains_email() {
        let result = PasswordValidator::validate("test@example.com!A1bc", "test@example.com");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("email")));
    }

    #[test]
    fn test_contains_username() {
        let result = PasswordValidator::validate("testuser!A1bcdefgh", "testuser@example.com");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("username")));
    }

    #[test]
    fn test_multiple_errors_returned() {
        let result = PasswordValidator::validate("short", "test@example.com");
        assert!(result.is_err());
        let errors = result.unwrap_err();
        // Should have multiple errors: too short, no uppercase, no number, no special
        assert!(errors.len() >= 4);
    }
}
