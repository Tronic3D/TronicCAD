//! Everything the editor draws, in board coordinates. The interface holds no
//! placement logic of its own, so it cannot disagree with the backend.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use tronic_schematic::{ElementId, Point, Rotation};

use crate::copper::{analyze, Airwire};
use crate::model::*;
use crate::zones::ZoneFill;

#[derive(Debug, Serialize)]
pub struct FootprintView {
    pub id: ElementId,
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub at: Point,
    pub rotation: Rotation,
    pub bottom: bool,
    pub silk: Vec<[Point; 2]>,
    pub courtyard: [Point; 2],
}

#[derive(Debug, Serialize)]
pub struct BoardView {
    pub copper_layers: u8,
    pub rules: DesignRules,
    pub outline: Vec<Point>,
    pub footprints: Vec<FootprintView>,
    pub pads: Vec<ResolvedPad>,
    pub tracks: Vec<Track>,
    pub vias: Vec<Via>,
    pub zones: Vec<Zone>,
    pub fills: Vec<ZoneFill>,
    pub airwires: Vec<Airwire>,
    pub net_lengths: BTreeMap<String, i64>,
    pub nets: Vec<String>,
}

pub fn board_view(board: &Board) -> BoardView {
    let analysis = analyze(board);
    let pads = board.resolved_pads();
    let footprints = board
        .footprints
        .iter()
        .filter_map(|instance| {
            let definition = board.footprint_defs.get(&instance.footprint)?;
            let (min, max) = board.courtyard(instance)?;
            Some(FootprintView {
                id: instance.id,
                reference: instance.reference.clone(),
                value: instance.value.clone(),
                footprint: definition.name.clone(),
                at: instance.at,
                rotation: instance.rotation,
                bottom: instance.bottom,
                silk: definition
                    .silk
                    .iter()
                    .map(|stroke| {
                        stroke.map(|p| {
                            place_on_board(p, instance.at, instance.rotation, instance.bottom)
                        })
                    })
                    .collect(),
                courtyard: [min, max],
            })
        })
        .collect();
    let nets: BTreeSet<String> = pads
        .iter()
        .filter_map(|pad| pad.net.clone())
        .chain(board.tracks.iter().map(|track| track.net.clone()))
        .collect();
    BoardView {
        copper_layers: board.copper_layers,
        rules: board.rules.clone(),
        outline: board.outline.clone(),
        footprints,
        pads,
        tracks: (*board.tracks).clone(),
        vias: (*board.vias).clone(),
        zones: (*board.zones).clone(),
        fills: analysis.fills,
        airwires: analysis.airwires,
        net_lengths: analysis.net_lengths,
        nets: nets.into_iter().collect(),
    }
}
