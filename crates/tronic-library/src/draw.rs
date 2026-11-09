//! Helpers to draw symbols. Coordinates are in grid units unless noted.

use serde::Deserialize;
use tronic_schematic::{
    Graphic, PartInfo, PinDef, PinDirection, PinKind, Point, Provenance, SymbolDef, SymbolKind,
    GRID,
};

pub const SOURCE: &str = "TronicCAD built-in library";
pub const AUTHOR: &str = "TronicCAD project";
pub const LICENSE: &str = "TronicCAD proprietary";

pub fn p(x: f64, y: f64) -> Point {
    Point::new((x * GRID as f64) as i64, (y * GRID as f64) as i64)
}

pub fn line(points: &[(f64, f64)]) -> Graphic {
    Graphic::Line {
        points: points.iter().map(|&(x, y)| p(x, y)).collect(),
    }
}

pub fn polygon(points: &[(f64, f64)], filled: bool) -> Graphic {
    Graphic::Polygon {
        points: points.iter().map(|&(x, y)| p(x, y)).collect(),
        filled,
    }
}

pub fn rect(a: (f64, f64), b: (f64, f64)) -> Graphic {
    Graphic::Rect {
        a: p(a.0, a.1),
        b: p(b.0, b.1),
    }
}

pub fn circle(center: (f64, f64), radius: f64, filled: bool) -> Graphic {
    Graphic::Circle {
        center: p(center.0, center.1),
        radius: (radius * GRID as f64) as i64,
        filled,
    }
}

pub fn arc(center: (f64, f64), radius: f64, start_deg: i32, end_deg: i32) -> Graphic {
    Graphic::Arc {
        center: p(center.0, center.1),
        radius: (radius * GRID as f64) as i64,
        start_deg,
        end_deg,
    }
}

pub fn text(at: (f64, f64), value: &str) -> Graphic {
    Graphic::Text {
        at: p(at.0, at.1),
        text: value.to_owned(),
        size: GRID,
    }
}

/// A pin given by connection point, direction into the body and length.
pub fn pin(
    number: &str,
    name: &str,
    kind: PinKind,
    at: (i64, i64),
    direction: PinDirection,
    length: i64,
) -> PinDef {
    PinDef {
        number: number.to_owned(),
        name: name.to_owned(),
        kind,
        at: Point::grid(at.0, at.1),
        direction,
        length: length * GRID,
        hide_name: true,
        hide_number: false,
    }
}

pub fn provenance() -> Provenance {
    Provenance {
        source: SOURCE.to_owned(),
        author: AUTHOR.to_owned(),
        license: LICENSE.to_owned(),
        reviewed: false,
    }
}

/// Everything but the drawing of a built-in symbol.
pub struct Meta<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub category: &'a str,
    pub prefix: &'a str,
    pub value: &'a str,
}

pub fn symbol(meta: Meta<'_>, graphics: Vec<Graphic>, pins: Vec<PinDef>) -> SymbolDef {
    SymbolDef {
        id: meta.id.to_owned(),
        name: meta.name.to_owned(),
        category: meta.category.to_owned(),
        kind: SymbolKind::Part,
        reference_prefix: meta.prefix.to_owned(),
        default_value: meta.value.to_owned(),
        description: String::new(),
        graphics,
        pins,
        part: PartInfo::default(),
        provenance: provenance(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinSide {
    Left,
    Right,
}

/// One pin in the description of a custom part.
#[derive(Debug, Clone, Deserialize)]
pub struct SpecPin {
    pub number: String,
    pub name: String,
    pub kind: PinKind,
    pub side: PinSide,
}

/// A user's description of a rectangular part.
#[derive(Debug, Clone, Deserialize)]
pub struct CustomPartSpec {
    pub id: String,
    pub name: String,
    pub category: String,
    pub reference_prefix: String,
    pub value: String,
    pub pins: Vec<SpecPin>,
    #[serde(default)]
    pub provenance: Option<Provenance>,
}

/// Draws a part as a box with pins on the left and right side.
pub fn rectangular_part(spec: &CustomPartSpec) -> SymbolDef {
    let (left, right): (Vec<&SpecPin>, Vec<&SpecPin>) =
        spec.pins.iter().partition(|pin| pin.side == PinSide::Left);
    let rows = left.len().max(right.len()).max(1) as i64;
    let half_width = rows.clamp(4, 6) * 2;
    // Pins sit on even grid lines so that they line up with other parts.
    let top = -rows + 1;
    let place = |row: usize| top + 2 * row as i64;
    let mut pins = Vec::with_capacity(spec.pins.len());
    let mut add = |list: &[&SpecPin], x: i64, direction: PinDirection| {
        for (row, definition) in list.iter().enumerate() {
            pins.push(PinDef {
                number: definition.number.clone(),
                name: definition.name.clone(),
                kind: definition.kind,
                at: Point::grid(x, place(row)),
                direction,
                length: 3 * GRID,
                hide_name: false,
                hide_number: false,
            });
        }
    };
    add(&left, -half_width - 3, PinDirection::Right);
    add(&right, half_width + 3, PinDirection::Left);
    let bottom = top + 2 * (rows - 1);
    let graphics = vec![Graphic::Rect {
        a: Point::grid(-half_width, top - 2),
        b: Point::grid(half_width, bottom + 2),
    }];
    SymbolDef {
        id: spec.id.clone(),
        name: spec.name.clone(),
        category: spec.category.clone(),
        kind: SymbolKind::Part,
        reference_prefix: spec.reference_prefix.clone(),
        default_value: spec.value.clone(),
        description: String::new(),
        graphics,
        pins,
        part: PartInfo::default(),
        provenance: spec.provenance.clone().unwrap_or_else(provenance),
    }
}
