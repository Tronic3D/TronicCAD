//! Persistent board data. All lengths in nanometres.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use thiserror::Error;
use tronic_schematic::{ElementId, Point, Provenance, Rotation};

pub const MM: i64 = 1_000_000;
pub const ALLOWED_LAYER_COUNTS: [u8; 5] = [1, 2, 4, 6, 8];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PadShape {
    Rect,
    Round,
    Oblong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PadDef {
    /// Matches the pin number of the symbol. Empty for mounting holes.
    pub number: String,
    pub at: Point,
    /// Width and height. For round pads the width is the diameter.
    pub size: Point,
    pub shape: PadShape,
    /// Through-hole pads have a drill; pads without one are surface mounted.
    pub drill: Option<i64>,
    #[serde(default = "plated_default")]
    pub plated: bool,
}

fn plated_default() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootprintDef {
    pub id: String,
    pub name: String,
    pub pads: Vec<PadDef>,
    /// Silkscreen strokes as pairs of points.
    pub silk: Vec<[Point; 2]>,
    /// Keep-out area of the part, corners relative to its origin.
    pub courtyard: [Point; 2],
    pub provenance: Provenance,
    /// The part's 3D shape, built from simple bodies.
    #[serde(default)]
    pub body: Vec<BodyPart>,
    /// A model imported by the user, used instead of the bodies.
    #[serde(default)]
    pub model: Option<Arc<ImportedModel>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    Box,
    Cylinder,
}

/// One simple body of a part. Z counts from the mounting surface of the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyPart {
    pub kind: BodyKind,
    pub center: Point,
    pub size: Point,
    pub z: i64,
    pub height: i64,
    pub color: [u8; 3],
}

/// Triangles of an imported model, flat as x, y, z per corner, in file units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportedModel {
    pub name: String,
    pub positions: Vec<f32>,
    /// File units to millimetres.
    pub scale: f32,
    /// Lifts the model above the board, in millimetres.
    pub z_offset: f32,
}

// Models hold floats but never NaN: the importer rejects them.
impl Eq for ImportedModel {}

/// Mechanical data of a board. All lengths in nanometres.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mechanics {
    pub thickness: i64,
    pub part_clearance: i64,
    pub max_height_top: Option<i64>,
    pub max_height_bottom: Option<i64>,
}

impl Default for Mechanics {
    fn default() -> Self {
        Self {
            thickness: 1_600_000,
            part_clearance: 250_000,
            max_height_top: None,
            max_height_bottom: None,
        }
    }
}

/// A placed part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootprintInstance {
    pub id: ElementId,
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub at: Point,
    pub rotation: Rotation,
    pub bottom: bool,
    /// Pad number to net name, from the netlist.
    pub pad_nets: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: ElementId,
    pub net: String,
    pub layer: u8,
    pub a: Point,
    pub b: Point,
    pub width: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Via {
    pub id: ElementId,
    pub net: String,
    pub at: Point,
    pub drill: i64,
    pub diameter: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zone {
    pub id: ElementId,
    pub net: String,
    pub layer: u8,
    pub outline: Vec<Point>,
}

/// Limits for the check and defaults for new copper. These are presets,
/// not the capabilities of a particular manufacturer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignRules {
    pub min_clearance: i64,
    pub min_track_width: i64,
    pub track_width: i64,
    pub via_drill: i64,
    pub via_diameter: i64,
    pub min_drill: i64,
    pub max_drill: i64,
    pub min_annular_ring: i64,
    pub min_hole_spacing: i64,
    pub edge_clearance: i64,
    pub mask_margin: i64,
}

impl Default for DesignRules {
    fn default() -> Self {
        Self {
            min_clearance: 200_000,
            min_track_width: 200_000,
            track_width: 250_000,
            via_drill: 300_000,
            via_diameter: 600_000,
            min_drill: 300_000,
            max_drill: 6_300_000,
            min_annular_ring: 130_000,
            min_hole_spacing: 250_000,
            edge_clearance: 300_000,
            mask_margin: 50_000,
        }
    }
}

/// A complete board. Lists are shared between undo states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub next_id: ElementId,
    pub copper_layers: u8,
    pub rules: DesignRules,
    /// Closed polygon. Empty until the user draws one.
    pub outline: Vec<Point>,
    pub footprints: Arc<Vec<FootprintInstance>>,
    pub tracks: Arc<Vec<Track>>,
    pub vias: Arc<Vec<Via>>,
    pub zones: Arc<Vec<Zone>>,
    pub footprint_defs: BTreeMap<String, Arc<FootprintDef>>,
    #[serde(default)]
    pub mechanics: Mechanics,
}

