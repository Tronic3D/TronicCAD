//! Computes the filled area of copper zones.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use serde::Serialize;
use tronic_schematic::{ElementId, Point};

use crate::copper::pad_shape;
use crate::geom::{in_polygons, polygon_is_invalid, Shape};
use crate::model::*;

/// One connected piece of a zone: outer contour first, then its holes.
pub type FillShape = Vec<Vec<Point>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ZoneFill {
    pub zone: ElementId,
    pub net: String,
    pub layer: u8,
    pub shapes: Vec<FillShape>,
}

fn contour(points: &[Point]) -> Vec<[f64; 2]> {
    points.iter().map(|p| [p.x as f64, p.y as f64]).collect()
}

fn counter_clockwise(mut polygon: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let n = polygon.len();
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            a[0] * b[1] - a[1] * b[0]
        })
        .sum();
    if area < 0.0 {
        polygon.reverse();
    }
    polygon
}

fn edge_keepouts(points: &[Point], margin: f64) -> Vec<Vec<[f64; 2]>> {
    (0..points.len())
        .map(|i| {
            let shape = Shape::track(points[i], points[(i + 1) % points.len()], 0);
            counter_clockwise(shape.polygon(margin))
        })
        .collect()
}

/// Fills all zones in creation order; later zones keep clear of earlier ones.
pub fn fill_zones(board: &Board) -> Vec<ZoneFill> {
    let pads = board.resolved_pads();
    let clearance = board.rules.min_clearance as f64;
    let mut fills = Vec::with_capacity(board.zones.len());

    for (index, zone) in board.zones.iter().enumerate() {
        let empty = |zone: &Zone| ZoneFill {
            zone: zone.id,
            net: zone.net.clone(),
            layer: zone.layer,
            shapes: Vec::new(),
        };
        if polygon_is_invalid(&zone.outline) || polygon_is_invalid(&board.outline) {
            fills.push(empty(zone));
            continue;
        }

        let subject = vec![counter_clockwise(contour(&zone.outline))];
        let board_area = vec![counter_clockwise(contour(&board.outline))];
        let inside = subject.overlay(&board_area, OverlayRule::Intersect, FillRule::NonZero);
        let subject: Vec<Vec<[f64; 2]>> = inside.into_iter().flatten().collect();
        if subject.is_empty() {
            fills.push(empty(zone));
            continue;
        }

        // Everything that must stay free of this zone.
        let mut obstacles = edge_keepouts(&board.outline, board.rules.edge_clearance as f64);
        let mut own_points: Vec<(f64, f64)> = Vec::new();
        for pad in &pads {
            let on_layer = pad.layer.is_none_or(|layer| layer == zone.layer);
            if !on_layer {
                continue;
            }
            let shape = pad_shape(pad);
            let same_net = pad.net.as_deref() == Some(zone.net.as_str()) && pad.plated;
            if same_net {
                own_points.push(shape.center());
            } else {
                let margin = clearance;
                obstacles.push(counter_clockwise(shape.polygon(margin)));
            }
        }
        for track in board.tracks.iter().filter(|t| t.layer == zone.layer) {
            if track.net == zone.net {
                own_points.push((track.a.x as f64, track.a.y as f64));
                own_points.push((track.b.x as f64, track.b.y as f64));
            } else {
                let shape = Shape::track(track.a, track.b, track.width);
                obstacles.push(counter_clockwise(shape.polygon(clearance)));
            }
        }
        for via in board.vias.iter() {
            if via.net == zone.net {
                own_points.push((via.at.x as f64, via.at.y as f64));
            } else {
                let shape = Shape::circle(via.at, via.diameter);
                obstacles.push(counter_clockwise(shape.polygon(clearance)));
            }
        }
        for earlier in board.zones[..index]
            .iter()
            .filter(|z| z.layer == zone.layer && z.net != zone.net)
        {
            if !polygon_is_invalid(&earlier.outline) {
                obstacles.push(counter_clockwise(contour(&earlier.outline)));
                obstacles.extend(edge_keepouts(&earlier.outline, clearance));
            }
        }

        let result = subject.overlay(&obstacles, OverlayRule::Difference, FillRule::NonZero);

        // Islands without a connection to their own net are dropped.
        let mut shapes = Vec::new();
        for shape in result {
            if !own_points.iter().any(|p| in_polygons(*p, &shape)) {
                continue;
            }
            shapes.push(
                shape
                    .into_iter()
                    .map(|contour| {
                        contour
                            .into_iter()
                            .map(|[x, y]| Point::new(x.round() as i64, y.round() as i64))
                            .collect()
                    })
                    .collect(),
            );
        }
        fills.push(ZoneFill {
            zone: zone.id,
            net: zone.net.clone(),
            layer: zone.layer,
            shapes,
        });
    }
    fills
}
