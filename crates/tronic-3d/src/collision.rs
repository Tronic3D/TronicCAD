//! Mechanical checks: collisions, clearances and height limits.
//!
//! Parts are compared as axis-parallel boxes: each body part of a footprint, or
//! the bounding box of an imported model. That is exact for the built-in bodies
//! and conservative for imported models.

use serde::Serialize;
use std::collections::BTreeMap;
use tronic_pcb::{Board, FootprintInstance, MM};
use tronic_schematic::ElementId;

use crate::scene::{fallback_box, world_box, WorldBox};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    /// Stable identifier, also the key of the translated message.
    pub code: &'static str,
    pub elements: Vec<ElementId>,
    pub params: BTreeMap<&'static str, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartHeight {
    pub id: ElementId,
    pub reference: String,
    pub bottom: bool,
    /// Height above the mounting surface, in millimetres.
    pub height: f64,
    pub has_model: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct MechanicsReport {
    pub findings: Vec<Finding>,
    pub heights: Vec<PartHeight>,
    pub max_top: f64,
    pub max_bottom: f64,
}

fn boxes_of(board: &Board, instance: &FootprintInstance, thickness: f64) -> Vec<WorldBox> {
    let Some(definition) = board.footprint_defs.get(&instance.footprint) else {
        return Vec::new();
    };
    if let Some(model) = &definition.model {
        return vec![model_box(instance, thickness, model)];
    }
    if definition.body.is_empty() {
        return fallback_box(board, instance, thickness)
            .into_iter()
            .collect();
    }
    definition
        .body
        .iter()
        .map(|body| world_box(instance, thickness, body))
        .collect()
}

/// The bounding box of an imported model on the board.
fn model_box(
    instance: &FootprintInstance,
    thickness: f64,
    model: &tronic_pcb::ImportedModel,
) -> WorldBox {
    let (mut min, mut max) = ([f64::MAX; 3], [f64::MIN; 3]);
    let (ox, oy) = (
        instance.at.x as f64 / MM as f64,
        instance.at.y as f64 / MM as f64,
    );
    let scale = f64::from(model.scale);
    for p in model.positions.as_chunks::<3>().0 {
        let x = f64::from(p[0]) * scale;
        let y = -f64::from(p[1]) * scale;
        let z = f64::from(p[2]) * scale + f64::from(model.z_offset);
        let mx = if instance.bottom { -x } else { x };
        let (tx, ty) = match instance.rotation {
            tronic_schematic::Rotation::R0 => (mx, y),
            tronic_schematic::Rotation::R90 => (-y, mx),
            tronic_schematic::Rotation::R180 => (-mx, -y),
            tronic_schematic::Rotation::R270 => (y, -mx),
        };
        let corner = [
            ox + tx,
            -(oy + ty),
            if instance.bottom { -thickness - z } else { z },
        ];
        for axis in 0..3 {
            min[axis] = min[axis].min(corner[axis]);
            max[axis] = max[axis].max(corner[axis]);
        }
    }
    WorldBox { min, max }
}

/// The smallest distance between two boxes; zero or less means they overlap.
fn gap(a: &WorldBox, b: &WorldBox) -> f64 {
    let mut outside = 0.0f64;
    let mut inside = f64::MIN;
    for axis in 0..3 {
        let separation = (b.min[axis] - a.max[axis]).max(a.min[axis] - b.max[axis]);
        if separation > 0.0 {
            outside += separation * separation;
        }
        inside = inside.max(separation);
    }
    if outside > 0.0 {
        outside.sqrt()
    } else {
        inside
    }
}

/// Removes floating-point noise: lengths on a board are whole nanometres.
fn to_nanometres(value: f64) -> f64 {
    (value * 1e6).round() / 1e6
}

fn number(value: f64) -> String {
    format!("{value:.2}")
}

/// Checks all parts of a board.
pub fn check_mechanics(board: &Board) -> MechanicsReport {
    let rules = &board.mechanics;
    let thickness = rules.thickness as f64 / MM as f64;
    let clearance = rules.part_clearance as f64 / MM as f64;
    let mut report = MechanicsReport::default();
    let parts: Vec<(&FootprintInstance, Vec<WorldBox>)> = board
        .footprints
        .iter()
        .map(|instance| (instance, boxes_of(board, instance, thickness)))
        .collect();

    for (instance, boxes) in &parts {
        let Some(definition) = board.footprint_defs.get(&instance.footprint) else {
            continue;
        };
        let height = boxes
            .iter()
            .map(|b| {
                if instance.bottom {
                    -thickness - b.min[2]
                } else {
                    b.max[2]
                }
            })
            .fold(0.0, f64::max);
        let height = to_nanometres(height);
        let has_model = definition.model.is_some() || !definition.body.is_empty();
        report.heights.push(PartHeight {
            id: instance.id,
            reference: instance.reference.clone(),
            bottom: instance.bottom,
            height,
            has_model,
        });
        if instance.bottom {
            report.max_bottom = report.max_bottom.max(height);
        } else {
            report.max_top = report.max_top.max(height);
        }
        if !has_model {
            report.findings.push(Finding {
                code: "no_model",
                elements: vec![instance.id],
                params: BTreeMap::from([("reference", instance.reference.clone())]),
            });
        }
        let limit = if instance.bottom {
            rules.max_height_bottom
        } else {
            rules.max_height_top
        };
        if let Some(limit) = limit {
            let limit = limit as f64 / MM as f64;
            if height > limit {
                report.findings.push(Finding {
                    code: "too_high",
                    elements: vec![instance.id],
                    params: BTreeMap::from([
                        ("reference", instance.reference.clone()),
                        ("actual", number(height)),
                        ("limit", number(limit)),
                    ]),
                });
            }
        }
    }

    for (index, (a, boxes_a)) in parts.iter().enumerate() {
        for (b, boxes_b) in &parts[index + 1..] {
            let mut smallest = f64::MAX;
            for first in boxes_a {
                for second in boxes_b {
                    smallest = smallest.min(gap(first, second));
                }
            }
            if smallest == f64::MAX {
                continue;
            }
            let smallest = to_nanometres(smallest);
            let references = [("a", a.reference.clone()), ("b", b.reference.clone())];
            if smallest <= 0.0 {
                report.findings.push(Finding {
                    code: "collision",
                    elements: vec![a.id, b.id],
                    params: references.into_iter().collect(),
                });
            } else if smallest < clearance {
                let mut params: BTreeMap<&'static str, String> = references.into_iter().collect();
                params.insert("actual", number(smallest));
                params.insert("required", number(clearance));
                report.findings.push(Finding {
                    code: "too_close",
                    elements: vec![a.id, b.id],
                    params,
                });
            }
        }
    }
    report
}
