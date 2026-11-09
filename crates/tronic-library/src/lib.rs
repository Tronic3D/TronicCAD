//! TronicCAD's own component library.
//!
//! Every symbol here was drawn for this project and carries a provenance
//! record. Pin numbers of real part types are not yet checked against
//! datasheets by a person, which `reviewed: false` states.

mod builtin;
mod draw;
mod file;
mod footprints;

pub use builtin::{builtin, CATEGORIES};
pub use draw::{rectangular_part, CustomPartSpec, PinSide, SpecPin};
pub use file::{decode_library, encode_library, LibraryError, LIBRARY_EXTENSION};
pub use footprints::{find_footprint, footprints};
