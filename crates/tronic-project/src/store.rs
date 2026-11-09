//! Crash-safe file access.
//!
//! A save writes to a temporary file in the target directory, flushes it to
//! disk and renames it over the target. A crash therefore leaves either the
//! complete old file or the complete new file, never a mixture. The previous
//! version is kept next to it as `<name>.bak`.

use std::ffi::OsString;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use crate::ProjectError;

/// Upper bound for reading a project file, protecting against loading
/// arbitrary huge files by mistake.
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

fn io_error(path: &Path, source: std::io::Error) -> ProjectError {
    ProjectError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Path of the backup belonging to `path`.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_owned();
    name.push(".bak");
    PathBuf::from(name)
}

/// Reads a file, enforcing [`MAX_FILE_BYTES`].
pub fn read_file(path: &Path) -> Result<Vec<u8>, ProjectError> {
    let file = fs::File::open(path).map_err(|e| io_error(path, e))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(path, e))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ProjectError::TooLarge {
            path: path.to_owned(),
            limit: MAX_FILE_BYTES,
        });
    }
    Ok(bytes)
}

/// Like [`read_file`], but returns `None` if the file does not exist.
pub fn read_if_exists(path: &Path) -> Result<Option<Vec<u8>>, ProjectError> {
    match read_file(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(ProjectError::Io { source, .. }) if source.kind() == ErrorKind::NotFound => Ok(None),
        Err(other) => Err(other),
    }
}

/// Atomically replaces `path` with `bytes`, keeping the old version as backup.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    write_temp_then(path, bytes, true)
}

/// Atomically replaces `path` with `bytes`, without a backup.
pub fn replace_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    write_temp_then(path, bytes, false)
}

fn write_temp_then(path: &Path, bytes: &[u8], backup: bool) -> Result<(), ProjectError> {
    let directory = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let mut temp = tempfile::Builder::new()
        .prefix(".tronic-save-")
        .suffix(".tmp")
        .tempfile_in(directory)
        .map_err(|e| io_error(directory, e))?;
    temp.write_all(bytes).map_err(|e| io_error(path, e))?;
    temp.as_file().sync_all().map_err(|e| io_error(path, e))?;

    if backup {
        match fs::copy(path, backup_path(path)) {
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(io_error(&backup_path(path), e)),
        }
    }

    temp.persist(path).map_err(|e| io_error(path, e.error))?;

    // Makes the rename itself durable. Windows offers no directory handles
    // for this; NTFS journals the rename.
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| io_error(directory, e))?;
    Ok(())
}
