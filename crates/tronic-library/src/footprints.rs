//! TronicCAD's own footprints, drawn from common package dimensions.
//!
//! The dimensions are typical values and have not been checked against
//! manufacturer drawings or IPC-7351 by a person, which `reviewed: false`
//! states. Check them before ordering boards.

use tronic_pcb::{BodyKind, BodyPart, FootprintDef, PadDef, PadShape};
use tronic_schematic::Point;

use crate::draw::provenance;

const UM: i64 = 1000;

fn pt(x_um: i64, y_um: i64) -> Point {
    Point::new(x_um * UM, y_um * UM)
}

fn smd(number: &str, x: i64, y: i64, w: i64, h: i64) -> PadDef {
    PadDef {
        number: number.to_owned(),
        at: pt(x, y),
        size: pt(w, h),
        shape: PadShape::Rect,
        drill: None,
        plated: true,
    }
}

fn tht(number: &str, x: i64, y: i64, diameter: i64, drill: i64, square: bool) -> PadDef {
    PadDef {
        number: number.to_owned(),
        at: pt(x, y),
        size: pt(diameter, diameter),
        shape: if square {
            PadShape::Rect
        } else {
            PadShape::Round
        },
        drill: Some(drill * UM),
        plated: true,
    }
}

fn rect_silk(half_w: i64, half_h: i64) -> Vec<[Point; 2]> {
    let (a, b, c, d) = (
        pt(-half_w, -half_h),
        pt(half_w, -half_h),
        pt(half_w, half_h),
        pt(-half_w, half_h),
    );
    vec![[a, b], [b, c], [c, d], [d, a]]
}

fn footprint(
    id: &str,
    name: &str,
    pads: Vec<PadDef>,
    silk: Vec<[Point; 2]>,
    half_w: i64,
    half_h: i64,
) -> FootprintDef {
    FootprintDef {
        id: id.to_owned(),
        name: name.to_owned(),
        pads,
        silk,
        courtyard: [pt(-half_w, -half_h), pt(half_w, half_h)],
        provenance: provenance(),
        body: Vec::new(),
        model: None,
    }
}

/// Two-pad chip part.
fn chip(id: &str, name: &str, pitch: i64, w: i64, h: i64, body_h: i64) -> FootprintDef {
    let half = pitch / 2;
    let silk_x = (half - w / 2 - 150).max(0);
    let silk_y = body_h / 2 + 150;
    let silk = vec![
        [pt(-silk_x, -silk_y), pt(silk_x, -silk_y)],
        [pt(-silk_x, silk_y), pt(silk_x, silk_y)],
    ];
    footprint(
        id,
        name,
        vec![smd("1", -half, 0, w, h), smd("2", half, 0, w, h)],
        silk,
        half + w / 2 + 250,
        h / 2 + 250,
    )
}

/// Small outline package with pins on two sides, numbered counter-clockwise.
fn soic(id: &str, name: &str, pins: usize, pitch: i64, span: i64, w: i64, h: i64) -> FootprintDef {
    let rows = pins / 2;
    let top = -(pitch * (rows as i64 - 1)) / 2;
    let mut pads = Vec::with_capacity(pins);
    for row in 0..rows {
        let y = top + pitch * row as i64;
        pads.push(smd(&(row + 1).to_string(), -span / 2, y, w, h));
    }
    for row in 0..rows {
        let y = -top - pitch * row as i64;
        pads.push(smd(&(rows + row + 1).to_string(), span / 2, y, w, h));
    }
    let body_x = span / 2 - w / 2 - 150;
    let body_y = -top + h / 2 + 300;
    let mut silk = rect_silk(body_x, body_y);
    // Pin 1 mark.
    silk.push([pt(-body_x, -body_y + 500), pt(-body_x + 500, -body_y)]);
    footprint(id, name, pads, silk, span / 2 + w / 2 + 250, body_y + 250)
}

