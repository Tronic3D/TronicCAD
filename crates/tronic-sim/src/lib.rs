//! TronicCAD's own circuit simulator. Specification: `docs/specs/simulation.md`.
//!
//! Modified nodal analysis with Newton iteration. Own code with published model
//! equations, so no third-party licence is involved. `Circuit::to_spice` writes
//! the same circuit as a SPICE deck, which makes every result reproducible with
//! other simulators.

pub mod build;
pub mod circuit;
pub mod solve;
pub mod value;

pub use build::{circuit_from_schematic, is_ground_name, supply_voltage, BuildReport, ModelEntry};
pub use circuit::{
    BjtModel, Circuit, Device, DiodeModel, Element, MosModel, Source, Waveform, GROUND,
};
pub use solve::{simulate, Analysis, SimError, SimResult, Trace};
pub use value::{parse_source, parse_value};
