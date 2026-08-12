use std::fmt;

use thiserror::Error;
use zeroize::Zeroize;

/// Longest credential accepted. Bounded so a pasted file cannot be pushed into
/// the OS credential manager.
const MAX_SECRET_LENGTH: usize = 512;
/// Shortest credential accepted. Well below any real provider key, but enough
/// to reject an empty or obviously truncated paste.
const MIN_SECRET_LENGTH: usize = 16;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SecretValidationError {
    #[error("the credential is empty")]
    Empty,
    #[error("the credential is too short to be a provider key")]
    TooShort,
    #[error("the credential is longer than {MAX_SECRET_LENGTH} characters")]
    TooLong,
    #[error("the credential contains spaces or control characters")]
    InvalidCharacters,
}

/// An in-memory secret.
///
/// The inner value is only reachable through [`SecretString::expose`], which
/// makes every read site greppable. The buffer is zeroed on drop.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    /// Accepts a pasted credential after bounded validation. Validation errors
    /// never echo the value.
    pub fn new(value: impl Into<String>) -> Result<Self, SecretValidationError> {
        let mut value: String = value.into();
        let trimmed = value.trim();
        if trimmed.len() != value.len() {
            let owned = trimmed.to_owned();
            value.zeroize();
            value = owned;
        }

        if value.is_empty() {
            return Err(SecretValidationError::Empty);
        }
        if value.len() > MAX_SECRET_LENGTH {
            value.zeroize();
            return Err(SecretValidationError::TooLong);
        }
        if value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        {
            value.zeroize();
            return Err(SecretValidationError::InvalidCharacters);
        }
        if value.len() < MIN_SECRET_LENGTH {
            value.zeroize();
            return Err(SecretValidationError::TooShort);
        }

        Ok(Self(value))
    }

    /// The only way to read the secret. Callers must not log, serialize, or
    /// store the returned value anywhere other than an outbound provider
    /// request.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// A non-reversible hint safe to show in a UI, for example `sk-…3f9a`.
    /// It never contains enough characters to reconstruct the credential.
    pub fn hint(&self) -> String {
        let characters: Vec<char> = self.0.chars().collect();
        let tail: String = characters
            .iter()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("…{tail}")
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString(<redacted>)")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "sk-test-0123456789abcdef";

    #[test]
    fn debug_and_display_never_reveal_the_secret() {
        let secret = SecretString::new(SAMPLE).expect("sample key should be accepted");

        assert_eq!(format!("{secret:?}"), "SecretString(<redacted>)");
        assert_eq!(format!("{secret}"), "<redacted>");
        assert!(!format!("{secret:?} {secret}").contains("0123456789"));
    }

    #[test]
    fn a_hint_shows_only_the_last_four_characters() {
        let secret = SecretString::new(SAMPLE).expect("sample key should be accepted");

        assert_eq!(secret.hint(), "…cdef");
        assert!(!secret.hint().contains("sk-test"));
    }

    #[test]
    fn surrounding_whitespace_from_a_paste_is_removed() {
        let secret =
            SecretString::new(format!("  {SAMPLE}\n")).expect("padded key should be accepted");

        assert_eq!(secret.expose(), SAMPLE);
    }

    #[test]
    fn malformed_credentials_are_rejected_without_echoing_them() {
        let cases = [
            ("", SecretValidationError::Empty),
            ("   ", SecretValidationError::Empty),
            ("sk-short", SecretValidationError::TooShort),
            (
                "sk-test 0123456789abcdef",
                SecretValidationError::InvalidCharacters,
            ),
            (
                "sk-test\t0123456789abcdef",
                SecretValidationError::InvalidCharacters,
            ),
        ];

        for (input, expected) in cases {
            let error = SecretString::new(input).expect_err("malformed key must be rejected");
            assert_eq!(error, expected);
            assert!(!error.to_string().contains(input) || input.trim().is_empty());
        }

        let long = "a".repeat(MAX_SECRET_LENGTH + 1);
        assert_eq!(
            SecretString::new(long.clone()).expect_err("over-long key must be rejected"),
            SecretValidationError::TooLong
        );
    }
}