/// Dual inline package, 2.54 mm pitch.
fn dip(id: &str, name: &str, pins: usize, span: i64) -> FootprintDef {
    let rows = pins / 2;
    let pitch = 2540;
    let top = -(pitch * (rows as i64 - 1)) / 2;
    let mut pads = Vec::with_capacity(pins);
    for row in 0..rows {
        pads.push(tht(
            &(row + 1).to_string(),
            -span / 2,
            top + pitch * row as i64,
            1600,
            800,
            row == 0,
        ));
    }
    for row in 0..rows {
        pads.push(tht(
            &(rows + row + 1).to_string(),
            span / 2,
            -top - pitch * row as i64,
            1600,
            800,
            false,
        ));
    }
    let body_x = span / 2 - 1100;
    let body_y = -top + 1300;
    footprint(
        id,
        name,
        pads,
        rect_silk(body_x, body_y),
        span / 2 + 1050,
        body_y + 250,
    )
}

/// Pin header, one row, 2.54 mm pitch. Pin 1 is square.
fn header(pins: usize) -> FootprintDef {
    let pitch = 2540;
    let top = -(pitch * (pins as i64 - 1)) / 2;
    let pads = (0..pins)
        .map(|n| {
            tht(
                &(n + 1).to_string(),
                0,
                top + pitch * n as i64,
                1700,
                1000,
                n == 0,
            )
        })
        .collect();
    footprint(
        &format!("header:1x{pins:02}"),
        &format!("Header 1x{pins:02} 2.54 mm"),
        pads,
        rect_silk(1300, -top + 1300),
        1550,
        -top + 1550,
    )
}

/// How far a through-hole pin reaches below the mounting surface: a 1.6 mm board
/// and the usual 1.4 mm of cut lead.
const PIN_BELOW: i64 = 3000;
const BLACK: [u8; 3] = [32, 32, 36];
const TAN: [u8; 3] = [186, 150, 96];
const METAL: [u8; 3] = [190, 190, 196];
const WHITE: [u8; 3] = [226, 226, 216];
const RED: [u8; 3] = [200, 40, 36];

fn body(kind: BodyKind, center: [i64; 2], size: [i64; 2], z: [i64; 2], color: [u8; 3]) -> BodyPart {
    BodyPart {
        kind,
        center: pt(center[0], center[1]),
        size: pt(size[0], size[1]),
        z: z[0] * UM,
        height: z[1] * UM,
        color,
    }
}

fn cuboid(x: i64, y: i64, w: i64, h: i64, z: i64, height: i64, color: [u8; 3]) -> BodyPart {
    body(BodyKind::Box, [x, y], [w, h], [z, height], color)
}

/// Typical shapes, not the exact outline of any manufacturer's part.
fn add_bodies(footprint: &mut FootprintDef) {
    let pads = footprint.pads.clone();
    let pins = |height: i64, side: i64| -> Vec<BodyPart> {
        pads.iter()
            .map(|pad| {
                cuboid(
                    pad.at.x / UM,
                    pad.at.y / UM,
                    side,
                    side,
                    -PIN_BELOW,
                    height + PIN_BELOW,
                    METAL,
                )
            })
            .collect()
    };
    let id = footprint.id.clone();
    let parts: Vec<BodyPart> = match id.as_str() {
        "chip:0603" => vec![cuboid(0, 0, 1600, 800, 0, 500, TAN)],
        "chip:0805" => vec![cuboid(0, 0, 2000, 1250, 0, 600, TAN)],
        "chip:1206" => vec![cuboid(0, 0, 3200, 1600, 0, 700, TAN)],
        "sot:23" => vec![cuboid(0, 0, 2900, 1300, 0, 1100, BLACK)],
        "sot:223" => vec![cuboid(0, 0, 6500, 3500, 0, 1800, BLACK)],
        "to:220_vertical" => {
            let mut parts = vec![
                cuboid(0, 0, 10000, 4500, 3000, 9000, BLACK),
                cuboid(0, 1500, 10000, 1300, 12000, 6500, METAL),
            ];
            parts.extend(pins(3000, 700));
            parts
        }
        "led:5mm" => {
            let mut parts = vec![body(
                BodyKind::Cylinder,
                [0, 0],
                [5000, 5000],
                [1000, 8000],
                RED,
            )];
            parts.extend(pins(1000, 500));
            parts
        }
        "hole:m3" => Vec::new(),
        other if other.starts_with("soic:") => {
            let [min, max] = footprint.courtyard;
            let half_h = (max.y - min.y) / UM / 2 - 250;
            let width = if other == "soic:24w" { 7500 } else { 3900 };
            let mut parts = vec![cuboid(0, 0, width, 2 * half_h, 150, 1500, BLACK)];
            parts.extend(pads.iter().map(|pad| {
                cuboid(
                    pad.at.x / UM,
                    pad.at.y / UM,
                    pad.size.x / UM,
                    400,
                    0,
                    150,
                    METAL,
                )
            }));
            parts
        }
        other if other.starts_with("dip:") => {
            let [min, max] = footprint.courtyard;
            let half_h = (max.y - min.y) / UM / 2 - 250;
            let mut parts = vec![cuboid(0, 0, 6350, 2 * half_h, 500, 3500, BLACK)];
            parts.extend(pins(500, 500));
            parts
        }
        other if other.starts_with("header:") => {
            let [min, max] = footprint.courtyard;
            let half_h = (max.y - min.y) / UM / 2 - 250;
            let mut parts = vec![cuboid(0, 0, 2540, 2 * half_h, 0, 2540, BLACK)];
            parts.extend(pins(8500, 640));
            parts
        }
        _ => vec![cuboid(0, 0, 2000, 2000, 0, 1000, WHITE)],
    };
    footprint.body = parts;
}

