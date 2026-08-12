use serde::Serialize;
use std::fmt;
use uuid::Uuid;

pub const KEYRING_SERVICE: &str = "io.bloomery.desktop";
pub const MAX_SECRET_GENERATION: u64 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef {
    profile_id: Uuid,
    credential_name: String,
    generation: u64,
}

impl SecretRef {
    pub fn new(profile_id: Uuid, credential_name: impl Into<String>) -> Result<Self, SecretError> {
        let credential_name = credential_name.into().trim().to_string();
        if credential_name.is_empty()
            || credential_name.len() > 64
            || !credential_name.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
            })
        {
            return Err(SecretError::invalid_reference());
        }
        Ok(Self {
            profile_id,
            credential_name,
            generation: 0,
        })
    }

    pub fn at_generation(
        profile_id: Uuid,
        credential_name: impl Into<String>,
        generation: u64,
    ) -> Result<Self, SecretError> {
        let mut reference = Self::new(profile_id, credential_name)?;
        reference.generation = generation;
        Ok(reference)
    }

    pub fn account(&self) -> String {
        let base = format!("{}/{}", self.profile_id, self.credential_name);
        if self.generation == 0 {
            base
        } else {
            format!("{base}/{}", self.generation)
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Result<Self, SecretError> {
        let value = value.into();
        if value.trim().is_empty() {
            Err(SecretError::new(
                "secret_value_required",
                "secret value is required",
            ))
        } else {
            Ok(Self(value))
        }
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretError {
    code: &'static str,
    message: String,
}

impl SecretError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn not_found() -> Self {
        Self::new("secret_not_found", "secret is not configured")
    }

    pub fn backend(message: impl Into<String>) -> Self {
        Self::new("secret_backend_failed", message)
    }

    fn invalid_reference() -> Self {
        Self::new("invalid_secret_reference", "invalid credential name")
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn is_not_found(&self) -> bool {
        self.code == "secret_not_found"
    }
}

impl fmt::Display for SecretError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SecretError {}

pub trait SecretStore: Send + Sync {
    fn set(&self, reference: &SecretRef, value: &SecretValue) -> Result<(), SecretError>;
    fn get(&self, reference: &SecretRef) -> Result<SecretValue, SecretError>;
    fn delete(&self, reference: &SecretRef) -> Result<(), SecretError>;
}

#[derive(Debug, Default)]
pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn set(&self, reference: &SecretRef, value: &SecretValue) -> Result<(), SecretError> {
        entry(reference)?
            .set_password(value.expose())
            .map_err(map_keyring_error)
    }

    fn get(&self, reference: &SecretRef) -> Result<SecretValue, SecretError> {
        entry(reference)?
            .get_password()
            .map_err(map_keyring_error)
            .and_then(SecretValue::new)
    }

    fn delete(&self, reference: &SecretRef) -> Result<(), SecretError> {
        entry(reference)?
            .delete_credential()
            .map_err(map_keyring_error)
    }
}

fn entry(reference: &SecretRef) -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(KEYRING_SERVICE, &reference.account()).map_err(map_keyring_error)
}

fn map_keyring_error(error: keyring::Error) -> SecretError {
    if matches!(error, keyring::Error::NoEntry) {
        SecretError::not_found()
    } else {
        SecretError::backend(format!("credential store operation failed: {error}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SecretStatus {
    pub configured: bool,
}

pub fn status(store: &dyn SecretStore, reference: &SecretRef) -> Result<SecretStatus, SecretError> {
    match store.get(reference) {
        Ok(_) => Ok(SecretStatus { configured: true }),
        Err(error) if error.is_not_found() => Ok(SecretStatus { configured: false }),
        Err(error) => Err(error),
    }
}

pub struct SecretState {
    store: Box<dyn SecretStore>,
}

impl Default for SecretState {
    fn default() -> Self {
        Self {
            store: Box::new(KeyringSecretStore),
        }
    }
}

impl SecretState {
    pub fn store(&self) -> &dyn SecretStore {
        self.store.as_ref()
    }
}
