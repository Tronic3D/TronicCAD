//! Automatic snapshots of unsaved work, and recovery after a crash.
//! Specification: `docs/specs/autosave.md`.
//!
//! Snapshots live in a local directory, not next to the project, so that they
//! also work for read-only or unreachable network locations. Every running
//! application holds an operating system lock on its own lock file. The system
//! releases that lock when the process ends, however it ends, which tells a
//! crashed instance's snapshots apart from those of one still running.

use serde::{Deserialize, Serialize};
use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::format::{self, sha256_hex};
use crate::session::ProjectSession;
use crate::{store, ProjectError};

const FORMAT_ID: &str = "tronicautosave";
const FORMAT_VERSION: u32 = 1;
const SNAPSHOT_SUFFIX: &str = ".autosave";
const LOCK_SUFFIX: &str = ".lock";

#[derive(Serialize, Deserialize)]
struct Snapshot {
    format: String,
    format_version: u32,
    owner: String,
    original_path: String,
    /// Hash of the project file this work is based on.
    base_hash: String,
    saved_at: String,
    /// The complete project in the regular file format, checksum included.
    document: String,
}

/// How the project file relates to the state a snapshot is based on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskState {
    Unchanged,
    Changed,
    Missing,
}

/// Unsaved work left behind by an application that is no longer running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryEntry {
    pub path: String,
    pub name: String,
    pub saved_at: String,
    pub disk_state: DiskState,
}

/// The snapshot directory, as seen by one running application.
#[derive(Debug)]
pub struct AutosaveStore {
    directory: PathBuf,
    owner: String,
    /// Held for the lifetime of the application.
    _lock: File,
}

fn io_error(path: &Path, source: std::io::Error) -> ProjectError {
    ProjectError::Io {
        path: path.to_owned(),
        source,
    }
}

impl AutosaveStore {
    /// Opens the directory and registers this application as running.
    pub fn open(directory: &Path) -> Result<Self, ProjectError> {
        fs::create_dir_all(directory).map_err(|e| io_error(directory, e))?;
        let nanos = OffsetDateTime::now_utc().unix_timestamp_nanos();
        let owner = format!("{}-{nanos}", std::process::id());
        let lock_path = directory.join(format!("{owner}{LOCK_SUFFIX}"));
        let lock = File::create(&lock_path).map_err(|e| io_error(&lock_path, e))?;
        lock.lock().map_err(|e| io_error(&lock_path, e))?;
        let store = Self {
            directory: directory.to_owned(),
            owner,
            _lock: lock,
        };
        store.remove_stale_files();
        Ok(store)
    }

    fn snapshot_path(&self, project: &str) -> PathBuf {
        self.directory.join(format!(
            "{}{SNAPSHOT_SUFFIX}",
            sha256_hex(project.as_bytes())
        ))
    }

    fn owner_is_running(&self, owner: &str) -> bool {
        if owner == self.owner {
            return true;
        }
        // The name comes from a file; it must not lead out of the directory.
        if !owner.chars().all(|c| c.is_ascii_digit() || c == '-') {
            return false;
        }
        let path = self.directory.join(format!("{owner}{LOCK_SUFFIX}"));
        let Ok(file) = File::options().write(true).open(path) else {
            return false;
        };
        match file.try_lock() {
            Ok(()) => false,
            Err(TryLockError::WouldBlock) => true,
            // When in doubt, never touch what might be another's live work.
            Err(TryLockError::Error(_)) => true,
        }
    }

