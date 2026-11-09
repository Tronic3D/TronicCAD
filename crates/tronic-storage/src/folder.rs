//! A folder: an external drive, a network drive, a NAS.

use std::path::{Path, PathBuf};

use crate::provider::{check_key, StorageError, StorageProvider};

pub struct FolderProvider {
    root: PathBuf,
}

impl FolderProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, key: &str) -> Result<PathBuf, StorageError> {
        check_key(key)?;
        Ok(key
            .split('/')
            .fold(self.root.clone(), |path, part| path.join(part)))
    }
}

fn io(error: std::io::Error) -> StorageError {
    match error.kind() {
        // A drive that is not plugged in is the same as being offline.
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotConnected => {
            StorageError::Unreachable(error.to_string())
        }
        _ => StorageError::Io(error.to_string()),
    }
}

fn walk(directory: &Path, prefix: &str, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // Temporary files of interrupted writes are not objects.
        if name.ends_with(".partial") {
            continue;
        }
        let key = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type()?.is_dir() {
            walk(&entry.path(), &key, out)?;
        } else {
            out.push(key);
        }
    }
    Ok(())
}

impl StorageProvider for FolderProvider {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StorageError> {
        let path = self.path(key)?;
        if !self.root.is_dir() {
            return Err(StorageError::Unreachable(self.root.display().to_string()));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        // Written beside its place and renamed, so that an interrupted write never
        // looks like an object.
        let partial = path.with_extension("partial");
        std::fs::write(&partial, bytes).map_err(io)?;
        std::fs::rename(&partial, &path).map_err(io)
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        let path = self.path(key)?;
        if !self.root.is_dir() {
            return Err(StorageError::Unreachable(self.root.display().to_string()));
        }
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io(error)),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let start = self.path(prefix.trim_end_matches('/'))?;
        if !self.root.is_dir() {
            return Err(StorageError::Unreachable(self.root.display().to_string()));
        }
        if !start.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        walk(&start, prefix.trim_end_matches('/'), &mut out).map_err(io)?;
        Ok(out)
    }
}
