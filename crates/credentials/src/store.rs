use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

use thiserror::Error;

use crate::SecretString;

/// Named credential slots the product knows about. A slot maps to one entry in
/// the OS credential manager; adding a provider adds a slot, never a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CredentialSlot {
    OpenAiApiKey,
}

impl CredentialSlot {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiApiKey => "openai-api-key",
        }
    }
}

/// What an interface is allowed to know about a stored credential: whether one
/// exists and a non-reversible hint. Never the value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialStatus {
    pub configured: bool,
    pub hint: Option<String>,
}

impl CredentialStatus {
    pub fn not_configured() -> Self {
        Self {
            configured: false,
            hint: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("the operating system credential store is unavailable")]
    Unavailable,
    #[error("the stored credential could not be read back")]
    Unreadable,
    #[error("credential storage failed")]
    Backend,
}

/// Credential persistence boundary.
///
/// Implementations must keep secrets out of application files. Errors are
/// deliberately message-free so a backend cannot leak the value through an
/// error string that is later rendered or logged.
pub trait CredentialStore: Send + Sync {
    fn store(&self, slot: CredentialSlot, secret: &SecretString) -> Result<(), CredentialError>;
    fn read(&self, slot: CredentialSlot) -> Result<Option<SecretString>, CredentialError>;
    fn delete(&self, slot: CredentialSlot) -> Result<(), CredentialError>;

    /// Default status derives from `read`, but a backend may override it with a
    /// cheaper existence check.
    fn status(&self, slot: CredentialSlot) -> Result<CredentialStatus, CredentialError> {
        Ok(match self.read(slot)? {
            Some(secret) => CredentialStatus {
                configured: true,
                hint: Some(secret.hint()),
            },
            None => CredentialStatus::not_configured(),
        })
    }
}

/// Process-local store used by tests and by headless runs. It never writes to
/// disk, so a test cannot pollute the developer's real credential manager.
#[derive(Default)]
pub struct InMemoryCredentialStore {
    entries: Mutex<HashMap<CredentialSlot, SecretString>>,
}

impl InMemoryCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn store(&self, slot: CredentialSlot, secret: &SecretString) -> Result<(), CredentialError> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(slot, secret.clone());
        Ok(())
    }

    fn read(&self, slot: CredentialSlot) -> Result<Option<SecretString>, CredentialError> {
        Ok(self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&slot)
            .cloned())
    }

    fn delete(&self, slot: CredentialSlot) -> Result<(), CredentialError> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&slot);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "sk-test-0123456789abcdef";

    #[test]
    fn a_stored_credential_round_trips_and_can_be_removed() -> Result<(), CredentialError> {
        let store = InMemoryCredentialStore::new();
        let secret = SecretString::new(SAMPLE).expect("sample key should be accepted");

        assert_eq!(
            store.status(CredentialSlot::OpenAiApiKey)?,
            CredentialStatus::not_configured()
        );

        store.store(CredentialSlot::OpenAiApiKey, &secret)?;
        let status = store.status(CredentialSlot::OpenAiApiKey)?;
        assert!(status.configured);
        assert_eq!(status.hint.as_deref(), Some("…cdef"));
        assert_eq!(
            store
                .read(CredentialSlot::OpenAiApiKey)?
                .as_ref()
                .map(SecretString::expose),
            Some(SAMPLE)
        );

        store.delete(CredentialSlot::OpenAiApiKey)?;
        assert!(store.read(CredentialSlot::OpenAiApiKey)?.is_none());
        Ok(())
    }

    #[test]
    fn replacing_a_credential_keeps_only_the_new_value() -> Result<(), CredentialError> {
        let store = InMemoryCredentialStore::new();
        store.store(
            CredentialSlot::OpenAiApiKey,
            &SecretString::new(SAMPLE).expect("sample key should be accepted"),
        )?;
        store.store(
            CredentialSlot::OpenAiApiKey,
            &SecretString::new("sk-test-fedcba9876543210")
                .expect("replacement key should be accepted"),
        )?;

        assert_eq!(
            store
                .read(CredentialSlot::OpenAiApiKey)?
                .as_ref()
                .map(SecretString::expose),
            Some("sk-test-fedcba9876543210")
        );
        Ok(())
    }

    #[test]
    fn credential_errors_carry_no_secret_bearing_detail() {
        for error in [
            CredentialError::Unavailable,
            CredentialError::Unreadable,
            CredentialError::Backend,
        ] {
            let rendered = error.to_string();
            assert!(!rendered.contains(SAMPLE));
            assert!(!rendered.contains("sk-"));
        }
    }

    #[test]
    fn a_status_never_carries_the_credential_itself() -> Result<(), CredentialError> {
        let store = InMemoryCredentialStore::new();
        store.store(
            CredentialSlot::OpenAiApiKey,
            &SecretString::new(SAMPLE).expect("sample key should be accepted"),
        )?;

        let status = store.status(CredentialSlot::OpenAiApiKey)?;
        let rendered = format!("{status:?}");
        assert!(!rendered.contains(SAMPLE));
        assert!(!rendered.contains("0123456789"));
        Ok(())
    }
}
