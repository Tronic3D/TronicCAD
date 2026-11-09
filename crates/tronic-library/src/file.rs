//! The `.troniclib` library file: symbols in JSON, version checked.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tronic_schematic::{validate_symbol, SymbolDef};

pub const LIBRARY_EXTENSION: &str = "troniclib";
const FORMAT_ID: &str = "troniclibrary";
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("not a TronicCAD library")]
    NotALibrary,
    #[error("library uses format {found}, this build supports up to {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("invalid symbol: {0}")]
    InvalidSymbol(String),
}

#[derive(Serialize, Deserialize)]
struct LibraryFile {
    format: String,
    format_version: u32,
    #[serde(default)]
    symbols: Vec<SymbolDef>,
}

#[derive(Deserialize)]
struct Header {
    format: String,
    format_version: u32,
}

pub fn encode_library(symbols: &[SymbolDef]) -> Result<Vec<u8>, LibraryError> {
    let file = LibraryFile {
        format: FORMAT_ID.to_owned(),
        format_version: FORMAT_VERSION,
        symbols: symbols.to_vec(),
    };
    serde_json::to_vec_pretty(&file).map_err(|_| LibraryError::NotALibrary)
}

pub fn decode_library(bytes: &[u8]) -> Result<Vec<SymbolDef>, LibraryError> {
    let header: Header = serde_json::from_slice(bytes).map_err(|_| LibraryError::NotALibrary)?;
    if header.format != FORMAT_ID {
        return Err(LibraryError::NotALibrary);
    }
    if header.format_version > FORMAT_VERSION {
        return Err(LibraryError::UnsupportedVersion {
            found: header.format_version,
            supported: FORMAT_VERSION,
        });
    }
    let file: LibraryFile = serde_json::from_slice(bytes).map_err(|_| LibraryError::NotALibrary)?;
    for symbol in &file.symbols {
        validate_symbol(symbol).map_err(LibraryError::InvalidSymbol)?;
    }
    Ok(file.symbols)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::builtin;

    #[test]
    fn round_trip_keeps_all_symbols() {
        let symbols = builtin();
        let bytes = encode_library(&symbols).unwrap();
        assert_eq!(decode_library(&bytes).unwrap(), symbols);
    }

    #[test]
    fn rejects_foreign_files_and_newer_versions() {
        assert!(matches!(
            decode_library(b"{}"),
            Err(LibraryError::NotALibrary)
        ));
        assert!(matches!(
            decode_library(br#"{"format":"other","format_version":1}"#),
            Err(LibraryError::NotALibrary)
        ));
        assert!(matches!(
            decode_library(br#"{"format":"troniclibrary","format_version":9}"#),
            Err(LibraryError::UnsupportedVersion { found: 9, .. })
        ));
    }

    #[test]
    fn rejects_symbols_with_repeated_pins() {
        let mut symbol = builtin().remove(0);
        let first = symbol.pins[0].clone();
        symbol.pins.push(first);
        let bytes = encode_library(&[symbol]).unwrap();
        assert!(matches!(
            decode_library(&bytes),
            Err(LibraryError::InvalidSymbol(_))
        ));
    }
}