/// All built-in footprints.
pub fn footprints() -> Vec<FootprintDef> {
    let mut list = vec![
        chip("chip:0603", "Chip 0603", 1600, 900, 950, 800),
        chip("chip:0805", "Chip 0805", 1900, 1000, 1300, 1250),
        chip("chip:1206", "Chip 1206", 3000, 1150, 1700, 1600),
        footprint(
            "sot:23",
            "SOT-23",
            vec![
                smd("1", -950, 1050, 800, 900),
                smd("2", 950, 1050, 800, 900),
                smd("3", 0, -1050, 800, 900),
            ],
            rect_silk(1500, 700),
            1700,
            1750,
        ),
        footprint(
            "sot:223",
            "SOT-223",
            vec![
                smd("1", -2300, 3150, 1000, 1800),
                smd("2", 0, 3150, 1000, 1800),
                smd("3", 2300, 3150, 1000, 1800),
                smd("4", 0, -3150, 3300, 1800),
            ],
            rect_silk(3400, 1900),
            3600,
            4300,
        ),
        footprint(
            "to:220_vertical",
            "TO-220 vertical",
            vec![
                tht("1", -2540, 0, 1800, 1100, true),
                tht("2", 0, 0, 1800, 1100, false),
                tht("3", 2540, 0, 1800, 1100, false),
            ],
            rect_silk(5200, 2500),
            5400,
            2700,
        ),
        footprint(
            "led:5mm",
            "LED 5 mm",
            vec![
                tht("1", -1270, 0, 1800, 900, true),
                tht("2", 1270, 0, 1800, 900, false),
            ],
            rect_silk(2800, 2800),
            3000,
            3000,
        ),
        footprint(
            "hole:m3",
            "Mounting hole M3",
            vec![PadDef {
                number: String::new(),
                at: pt(0, 0),
                size: pt(3200, 3200),
                shape: PadShape::Round,
                drill: Some(3200 * UM),
                plated: false,
            }],
            Vec::new(),
            3000,
            3000,
        ),
    ];
    for (pins, id, name) in [
        (8, "soic:8", "SOIC-8"),
        (14, "soic:14", "SOIC-14"),
        (16, "soic:16", "SOIC-16"),
    ] {
        list.push(soic(id, name, pins, 1270, 5400, 1500, 600));
    }
    list.push(soic("soic:24w", "SOIC-24 wide", 24, 1270, 9300, 2000, 600));
    for (pins, id, name) in [
        (8, "dip:8", "DIP-8"),
        (14, "dip:14", "DIP-14"),
        (16, "dip:16", "DIP-16"),
    ] {
        list.push(dip(id, name, pins, 7620));
    }
    for pins in [2, 3, 4, 6, 8, 10, 16] {
        list.push(header(pins));
    }
    for footprint in &mut list {
        add_bodies(footprint);
    }
    list
}