impl Default for Board {
    fn default() -> Self {
        Self {
            next_id: 1,
            copper_layers: 2,
            rules: DesignRules::default(),
            outline: Vec::new(),
            footprints: Arc::new(Vec::new()),
            tracks: Arc::new(Vec::new()),
            vias: Arc::new(Vec::new()),
            zones: Arc::new(Vec::new()),
            footprint_defs: BTreeMap::new(),
            mechanics: Mechanics::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("board is inconsistent: {0}")]
pub struct InvalidBoard(pub String);

/// Which copper layers something occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadLayers {
    Single(u8),
    All,
}

/// A pad in board coordinates, with its net resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedPad {
    pub instance: ElementId,
    pub reference: String,
    pub number: String,
    pub net: Option<String>,
    pub at: Point,
    pub size: Point,
    pub shape: PadShape,
    pub drill: Option<i64>,
    pub plated: bool,
    /// Copper layer of a surface pad, None for through-hole.
    pub layer: Option<u8>,
    pub bottom: bool,
}

impl ResolvedPad {
    pub fn layers(&self) -> PadLayers {
        match self.layer {
            Some(layer) => PadLayers::Single(layer),
            None => PadLayers::All,
        }
    }
}

/// Footprint space to board space: mirror for the bottom side, rotate, translate.
pub fn place_on_board(local: Point, at: Point, rotation: Rotation, bottom: bool) -> Point {
    tronic_schematic::geometry::place(local, at, rotation, bottom)
}

impl Board {
    pub fn bottom_layer(&self) -> u8 {
        self.copper_layers.saturating_sub(1)
    }

    /// All pads with their position, orientation and net on the board.
    pub fn resolved_pads(&self) -> Vec<ResolvedPad> {
        let mut out = Vec::new();
        for instance in self.footprints.iter() {
            let Some(definition) = self.footprint_defs.get(&instance.footprint) else {
                continue;
            };
            for pad in &definition.pads {
                let turned = matches!(instance.rotation, Rotation::R90 | Rotation::R270);
                let size = if turned {
                    Point::new(pad.size.y, pad.size.x)
                } else {
                    pad.size
                };
                let layer = pad.drill.is_none().then(|| {
                    if instance.bottom {
                        self.bottom_layer()
                    } else {
                        0
                    }
                });
                out.push(ResolvedPad {
                    instance: instance.id,
                    reference: instance.reference.clone(),
                    number: pad.number.clone(),
                    net: instance.pad_nets.get(&pad.number).cloned(),
                    at: place_on_board(pad.at, instance.at, instance.rotation, instance.bottom),
                    size,
                    shape: pad.shape,
                    drill: pad.drill,
                    plated: pad.plated,
                    layer,
                    bottom: instance.bottom,
                });
            }
        }
        out
    }

    /// The keep-out rectangle of a placed part, as min and max corner.
    pub fn courtyard(&self, instance: &FootprintInstance) -> Option<(Point, Point)> {
        let definition = self.footprint_defs.get(&instance.footprint)?;
        let a = place_on_board(
            definition.courtyard[0],
            instance.at,
            instance.rotation,
            instance.bottom,
        );
        let b = place_on_board(
            definition.courtyard[1],
            instance.at,
            instance.rotation,
            instance.bottom,
        );
        Some((
            Point::new(a.x.min(b.x), a.y.min(b.y)),
            Point::new(a.x.max(b.x), a.y.max(b.y)),
        ))
    }

    /// Checks structural rules of data read from outside.
    pub fn validate(&self) -> Result<(), InvalidBoard> {
        let fail = |message: String| Err(InvalidBoard(message));
        if !ALLOWED_LAYER_COUNTS.contains(&self.copper_layers) {
            return fail(format!("{} copper layers", self.copper_layers));
        }
        let mut seen = HashSet::new();
        let ids = self
            .footprints
            .iter()
            .map(|e| e.id)
            .chain(self.tracks.iter().map(|e| e.id))
            .chain(self.vias.iter().map(|e| e.id))
            .chain(self.zones.iter().map(|e| e.id));
        for id in ids {
            if id >= self.next_id || !seen.insert(id) {
                return fail(format!("bad element id {id}"));
            }
        }
        for instance in self.footprints.iter() {
            if !self.footprint_defs.contains_key(&instance.footprint) {
                return fail(format!("unknown footprint {}", instance.footprint));
            }
        }
        if self.mechanics.thickness <= 0 {
            return fail("board thickness must be positive".to_owned());
        }
        for track in self.tracks.iter() {
            if track.width <= 0 {
                return fail(format!("track {} has no width", track.id));
            }
        }
        for via in self.vias.iter() {
            if via.drill <= 0 || via.diameter <= 0 {
                return fail(format!("via {} has no size", via.id));
            }
        }
        Ok(())
    }
}
