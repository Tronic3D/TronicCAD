//! Manufacturing data: Gerber RS-274X with X2 attributes, Excellon drill files,
//! bill of materials and pick-and-place list. Specification: `docs/specs/pcb.md`.

mod font;
mod gerber;
mod sample;

use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write;
use tronic_pcb::*;
use tronic_schematic::{Point, Rotation};

use font::text_strokes;
use gerber::{Gerber, Polarity};
pub use sample::sample_board;

const SILK_WIDTH: i64 = 150_000;
const OUTLINE_WIDTH: i64 = 100_000;
const TEXT_HEIGHT: i64 = 1_000_000;

/// One generated file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FabFile {
    pub name: String,
    pub content: String,
}

fn copper_function(board: &Board, layer: u8) -> String {
    let number = layer + 1;
    let kind = if layer == 0 {
        "Top"
    } else if layer == board.bottom_layer() {
        "Bot"
    } else {
        "Inr"
    };
    format!("Copper,L{number},{kind}")
}

fn copper_name(board: &Board, layer: u8) -> String {
    let side = if layer == 0 {
        "top"
    } else if layer == board.bottom_layer() {
        "bottom"
    } else {
        "inner"
    };
    format!("copper-l{}-{side}", layer + 1)
}

fn pad_on_layer(pad: &ResolvedPad, layer: u8) -> bool {
    pad.plated && pad.layer.is_none_or(|own| own == layer)
}

fn copper_layer(board: &Board, layer: u8, pads: &[ResolvedPad], fills: &[ZoneFill]) -> String {
    let mut out = Gerber::new(&copper_function(board, layer), Polarity::Positive);
    // Largest shapes first: a shape inside another one's hole is always smaller.
    let mut shapes: Vec<&Vec<Vec<Point>>> = fills
        .iter()
        .filter(|fill| fill.layer == layer)
        .flat_map(|fill| fill.shapes.iter())
        .collect();
    shapes.sort_by_key(|shape| std::cmp::Reverse(shape.first().map_or(0, |c| area_key(c))));
    for shape in shapes {
        out.region_with_holes(shape);
    }
    for track in board.tracks.iter().filter(|t| t.layer == layer) {
        out.stroke(track.a, track.b, track.width);
    }
    for via in board.vias.iter() {
        out.flash(
            via.at,
            PadShape::Round,
            Point::new(via.diameter, via.diameter),
        );
    }
    for pad in pads.iter().filter(|pad| pad_on_layer(pad, layer)) {
        out.flash(pad.at, pad.shape, pad.size);
    }
    out.finish()
}

fn area_key(contour: &[Point]) -> i128 {
    let n = contour.len();
    let doubled: i128 = (0..n)
        .map(|i| {
            let (a, b) = (contour[i], contour[(i + 1) % n]);
            i128::from(a.x) * i128::from(b.y) - i128::from(a.y) * i128::from(b.x)
        })
        .sum();
    doubled.abs()
}

fn mask_layer(board: &Board, bottom: bool, pads: &[ResolvedPad]) -> String {
    let side = if bottom { "Bot" } else { "Top" };
    // Negative: what is drawn here stays free of solder mask.
    let mut out = Gerber::new(&format!("Soldermask,{side}"), Polarity::Negative);
    let margin = 2 * board.rules.mask_margin;
    for pad in pads {
        let on_side = pad.drill.is_some() || pad.bottom == bottom;
        if on_side {
            let size = Point::new(pad.size.x + margin, pad.size.y + margin);
            out.flash(pad.at, pad.shape, size);
        }
    }
    out.finish()
}

fn paste_layer(bottom: bool, pads: &[ResolvedPad]) -> String {
    let side = if bottom { "Bot" } else { "Top" };
    let mut out = Gerber::new(&format!("Paste,{side}"), Polarity::Positive);
    for pad in pads
        .iter()
        .filter(|p| p.drill.is_none() && p.bottom == bottom)
    {
        out.flash(pad.at, pad.shape, pad.size);
    }
    out.finish()
}

fn silk_layer(board: &Board, bottom: bool) -> String {
    let side = if bottom { "Bot" } else { "Top" };
    let mut out = Gerber::new(&format!("Legend,{side}"), Polarity::Positive);
    for instance in board.footprints.iter().filter(|f| f.bottom == bottom) {
        let Some(definition) = board.footprint_defs.get(&instance.footprint) else {
            continue;
        };
        for stroke in &definition.silk {
            let a = place_on_board(stroke[0], instance.at, instance.rotation, instance.bottom);
            let b = place_on_board(stroke[1], instance.at, instance.rotation, instance.bottom);
            out.stroke(a, b, SILK_WIDTH);
        }
        let Some((min, max)) = board.courtyard(instance) else {
            continue;
        };
        // The reference sits above the part, centred; mirrored on the bottom side.
        let anchor = Point::new((min.x + max.x) / 2, min.y - 300_000);
        for (a, b) in text_strokes(&instance.reference, anchor, TEXT_HEIGHT, bottom) {
            out.stroke(a, b, SILK_WIDTH);
        }
    }
    out.finish()
}