    /// Clears lock files of ended applications and interrupted temporary files.
    fn remove_stale_files(&self) {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stale = match name.strip_suffix(LOCK_SUFFIX) {
                Some(owner) => !self.owner_is_running(owner),
                None => name.starts_with(".tronic-save-"),
            };
            if stale {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    /// Writes a snapshot if the session changed since the last one.
    /// Returns its time, or `None` if there was nothing to do.
    pub fn write_if_needed(
        &self,
        session: &mut ProjectSession,
    ) -> Result<Option<String>, ProjectError> {
        if !session.needs_autosave() {
            return Ok(None);
        }
        self.write(session).map(Some)
    }

    /// Writes a snapshot of the session.
    pub fn write(&self, session: &mut ProjectSession) -> Result<String, ProjectError> {
        let original = session.path().display().to_string();
        let target = self.snapshot_path(&original);
        let saved_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_default();
        let bytes = format::encode(
            session.project(),
            session.schematic(),
            session.board(),
            session.path(),
        )?;
        let document = String::from_utf8(bytes).map_err(|e| {
            io_error(
                &target,
                std::io::Error::new(std::io::ErrorKind::InvalidData, e),
            )
        })?;
        let snapshot = Snapshot {
            format: FORMAT_ID.to_owned(),
            format_version: FORMAT_VERSION,
            owner: self.owner.clone(),
            original_path: original,
            base_hash: session.disk_hash().to_owned(),
            saved_at: saved_at.clone(),
            document,
        };
        let encoded = serde_json::to_vec(&snapshot).map_err(|e| io_error(&target, e.into()))?;
        store::replace_atomic(&target, &encoded)?;
        session.mark_autosaved();
        Ok(saved_at)
    }

    fn read(&self, path: &Path) -> Option<(Snapshot, format::Document)> {
        let bytes = store::read_file(path).ok()?;
        let snapshot: Snapshot = serde_json::from_slice(&bytes).ok()?;
        if snapshot.format != FORMAT_ID || snapshot.format_version > FORMAT_VERSION {
            return None;
        }
        let document = format::decode(
            snapshot.document.as_bytes(),
            Path::new(&snapshot.original_path),
        )
        .ok()?;
        Some((snapshot, document))
    }

    fn entry(snapshot: &Snapshot, document: &format::Document) -> RecoveryEntry {
        let disk_state = match store::read_if_exists(Path::new(&snapshot.original_path)) {
            Ok(None) => DiskState::Missing,
            Ok(Some(bytes)) if sha256_hex(&bytes) == snapshot.base_hash => DiskState::Unchanged,
            // An unreadable file counts as changed: it must not be overwritten unasked.
            _ => DiskState::Changed,
        };
        RecoveryEntry {
            path: snapshot.original_path.clone(),
            name: document.project.name.clone(),
            saved_at: snapshot.saved_at.clone(),
            disk_state,
        }
    }

    /// All intact snapshots whose application is no longer running, newest first.
    /// A damaged snapshot is skipped and left in place.
    pub fn recoverable(&self) -> Vec<RecoveryEntry> {
        let Ok(files) = fs::read_dir(&self.directory) else {
            return Vec::new();
        };
        let mut entries: Vec<RecoveryEntry> = files
            .flatten()
            .filter(|file| {
                file.file_name()
                    .to_string_lossy()
                    .ends_with(SNAPSHOT_SUFFIX)
            })
            .filter_map(|file| self.read(&file.path()))
            .filter(|(snapshot, _)| !self.owner_is_running(&snapshot.owner))
            .map(|(snapshot, document)| Self::entry(&snapshot, &document))
            .collect();
        entries.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
        entries
    }

    /// The recoverable snapshot of one project.
    pub fn recoverable_for(&self, project: &Path) -> Option<RecoveryEntry> {
        let path = self.snapshot_path(&project.display().to_string());
        let (snapshot, document) = self.read(&path)?;
        (!self.owner_is_running(&snapshot.owner)).then(|| Self::entry(&snapshot, &document))
    }

    /// Reopens unsaved work and takes the snapshot over from the ended application.
    pub fn recover(&self, project: &Path) -> Result<ProjectSession, ProjectError> {
        let path = self.snapshot_path(&project.display().to_string());
        let (snapshot, document) = self
            .read(&path)
            .ok_or_else(|| ProjectError::NotAProject { path: path.clone() })?;
        let mut session = ProjectSession::recovered(
            document,
            PathBuf::from(&snapshot.original_path),
            snapshot.base_hash,
        );
        // Immediately ours: a further crash must not lose it, and it must not
        // be offered to a second application meanwhile.
        self.write(&mut session)?;
        Ok(session)
    }

    /// Removes this application's snapshot of a project, once it is no longer needed.
    pub fn remove_own(&self, project: &Path) -> Result<(), ProjectError> {
        let path = self.snapshot_path(&project.display().to_string());
        let Ok(bytes) = store::read_file(&path) else {
            return Ok(());
        };
        let owned = serde_json::from_slice::<Snapshot>(&bytes)
            .is_ok_and(|snapshot| snapshot.owner == self.owner);
        if owned {
            fs::remove_file(&path).map_err(|e| io_error(&path, e))?;
        }
        Ok(())
    }

    /// Throws away left-behind work on the user's explicit request. Refuses
    /// snapshots of an application that is still running.
    pub fn discard(&self, project: &Path) -> Result<(), ProjectError> {
        let path = self.snapshot_path(&project.display().to_string());
        let running = store::read_file(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok())
            .is_some_and(|snapshot| self.owner_is_running(&snapshot.owner));
        if !running && path.exists() {
            fs::remove_file(&path).map_err(|e| io_error(&path, e))?;
        }
        Ok(())
    }
}
