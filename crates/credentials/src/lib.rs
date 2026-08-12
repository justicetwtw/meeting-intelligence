//! Secret handling and credential storage boundary.
//!
//! Nothing in this crate is serializable. A [`SecretString`] deliberately has no
//! `Serialize`, `Display` that reveals its value, or `Debug` that prints it, so
//! a credential cannot leak into settings JSON, SQLite payloads, tracing spans,
//! or an error rendered by an interface.

mod secret;
mod store;

#[cfg(feature = "os-store")]
mod os_store;

#[cfg(feature = "os-store")]
pub use os_store::OsCredentialStore;
pub use secret::{SecretString, SecretValidationError};
pub use store::{
    CredentialError, CredentialSlot, CredentialStatus, CredentialStore, InMemoryCredentialStore,
};
