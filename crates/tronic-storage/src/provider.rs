//! The exchangeable storage interface.

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StorageError {
    /// Offline or the server is away. Worth trying again later.
    #[error("the storage cannot be reached: {0}")]
    Unreachable(String),
    #[error("the storage rejected the login")]
    Unauthorized,
    #[error("no access to this data")]
    Forbidden,
    #[error("the storage quota is used up")]
    QuotaExceeded,
    #[error("the storage reports status {status}: {message}")]
    Server { status: u16, message: String },
    #[error("the address is invalid")]
    InvalidUrl,
    #[error("external storage needs https")]
    InsecureUrl,
    #[error("the object name is invalid")]
    InvalidKey,
    #[error("the local storage reports: {0}")]
    Io(String),
    #[error("stored data failed its checksum")]
    Corrupt,
    #[error("the storage answer cannot be read")]
    BadResponse,
}

/// Objects by name. There is deliberately no way to delete or to move: what a
/// provider holds can only grow, so a mistake in the sync cannot destroy a version.
pub trait StorageProvider: Send + Sync {
    /// Stores an object, replacing one of the same name.
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError>;
    /// Reads an object. None if it does not exist.
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError>;
    /// All object names under a prefix, at any depth.
    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError>;
}

/// Object names are plain paths. This keeps them from leaving their folder on any
/// provider, whatever a remote listing or a damaged state file says.
pub fn check_key(key: &str) -> Result<(), StorageError> {
    let fine = !key.is_empty()
        && key.len() <= 512
        && key.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !segment.ends_with('.')
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        });
    if fine {
        Ok(())
    } else {
        Err(StorageError::InvalidKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_paths_are_allowed() {
        for key in ["a", "projects/1-2/blobs/abc.def", "A_b-c/d.json"] {
            assert_eq!(check_key(key), Ok(()), "{key}");
        }
    }

    #[test]
    fn anything_that_could_leave_the_folder_is_refused() {
        for key in [
            "",
            "/etc/passwd",
            "../up",
            "a/../../up",
            "a/./b",
            "a//b",
            "a/",
            "a\\b",
            "C:\\x",
            "a b",
            "a%2f..",
            "nul.",
            "ä",
        ] {
            assert_eq!(check_key(key), Err(StorageError::InvalidKey), "{key}");
        }
        assert_eq!(check_key(&"x".repeat(513)), Err(StorageError::InvalidKey));
    }
}
