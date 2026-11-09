//! All changes to a board, as atomic commands with undo.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use thiserror::Error;
use tronic_schematic::geometry::Transform;
use tronic_schematic::{ElementId, Netlist, Point};

use crate::model::*;
use crate::sync::{sync_board, SyncReport};

const MAX_UNDO: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PcbError {
    #[error("{0} copper layers are not supported")]
    UnsupportedLayerCount(u8),
    #[error("layer {0} does not exist")]
    UnknownLayer(u8),
    #[error("copper on layer {0} would be lost")]
    LayerInUse(u8),
    #[error("a track needs a net")]
    NoNet,
    #[error("a size must be positive")]
    InvalidSize,
    #[error("the board has only one copper layer")]
    SingleLayerBoard,
    #[error("a polygon needs at least three corners and must not cross itself")]
    InvalidPolygon,
    #[error("this footprint is not on the board")]
    UnknownFootprint,
    #[error("nothing to undo or redo")]
    NothingToRestore,
}

/// A track without an id yet, as proposed by the router.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTrack {
    pub net: String,
    pub layer: u8,
    pub a: Point,
    pub b: Point,
    pub width: i64,
}

/// A via without an id yet, as proposed by the router.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewVia {
    pub net: String,
    pub at: Point,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PcbCommand {
    AddRoutes {
        tracks: Vec<NewTrack>,
        vias: Vec<NewVia>,
    },
    SetOutline {
        points: Vec<Point>,
    },
    AddTracks {
        net: String,
        layer: u8,
        points: Vec<Point>,
        width: i64,
    },
    AddVia {
        net: String,
        at: Point,
    },
    AddZone {
        net: String,
        layer: u8,
        outline: Vec<Point>,
    },
    Move {
        ids: Vec<ElementId>,
        delta: Point,
    },
    Rotate {
        ids: Vec<ElementId>,
    },
    Flip {
        ids: Vec<ElementId>,
    },
    Delete {
        ids: Vec<ElementId>,
    },
    SetRules {
        rules: DesignRules,
    },
    SetLayerCount {
        layers: u8,
    },
    SetMechanics {
        mechanics: Mechanics,
    },
    SetFootprintModel {
        footprint: String,
        model: Option<ImportedModel>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Applied {
    pub created: Vec<ElementId>,
}

#[derive(Debug, Clone, Default)]
pub struct PcbEditor {
    board: Board,
    undo: Vec<Board>,
    redo: Vec<Board>,
}

fn allocate(board: &mut Board) -> ElementId {
    let id = board.next_id;
    board.next_id += 1;
    id
}

fn check_layer(board: &Board, layer: u8) -> Result<(), PcbError> {
    if layer >= board.copper_layers {
        return Err(PcbError::UnknownLayer(layer));
    }
    Ok(())
}

fn run(board: &mut Board, command: PcbCommand) -> Result<Applied, PcbError> {
    let mut applied = Applied::default();
    match command {
        PcbCommand::AddRoutes { tracks, vias } => {
            let mut new_tracks = Vec::with_capacity(tracks.len());
            for track in tracks {
                check_layer(board, track.layer)?;
                if track.width <= 0 {
                    return Err(PcbError::InvalidSize);
                }
                let id = allocate(board);
                new_tracks.push(Track {
                    id,
                    net: track.net,
                    layer: track.layer,
                    a: track.a,
                    b: track.b,
                    width: track.width,
                });
                applied.created.push(id);
            }
            let mut new_vias = Vec::with_capacity(vias.len());
            for via in vias {
                let id = allocate(board);
                new_vias.push(Via {
                    id,
                    net: via.net,
                    at: via.at,
                    drill: board.rules.via_drill,
                    diameter: board.rules.via_diameter,
                });
                applied.created.push(id);
            }
            Arc::make_mut(&mut board.tracks).extend(new_tracks);
            Arc::make_mut(&mut board.vias).extend(new_vias);
        }
        PcbCommand::SetOutline { points } => {
            if !points.is_empty() && crate::geom::polygon_is_invalid(&points) {
                return Err(PcbError::InvalidPolygon);
            }
            board.outline = points;
        }
        PcbCommand::AddTracks {
            net,
            layer,
            points,
            width,
        } => {
            check_layer(board, layer)?;
            if net.trim().is_empty() {
                return Err(PcbError::NoNet);
            }
            if width <= 0 {
                return Err(PcbError::InvalidSize);
            }
            let mut added = Vec::new();
            for pair in points.windows(2) {
                if pair[0] != pair[1] {
                    let id = allocate(board);
                    added.push(Track {
                        id,
                        net: net.clone(),
                        layer,
                        a: pair[0],
                        b: pair[1],
                        width,
                    });
                    applied.created.push(id);
                }
            }
            Arc::make_mut(&mut board.tracks).extend(added);
        }
        PcbCommand::AddVia { net, at } => {
            if net.trim().is_empty() {
                return Err(PcbError::NoNet);
            }
            let id = allocate(board);
            let via = Via {
                id,
                net,
                at,
                drill: board.rules.via_drill,
                diameter: board.rules.via_diameter,
            };
            Arc::make_mut(&mut board.vias).push(via);
            applied.created.push(id);
        }
        PcbCommand::AddZone {
            net,
            layer,
            outline,
        } => {
            check_layer(board, layer)?;
            if net.trim().is_empty() {
                return Err(PcbError::NoNet);
            }
            if crate::geom::polygon_is_invalid(&outline) {
                return Err(PcbError::InvalidPolygon);
            }
            let id = allocate(board);
            Arc::make_mut(&mut board.zones).push(Zone {
                id,
                net,
                layer,
                outline,
            });
            applied.created.push(id);
        }
        PcbCommand::Move { ids, delta } => {
            let ids: HashSet<ElementId> = ids.into_iter().collect();
            let moved = |p: &mut Point| *p = p.offset(delta);
            if board.footprints.iter().any(|e| ids.contains(&e.id)) {
                for item in Arc::make_mut(&mut board.footprints) {
                    if ids.contains(&item.id) {
                        moved(&mut item.at);
                    }
                }
            }
            if board.tracks.iter().any(|e| ids.contains(&e.id)) {
                for item in Arc::make_mut(&mut board.tracks) {
                    if ids.contains(&item.id) {
                        moved(&mut item.a);
                        moved(&mut item.b);
                    }
                }
            }
            if board.vias.iter().any(|e| ids.contains(&e.id)) {
                for item in Arc::make_mut(&mut board.vias) {
                    if ids.contains(&item.id) {
                        moved(&mut item.at);
                    }
                }
            }
            if board.zones.iter().any(|e| ids.contains(&e.id)) {
                for item in Arc::make_mut(&mut board.zones) {
                    if ids.contains(&item.id) {
                        item.outline.iter_mut().for_each(moved);
                    }
                }
            }
        }
        PcbCommand::Rotate { ids } => {
            let ids: HashSet<ElementId> = ids.into_iter().collect();
            for item in Arc::make_mut(&mut board.footprints) {
                if ids.contains(&item.id) {
                    let (rotation, _) =
                        Transform::RotateClockwise(item.at).orientation(item.rotation, false);
                    item.rotation = rotation;
                }
            }
        }
        PcbCommand::Flip { ids } => {
            if board.copper_layers < 2 {
                return Err(PcbError::SingleLayerBoard);
            }
            let ids: HashSet<ElementId> = ids.into_iter().collect();
            for item in Arc::make_mut(&mut board.footprints) {
                if ids.contains(&item.id) {
                    item.bottom = !item.bottom;
                }
            }
        }
        PcbCommand::Delete { ids } => {
            let ids: HashSet<ElementId> = ids.into_iter().collect();
            Arc::make_mut(&mut board.tracks).retain(|e| !ids.contains(&e.id));
            Arc::make_mut(&mut board.vias).retain(|e| !ids.contains(&e.id));
            Arc::make_mut(&mut board.zones).retain(|e| !ids.contains(&e.id));
            // Parts come from the schematic and stay until it drops them.
        }
        PcbCommand::SetRules { rules } => {
            let sizes = [
                rules.min_clearance,
                rules.min_track_width,
                rules.track_width,
                rules.via_drill,
                rules.via_diameter,
                rules.min_drill,
                rules.max_drill,
            ];
            if sizes.iter().any(|size| *size <= 0) || rules.via_diameter <= rules.via_drill {
                return Err(PcbError::InvalidSize);
            }
            board.rules = rules;
        }
        PcbCommand::SetLayerCount { layers } => set_layer_count(board, layers)?,
        PcbCommand::SetMechanics { mechanics } => {
            if mechanics.thickness <= 0 || mechanics.part_clearance < 0 {
                return Err(PcbError::InvalidSize);
            }
            board.mechanics = mechanics;
        }
        PcbCommand::SetFootprintModel { footprint, model } => {
            let definition = board
                .footprint_defs
                .get_mut(&footprint)
                .ok_or(PcbError::UnknownFootprint)?;
            Arc::make_mut(definition).model = model.map(Arc::new);
        }
    }
    Ok(applied)
}

/// The bottom layer stays the bottom layer, inner layers keep their index.
fn set_layer_count(board: &mut Board, layers: u8) -> Result<(), PcbError> {
    if !ALLOWED_LAYER_COUNTS.contains(&layers) {
        return Err(PcbError::UnsupportedLayerCount(layers));
    }
    let old_bottom = board.bottom_layer();
    let new_bottom = layers - 1;
    let map = |layer: u8| -> Result<u8, PcbError> {
        if layer == old_bottom {
            Ok(new_bottom)
        } else if layer < new_bottom {
            Ok(layer)
        } else {
            Err(PcbError::LayerInUse(layer))
        }
    };
    // Going to one layer, bottom copper would land on the top side.
    if layers == 1 && old_bottom != 0 {
        if let Some(track) = board.tracks.iter().find(|t| t.layer != 0) {
            return Err(PcbError::LayerInUse(track.layer));
        }
        if let Some(zone) = board.zones.iter().find(|z| z.layer != 0) {
            return Err(PcbError::LayerInUse(zone.layer));
        }
        if board.footprints.iter().any(|f| f.bottom) {
            return Err(PcbError::LayerInUse(old_bottom));
        }
    }
    let tracks: Vec<u8> = board
        .tracks
        .iter()
        .map(|t| map(t.layer))
        .collect::<Result<_, _>>()?;
    let zones: Vec<u8> = board
        .zones
        .iter()
        .map(|z| map(z.layer))
        .collect::<Result<_, _>>()?;
    for (track, layer) in Arc::make_mut(&mut board.tracks).iter_mut().zip(tracks) {
        track.layer = layer;
    }
    for (zone, layer) in Arc::make_mut(&mut board.zones).iter_mut().zip(zones) {
        zone.layer = layer;
    }
    board.copper_layers = layers;
    Ok(())
}

impl PcbEditor {
    pub fn new(board: Board) -> Self {
        Self {
            board,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn board(&self) -> &Board {
        &self.board
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn remember(&mut self, before: Board) -> bool {
        if self.board == before {
            return false;
        }
        self.undo.push(before);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        true
    }

    /// Runs a command. On error the board is unchanged. Returns whether it changed.
    pub fn apply(&mut self, command: PcbCommand) -> Result<(Applied, bool), PcbError> {
        let before = self.board.clone();
        match run(&mut self.board, command) {
            Ok(applied) => {
                let changed = self.remember(before);
                Ok((applied, changed))
            }
            Err(error) => {
                self.board = before;
                Err(error)
            }
        }
    }

    /// Applies the netlist as one undo step.
    pub fn sync(
        &mut self,
        netlist: &Netlist,
        find_footprint: &dyn Fn(&str) -> Option<FootprintDef>,
    ) -> (SyncReport, bool) {
        let before = self.board.clone();
        let report = sync_board(&mut self.board, netlist, find_footprint);
        let changed = self.remember(before);
        (report, changed)
    }

    pub fn undo(&mut self) -> Result<(), PcbError> {
        let state = self.undo.pop().ok_or(PcbError::NothingToRestore)?;
        self.redo.push(std::mem::replace(&mut self.board, state));
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), PcbError> {
        let state = self.redo.pop().ok_or(PcbError::NothingToRestore)?;
        self.undo.push(std::mem::replace(&mut self.board, state));
        Ok(())
    }
}
