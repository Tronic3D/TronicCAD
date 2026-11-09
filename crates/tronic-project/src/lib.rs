//! TronicCAD project persistence.
//!
//! * [`format`] encodes and decodes the `.tronic` file format.
//! * [`store`] reads and writes files atomically.
//! * [`session`] is an open project, including detection of external changes.

pub mod areas;
pub mod autosave;
pub mod compare;
pub mod format;
pub mod session;
pub mod store;

pub use autosave::{AutosaveStore, DiskState, RecoveryEntry};
pub use format::{EXTENSION, FORMAT_VERSION};
pub use session::{ProjectSession, SaveMode};

use std::path::PathBuf;
use thiserror::Error;

/// Any failure while creating, reading or writing a project.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error(transparent)]
    Validation(#[from] tronic_core::ValidationError),
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not a TronicCAD project")]
    NotAProject { path: PathBuf },
    #[error("{path} uses format version {found}, this build supports up to {supported}")]
    UnsupportedVersion {
        path: PathBuf,
        found: u32,
        supported: u32,
    },
    #[error("{path} failed its integrity check")]
    ChecksumMismatch { path: PathBuf },
    #[error("{path} exceeds the size limit of {limit} bytes")]
    TooLarge { path: PathBuf, limit: u64 },
    #[error("{path} already exists")]
    AlreadyExists { path: PathBuf },
    #[error("{path} was changed outside this session")]
    Conflict { path: PathBuf },
    #[error("{path} has inconsistent content: {detail}")]
    InvalidContent { path: PathBuf, detail: String },
}
