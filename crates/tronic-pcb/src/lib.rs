//! Circuit board model, edit commands, copper connectivity and zone filling.
//! Specification: `docs/specs/pcb.md`.

pub mod copper;
pub mod editor;
pub mod geom;
pub mod model;
pub mod sync;
pub mod view;
pub mod zones;

pub use copper::{analyze, Airwire, CopperAnalysis, CopperItem, ItemKind};
pub use editor::{Applied, NewTrack, NewVia, PcbCommand, PcbEditor, PcbError};
pub use geom::{Grid, Shape};
pub use model::*;
pub use sync::{compare_with_netlist, sync_board, Deviation, SyncReport};
pub use view::{board_view, BoardView};
pub use zones::{fill_zones, ZoneFill};
