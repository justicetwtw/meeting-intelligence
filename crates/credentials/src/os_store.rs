use keyring::{Entry, Error as KeyringError};

use crate::{CredentialError, CredentialSlot, CredentialStore, SecretString};

/// Default credential-manager service name. Every slot is a separate entry
/// underneath it.
const DEFAULT_SERVICE: &str = "org.meeting-intelligence.desktop";

/// OS-native credential storage.
///
/// On Windows this is the Windows Credential Manager, on macOS the Keychain,
/// and on other Unix systems the Secret Service — chosen and maintained by the
/// `keyring` crate rather than by hand-rolled cryptography in this repository.
pub struct OsCredentialStore {
    service: String,
}

impl OsCredentialStore {
    pub fn new() -> Self {
        Self {
            service: DEFAULT_SERVICE.to_owned(),
        }
    }

    /// Separate service namespace, used so a developer run cannot overwrite an
    /// installed application's stored credential.
    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, slot: CredentialSlot) -> Result<Entry, CredentialError> {
        Entry::new(&self.service, slot.as_str()).map_err(map_error)
    }
}

impl Default for OsCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps backend failures onto category-only errors. The upstream message is
/// intentionally discarded: it can quote entry contents on some platforms.
fn map_error(error: KeyringError) -> CredentialError {
    match error {
        KeyringError::NoStorageAccess(_) | KeyringError::NoDefaultStore => {
            CredentialError::Unavailable
        }
        KeyringError::BadEncoding(_) => CredentialError::Unreadable,
        _ => CredentialError::Backend,
    }
}

impl CredentialStore for OsCredentialStore {
    fn store(&self, slot: CredentialSlot, secret: &SecretString) -> Result<(), CredentialError> {
        self.entry(slot)?
            .set_password(secret.expose())
            .map_err(map_error)
    }

    fn read(&self, slot: CredentialSlot) -> Result<Option<SecretString>, CredentialError> {
        match self.entry(slot)?.get_password() {
            Ok(value) => SecretString::new(value)
                .map(Some)
                .map_err(|_| CredentialError::Unreadable),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(map_error(error)),
        }
    }

    fn delete(&self, slot: CredentialSlot) -> Result<(), CredentialError> {
        match self.entry(slot)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(map_error(error)),
        }
    }
}
