//! Key storage. The only real implementation is the OS keychain; a keychain error is reported
//! as unavailable and never answered with another store (ADR-020).

use crate::secret::Secret;

/// Keychain service name; the account is the profile id.
pub const KEYCHAIN_SERVICE: &str = "app.lullshift.companion";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStoreError;

pub trait KeyStore: Send + Sync {
    fn set(&self, account: &str, key: &Secret) -> Result<(), KeyStoreError>;
    /// `Ok(None)` when no key is saved for the account.
    fn get(&self, account: &str) -> Result<Option<Secret>, KeyStoreError>;
    /// Deleting a missing key is not an error.
    fn delete(&self, account: &str) -> Result<(), KeyStoreError>;
    /// Whether a key is saved, read from the keychain without any network use.
    fn has(&self, account: &str) -> Result<bool, KeyStoreError> {
        Ok(self.get(account)?.is_some())
    }
}

/// The OS keychain: Windows Credential Manager, macOS Keychain, Linux Secret Service.
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringStore;

impl KeyringStore {
    fn entry(account: &str) -> Result<keyring::Entry, KeyStoreError> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|_| KeyStoreError)
    }
}

impl KeyStore for KeyringStore {
    fn set(&self, account: &str, key: &Secret) -> Result<(), KeyStoreError> {
        Self::entry(account)?
            .set_password(key.expose())
            .map_err(|_| KeyStoreError)
    }

    fn get(&self, account: &str) -> Result<Option<Secret>, KeyStoreError> {
        match Self::entry(account)?.get_password() {
            Ok(p) => {
                let p = zeroize::Zeroizing::new(p);
                Secret::parse(&p).map(Some).map_err(|_| KeyStoreError)
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(KeyStoreError),
        }
    }

    fn delete(&self, account: &str) -> Result<(), KeyStoreError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(KeyStoreError),
        }
    }
}

/// An in-memory store for tests only (compiled with `test-support`, or in this crate's tests).
#[cfg(any(test, feature = "test-support"))]
mod memory {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::{KeyStore, KeyStoreError};
    use crate::secret::Secret;

    #[derive(Default)]
    pub struct MemoryKeyStore {
        keys: Mutex<HashMap<String, Secret>>,
        unavailable: bool,
    }

    impl MemoryKeyStore {
        pub fn new() -> Self {
            Self::default()
        }

        /// A store whose every call fails, like a missing Secret Service.
        pub fn unavailable() -> Self {
            Self {
                keys: Mutex::new(HashMap::new()),
                unavailable: true,
            }
        }
    }

    impl KeyStore for MemoryKeyStore {
        fn set(&self, account: &str, key: &Secret) -> Result<(), KeyStoreError> {
            if self.unavailable {
                return Err(KeyStoreError);
            }
            self.keys
                .lock()
                .map_err(|_| KeyStoreError)?
                .insert(account.to_owned(), key.clone());
            Ok(())
        }

        fn get(&self, account: &str) -> Result<Option<Secret>, KeyStoreError> {
            if self.unavailable {
                return Err(KeyStoreError);
            }
            Ok(self
                .keys
                .lock()
                .map_err(|_| KeyStoreError)?
                .get(account)
                .cloned())
        }

        fn delete(&self, account: &str) -> Result<(), KeyStoreError> {
            if self.unavailable {
                return Err(KeyStoreError);
            }
            self.keys.lock().map_err(|_| KeyStoreError)?.remove(account);
            Ok(())
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
pub use memory::MemoryKeyStore;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trips_and_unavailable_fails() {
        let s = MemoryKeyStore::new();
        let k = Secret::parse("sk-test-ABCDEFGH").unwrap();
        assert!(!s.has("a").unwrap());
        s.set("a", &k).unwrap();
        assert!(s.has("a").unwrap());
        assert_eq!(s.get("a").unwrap().unwrap().expose(), k.expose());
        s.delete("a").unwrap();
        s.delete("a").unwrap();
        assert!(s.get("a").unwrap().is_none());
        let u = MemoryKeyStore::unavailable();
        assert!(u.set("a", &k).is_err() && u.get("a").is_err() && u.has("a").is_err());
    }

    /// Real OS keychain. Run by hand: `cargo test -p companion -- --ignored`.
    #[test]
    #[ignore = "touches the real OS keychain"]
    fn real_keychain_round_trip() {
        let account = format!("lullshift-test-{}", std::process::id());
        let store = KeyringStore;
        let k = Secret::parse("test-key-NOT-A-REAL-KEY-12345").unwrap();
        let outcome = (|| {
            store.set(&account, &k)?;
            let got = store.get(&account)?.map(|s| s.expose().to_owned());
            store.delete(&account)?;
            let gone = store.get(&account)?.is_none();
            Ok::<_, KeyStoreError>((got, gone))
        })();
        // Clean up even when a step above failed.
        let _ = store.delete(&account);
        let (got, gone) = outcome.expect("keychain available");
        assert_eq!(got.as_deref(), Some(k.expose()));
        assert!(gone, "entry is deleted");
    }
}
