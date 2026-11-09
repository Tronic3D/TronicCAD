//! Storage providers and synchronization. Specification: `docs/specs/cloud.md`.
//!
//! The CAD core does not know this crate: it works on the bytes of a saved
//! project file. Providers only need to store objects by name, which a folder,
//! WebDAV, S3 and the TronicCAD cloud all can.

pub mod cloud;
pub mod folder;
mod http;
pub mod provider;
pub mod s3;
pub mod sync;
pub mod webdav;

pub use cloud::CloudProvider;
pub use folder::FolderProvider;
pub use http::{agent as http_agent, checked_base};
pub use provider::{check_key, StorageError, StorageProvider};
pub use s3::S3Provider;
pub use sync::{Commit, LinkState, Outcome, Repo};
pub use webdav::WebDavProvider;

use sha2::{Digest, Sha256};

/// A lowercase hexadecimal SHA-256.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