fn profile(board: &Board) -> String {
    let mut out = Gerber::new("Profile,NP", Polarity::Positive);
    let n = board.outline.len();
    for i in 0..n {
        out.stroke(board.outline[i], board.outline[(i + 1) % n], OUTLINE_WIDTH);
    }
    out.finish()
}

/// Coordinates in millimetres with 6 decimals, Y pointing up as in Gerber.
fn drill_coordinate(p: Point) -> String {
    format!(
        "X{:.6}Y{:.6}",
        p.x as f64 / MM as f64,
        -(p.y as f64) / MM as f64
    )
}

fn drill_file(holes: &[(Point, i64)], plated: bool, layers: u8) -> String {
    let mut tools: BTreeMap<i64, Vec<Point>> = BTreeMap::new();
    for (at, drill) in holes {
        tools.entry(*drill).or_default().push(*at);
    }
    let kind = if plated { "Plated" } else { "NonPlated" };
    let tag = if plated { "PTH" } else { "NPTH" };
    let mut out = String::from("M48\n");
    let _ = writeln!(out, "; TronicCAD {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "; #@! TF.FileFunction,{kind},1,{layers},{tag}");
    out.push_str("FMAT,2\nMETRIC,TZ\n");
    for (index, drill) in tools.keys().enumerate() {
        let _ = writeln!(out, "T{}C{:.3}", index + 1, *drill as f64 / MM as f64);
    }
    out.push_str("%\nG90\nG05\n");
    for (index, points) in tools.values().enumerate() {
        let _ = writeln!(out, "T{}", index + 1);
        for point in points {
            let _ = writeln!(out, "{}", drill_coordinate(*point));
        }
    }
    out.push_str("T0\nM30\n");
    out
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

fn bill_of_materials(board: &Board) -> String {
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for instance in board.footprints.iter() {
        let footprint = board
            .footprint_defs
            .get(&instance.footprint)
            .map_or(instance.footprint.clone(), |d| d.name.clone());
        groups
            .entry((instance.value.clone(), footprint))
            .or_default()
            .push(instance.reference.clone());
    }
    let mut out = String::from("Quantity,References,Value,Footprint\n");
    for ((value, footprint), mut references) in groups {
        references.sort();
        let _ = writeln!(
            out,
            "{},{},{},{}",
            references.len(),
            csv_field(&references.join(" ")),
            csv_field(&value),
            csv_field(&footprint)
        );
    }
    out
}

fn pick_and_place(board: &Board) -> String {
    let mut out = String::from("Reference,Value,Footprint,X_mm,Y_mm,Rotation,Side\n");
    let mut parts: Vec<&FootprintInstance> = board.footprints.iter().collect();
    parts.sort_by(|a, b| a.reference.cmp(&b.reference));
    for instance in parts {
        let footprint = board
            .footprint_defs
            .get(&instance.footprint)
            .map_or(instance.footprint.clone(), |d| d.name.clone());
        // Counter-clockwise, Y up, as placement machines expect.
        let rotation = match instance.rotation {
            Rotation::R0 => 0,
            Rotation::R90 => 270,
            Rotation::R180 => 180,
            Rotation::R270 => 90,
        };
        let _ = writeln!(
            out,
            "{},{},{},{:.4},{:.4},{},{}",
            csv_field(&instance.reference),
            csv_field(&instance.value),
            csv_field(&footprint),
            instance.at.x as f64 / MM as f64,
            -(instance.at.y as f64) / MM as f64,
            rotation,
            if instance.bottom { "bottom" } else { "top" },
        );
    }
    out
}

/// All manufacturing files of a board.
pub fn generate(board: &Board, project: &str) -> Vec<FabFile> {
    let pads = board.resolved_pads();
    let fills = fill_zones(board);
    let base: String = project
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut files = Vec::new();
    let mut add = |suffix: &str, content: String| {
        files.push(FabFile {
            name: format!("{base}-{suffix}"),
            content,
        });
    };
    for layer in 0..board.copper_layers {
        add(
            &format!("{}.gbr", copper_name(board, layer)),
            copper_layer(board, layer, &pads, &fills),
        );
    }
    add("mask-top.gbr", mask_layer(board, false, &pads));
    add("silk-top.gbr", silk_layer(board, false));
    add("paste-top.gbr", paste_layer(false, &pads));
    if board.copper_layers > 1 {
        add("mask-bottom.gbr", mask_layer(board, true, &pads));
        add("silk-bottom.gbr", silk_layer(board, true));
        add("paste-bottom.gbr", paste_layer(true, &pads));
    }
    add("outline.gbr", profile(board));

    let mut plated: Vec<(Point, i64)> = board.vias.iter().map(|v| (v.at, v.drill)).collect();
    let mut bare = Vec::new();
    for pad in &pads {
        if let Some(drill) = pad.drill {
            if pad.plated {
                plated.push((pad.at, drill));
            } else {
                bare.push((pad.at, drill));
            }
        }
    }
    add("plated.drl", drill_file(&plated, true, board.copper_layers));
    add(
        "nonplated.drl",
        drill_file(&bare, false, board.copper_layers),
    );
    add("bom.csv", bill_of_materials(board));
    add("placement.csv", pick_and_place(board));
    files
}
