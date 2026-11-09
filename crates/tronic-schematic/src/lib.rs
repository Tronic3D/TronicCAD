//! Schematic data model, edit commands, connectivity and netlist.
//! See `docs/specs/schematic.md` for the rules implemented here.

pub mod connectivity;
pub mod editor;
pub mod geometry;
pub mod model;
pub mod netlist;
mod normalize;

pub use connectivity::{Connectivity, Net, NetAttachment, PinNode};
pub use editor::{Applied, Clip, EditCommand, EditError, Editor};
pub use geometry::{Point, Rotation, GRID};
pub use model::*;
pub use netlist::Netlist;
