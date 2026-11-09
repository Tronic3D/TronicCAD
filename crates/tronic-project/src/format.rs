//! The `.tronic` file format, version 4. Specification:
//! `docs/specs/project-file-format.md` and `docs/specs/schematic.md`.
//!
//! A UTF-8 JSON envelope around the project. The checksum covers the exact
//! bytes of the `project` value as they appear in the file, so any change to
//! them is detected on load.
//!
//! Version 1 had no `schematic` field and version 2 no `board` field. Version 3
//! boards had no mechanical data. Missing data loads with its default.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tronic_core::Project;
use tronic_pcb::Board;
use tronic_schematic::Schematic;

use crate::ProjectError;

pub const FORMAT_ID: &str = "tronicproject";
pub const FORMAT_VERSION: u32 = 4;
pub const EXTENSION: &str = "tronic";
const CHECKSUM_ALGORITHM: &str = "sha-256";

#[derive(Serialize, Deserialize)]
struct Checksum {
    algorithm: String,
    value: String,
}

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format: &'a str,
    format_version: u32,
    checksum: Checksum,
    project: &'a RawValue,
}

/// Only identifies the file and its version, so that a file from a newer
/// release is reported as such instead of failing on an unknown structure.
#[derive(Deserialize)]
struct Header {
    format: String,
    format_version: u32,
}

#[derive(Deserialize)]
struct EnvelopeIn<'a> {
    checksum: Checksum,
    #[serde(borrow)]
    project: &'a RawValue,
}

#[derive(Serialize)]
struct BodyOut<'a> {
    #[serde(flatten)]
    project: &'a Project,
    schematic: &'a Schematic,
    board: &'a Board,
}

#[derive(Deserialize)]
struct BodyIn {
    #[serde(flatten)]
    project: Project,
    #[serde(default)]
    schematic: Schematic,
    #[serde(default)]
    board: Board,
}

/// The content of a project file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub project: Project,
    pub schematic: Schematic,
    pub board: Board,
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(byte & 0xf), 16).unwrap_or('0'));
    }
    out
}