/// A footprint by id.
pub fn find_footprint(id: &str) -> Option<FootprintDef> {
    footprints()
        .into_iter()
        .find(|footprint| footprint.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin;
    use std::collections::HashSet;
    use tronic_pcb::geom::Shape;

    fn shape(pad: &PadDef) -> Shape {
        match pad.shape {
            PadShape::Rect => Shape::rect(pad.at, pad.size),
            PadShape::Round => Shape::circle(pad.at, pad.size.x),
            PadShape::Oblong => Shape::oblong(pad.at, pad.size),
        }
    }

    #[test]
    fn ids_are_unique_and_every_footprint_has_provenance() {
        let mut ids = HashSet::new();
        for footprint in footprints() {
            assert!(ids.insert(footprint.id.clone()), "{}", footprint.id);
            assert!(!footprint.provenance.license.is_empty());
            assert!(!footprint.provenance.reviewed, "{}", footprint.id);
        }
    }

    #[test]
    fn pads_keep_the_default_clearance_from_each_other() {
        // Default rule: 0.2 mm. Packages tighter than that would always fail.
        for footprint in footprints() {
            for (i, a) in footprint.pads.iter().enumerate() {
                for b in &footprint.pads[i + 1..] {
                    let gap = shape(a).gap(&shape(b));
                    assert!(
                        gap >= 200_000.0,
                        "{}: pads {} and {} are {} nm apart",
                        footprint.id,
                        a.number,
                        b.number,
                        gap
                    );
                }
            }
        }
    }

    #[test]
    fn pads_lie_inside_the_courtyard() {
        for footprint in footprints() {
            let [min, max] = footprint.courtyard;
            for pad in &footprint.pads {
                let (x0, y0, x1, y1) = shape(pad).bounds();
                assert!(
                    x0 >= min.x as f64
                        && y0 >= min.y as f64
                        && x1 <= max.x as f64
                        && y1 <= max.y as f64,
                    "{} pad {}",
                    footprint.id,
                    pad.number
                );
            }
        }
    }

    #[test]
    fn every_part_except_a_bare_hole_has_a_body_with_its_pads_under_it() {
        for footprint in footprints() {
            if footprint.id == "hole:m3" {
                assert!(footprint.body.is_empty());
                continue;
            }
            assert!(!footprint.body.is_empty(), "{}", footprint.id);
            for part in &footprint.body {
                assert!(
                    part.size.x > 0 && part.size.y > 0 && part.height > 0,
                    "{}",
                    footprint.id
                );
            }
        }
    }

    #[test]
    fn dip_pins_are_numbered_counter_clockwise() {
        let dip8 = find_footprint("dip:8").unwrap_or_else(|| footprints().remove(0));
        let pad = |n: &str| dip8.pads.iter().find(|p| p.number == n).map(|p| p.at);
        assert_eq!(pad("1").map(|p| p.x), Some(-3_810_000));
        assert_eq!(pad("4").map(|p| p.x), Some(-3_810_000));
        assert_eq!(pad("5").map(|p| p.x), Some(3_810_000));
        // Pin 8 sits opposite pin 1, pin 5 opposite pin 4.
        assert_eq!(pad("8").map(|p| p.y), pad("1").map(|p| p.y));
        assert_eq!(pad("5").map(|p| p.y), pad("4").map(|p| p.y));
        assert_eq!(
            pad("2").map(|p| p.y - pad("1").map_or(0, |q| q.y)),
            Some(2_540_000)
        );
    }

    #[test]
    fn every_default_footprint_of_a_symbol_exists_and_has_all_its_pins() {
        for symbol in builtin() {
            let Some(id) = &symbol.part.footprint else {
                continue;
            };
            let footprint = find_footprint(id).unwrap_or_else(|| panic_missing(&symbol.id, id));
            for pin in &symbol.pins {
                assert!(
                    footprint.pads.iter().any(|pad| pad.number == pin.number),
                    "{}: no pad {} in {}",
                    symbol.id,
                    pin.number,
                    id
                );
            }
        }
    }

    fn panic_missing(symbol: &str, footprint: &str) -> FootprintDef {
        assert!(symbol.is_empty(), "{symbol}: footprint {footprint} missing");
        footprints().remove(0)
    }

    #[test]
    fn a_millimetre_is_what_the_board_crate_says() {
        assert_eq!(tronic_pcb::MM, 1000 * UM);
    }
}
