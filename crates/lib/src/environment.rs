//! Limits shared by the database environment store and its clients.

pub const MAX_ENV_KEY_BYTES: usize = 256;
pub const MAX_ENV_VALUE_BYTES: usize = 8 * 1024;
pub const MAX_ENV_VARS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentValidationError {
    InvalidKey,
    ValueTooLarge,
    TooManyVariables,
}

impl std::fmt::Display for EnvironmentValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidKey => "invalid POSIX environment variable name (maximum 256 bytes)",
            Self::ValueTooLarge => "environment value exceeds 8192 UTF-8 bytes",
            Self::TooManyVariables => "environment store exceeds 256 variables",
        })
    }
}

impl std::error::Error for EnvironmentValidationError {}

pub fn validate_key(key: &str) -> Result<(), EnvironmentValidationError> {
    let bytes = key.as_bytes();
    if bytes.is_empty()
        || bytes.len() > MAX_ENV_KEY_BYTES
        || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_')
        || !bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return Err(EnvironmentValidationError::InvalidKey);
    }
    Ok(())
}

/// NUL is representable in the database. Container launch separately rejects it.
pub fn validate_value(value: &str) -> Result<(), EnvironmentValidationError> {
    if value.len() > MAX_ENV_VALUE_BYTES {
        return Err(EnvironmentValidationError::ValueTooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_utf8_byte_limits_and_posix_keys_without_container_policy() {
        for key in ["", "1FIRST", "A=B", "A\0B", "é", "A-B"] {
            assert_eq!(validate_key(key), Err(EnvironmentValidationError::InvalidKey));
        }
        assert!(validate_key(&"A".repeat(256)).is_ok());
        assert!(validate_key(&"A".repeat(257)).is_err());
        assert!(validate_key("SPACETIMEDB_USER_DATA").is_ok());
        assert!(validate_value("\0").is_ok());
        assert!(validate_value(&"é".repeat(4096)).is_ok());
        assert!(validate_value(&"é".repeat(4097)).is_err());
    }
}