/// Serializes a project into the bytes of a `.tronic` file.
pub fn encode(
    project: &Project,
    schematic: &Schematic,
    board: &Board,
    path: &Path,
) -> Result<Vec<u8>, ProjectError> {
    let to_io = |source: serde_json::Error| ProjectError::Io {
        path: path.to_owned(),
        source: source.into(),
    };
    let body = serde_json::to_string_pretty(&BodyOut {
        project,
        schematic,
        board,
    })
    .map_err(to_io)?;
    let checksum = Checksum {
        algorithm: CHECKSUM_ALGORITHM.to_owned(),
        value: sha256_hex(body.as_bytes()),
    };
    let raw = RawValue::from_string(body).map_err(to_io)?;
    let envelope = EnvelopeOut {
        format: FORMAT_ID,
        format_version: FORMAT_VERSION,
        checksum,
        project: &raw,
    };
    let mut bytes = serde_json::to_vec_pretty(&envelope).map_err(to_io)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Parses and verifies the bytes of a `.tronic` file.
pub fn decode(bytes: &[u8], path: &Path) -> Result<Document, ProjectError> {
    let not_a_project = || ProjectError::NotAProject {
        path: path.to_owned(),
    };
    let header: Header = serde_json::from_slice(bytes).map_err(|_| not_a_project())?;
    if header.format != FORMAT_ID {
        return Err(not_a_project());
    }
    if header.format_version > FORMAT_VERSION {
        return Err(ProjectError::UnsupportedVersion {
            path: path.to_owned(),
            found: header.format_version,
            supported: FORMAT_VERSION,
        });
    }
    let envelope: EnvelopeIn<'_> = serde_json::from_slice(bytes).map_err(|_| not_a_project())?;
    if envelope.checksum.algorithm != CHECKSUM_ALGORITHM
        || sha256_hex(envelope.project.get().as_bytes()) != envelope.checksum.value
    {
        return Err(ProjectError::ChecksumMismatch {
            path: path.to_owned(),
        });
    }
    let body: BodyIn = serde_json::from_str(envelope.project.get()).map_err(|_| not_a_project())?;
    body.project.validate()?;
    body.schematic
        .validate()
        .map_err(|error| ProjectError::InvalidContent {
            path: path.to_owned(),
            detail: error.0,
        })?;
    body.board
        .validate()
        .map_err(|error| ProjectError::InvalidContent {
            path: path.to_owned(),
            detail: error.0,
        })?;
    Ok(Document {
        project: body.project,
        schematic: body.schematic,
        board: body.board,
    })
}

/// Appends the `.tronic` extension unless the path already carries it.
pub fn with_extension(path: &Path) -> PathBuf {
    let has_extension = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION));
    if has_extension {
        return path.to_owned();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(EXTENSION);
    PathBuf::from(name)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn project() -> Project {
        Project::new(
            "电源 Größe",
            "Beschreibung\nZeile 2",
            OffsetDateTime::UNIX_EPOCH,
        )
        .unwrap()
    }

    fn path() -> &'static Path {
        Path::new("t.tronic")
    }

    fn bytes() -> Vec<u8> {
        encode(&project(), &Schematic::default(), &Board::default(), path()).unwrap()
    }

    #[test]
    fn round_trip() {
        let p = project();
        let schematic = Schematic::default();
        let encoded = encode(&p, &schematic, &Board::default(), path()).unwrap();
        let document = decode(&encoded, path()).unwrap();
        assert_eq!(document.project, p);
        assert_eq!(document.schematic, schematic);
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn detects_tampered_content() {
        let text = String::from_utf8(bytes()).unwrap();
        let tampered = text.replace("Beschreibung", "Manipuliert!");
        assert_ne!(text, tampered);
        assert!(matches!(
            decode(tampered.as_bytes(), path()),
            Err(ProjectError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn rejects_foreign_and_broken_files() {
        for bytes in [
            &b""[..],
            b"not json",
            b"{}",
            b"[1,2]",
            br#"{"format":"other","format_version":1}"#,
            br#"{"format":"tronicproject","format_version":1}"#,
        ] {
            assert!(
                matches!(decode(bytes, path()), Err(ProjectError::NotAProject { .. })),
                "input: {}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn reports_files_from_newer_versions() {
        let bytes = br#"{"format":"tronicproject","format_version":99,"future":{}}"#;
        match decode(bytes, path()) {
            Err(ProjectError::UnsupportedVersion {
                found, supported, ..
            }) => {
                assert_eq!((found, supported), (99, FORMAT_VERSION));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    fn file_with_body(body: &str) -> String {
        format!(
            r#"{{"format":"tronicproject","format_version":4,"checksum":{{"algorithm":"sha-256","value":"{}"}},"project":{body}}}"#,
            sha256_hex(body.as_bytes())
        )
    }

    #[test]
    fn rejects_valid_checksum_over_invalid_metadata() {
        let body = r#"{"id":"00000000-0000-4000-8000-000000000000","name":"  ","description":"","created_at":"2026-01-01T00:00:00Z","modified_at":"2026-01-01T00:00:00Z"}"#;
        assert!(matches!(
            decode(file_with_body(body).as_bytes(), path()),
            Err(ProjectError::Validation(_))
        ));
    }

    #[test]
    fn rejects_valid_checksum_over_an_inconsistent_schematic() {
        let body = r#"{"id":"00000000-0000-4000-8000-000000000000","name":"A","description":"","created_at":"2026-01-01T00:00:00Z","modified_at":"2026-01-01T00:00:00Z","schematic":{"next_id":1,"sheets":[],"symbols":{}}}"#;
        assert!(matches!(
            decode(file_with_body(body).as_bytes(), path()),
            Err(ProjectError::InvalidContent { .. })
        ));
    }

    #[test]
    fn extension_is_added_only_when_missing() {
        assert_eq!(
            with_extension(Path::new("a/b")),
            PathBuf::from("a/b.tronic")
        );
        assert_eq!(
            with_extension(Path::new("a/b.v2")),
            PathBuf::from("a/b.v2.tronic")
        );
        assert_eq!(
            with_extension(Path::new("a/B.TRONIC")),
            PathBuf::from("a/B.TRONIC")
        );
    }
}
