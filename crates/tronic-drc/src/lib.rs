//! Design rule check. Rules and severities: `docs/specs/pcb.md`.

use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use tronic_pcb::copper::{analyze, copper_items, pad_shape, CopperItem};
use tronic_pcb::geom::polygon_is_invalid;
use tronic_pcb::*;
use tronic_schematic::{ElementId, Point};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Violation {
    /// Stable identifier, also the key of the translated message.
    pub code: &'static str,
    pub severity: Severity,
    pub at: Point,
    pub layer: Option<u8>,
    pub elements: Vec<ElementId>,
    pub params: BTreeMap<&'static str, String>,
}

struct Report(Vec<Violation>);

impl Report {
    fn add(
        &mut self,
        code: &'static str,
        severity: Severity,
        at: Point,
        layer: Option<u8>,
        elements: Vec<ElementId>,
        params: &[(&'static str, String)],
    ) {
        self.0.push(Violation {
            code,
            severity,
            at,
            layer,
            elements,
            params: params.iter().cloned().collect(),
        });
    }
}

/// A length for messages, in millimetres.
fn mm(nm: f64) -> String {
    format!("{:.3}", nm / 1_000_000.0)
}

fn point_of(p: (f64, f64)) -> Point {
    Point::new(p.0.round() as i64, p.1.round() as i64)
}

/// Checks a board. The result is deterministic.
pub fn check(board: &Board) -> Vec<Violation> {
    let mut report = Report(Vec::new());
    check_outline(board, &mut report);
    check_copper_spacing(board, &mut report);
    check_sizes(board, &mut report);
    check_holes(board, &mut report);
    check_edge(board, &mut report);
    check_placement(board, &mut report);
    check_nets(board, &mut report);
    report.0.sort_by(|a, b| {
        (a.severity, a.at, a.code, &a.elements).cmp(&(b.severity, b.at, b.code, &b.elements))
    });
    report.0.dedup();
    report.0
}

fn check_outline(board: &Board, report: &mut Report) {
    if board.outline.is_empty() {
        report.add(
            "outline_missing",
            Severity::Error,
            Point::default(),
            None,
            vec![],
            &[],
        );
    } else if polygon_is_invalid(&board.outline) {
        report.add(
            "outline_invalid",
            Severity::Error,
            board.outline[0],
            None,
            vec![],
            &[],
        );
    }
}

fn check_copper_spacing(board: &Board, report: &mut Report) {
    let pads = board.resolved_pads();
    let items = copper_items(board, &pads);
    let clearance = board.rules.min_clearance as f64;
    let mut grid = Grid::new(5_000_000.0);
    for (index, item) in items.iter().enumerate() {
        grid.insert(index, item.shape.bounds());
    }
    let shared_layer = |a: &CopperItem, b: &CopperItem| a.layer.or(b.layer);
    for (index, item) in items.iter().enumerate() {
        let (x0, y0, x1, y1) = item.shape.bounds();
        let search = (
            x0 - clearance,
            y0 - clearance,
            x1 + clearance,
            y1 + clearance,
        );
        for other in grid.query(search) {
            if other <= index {
                continue;
            }
            let candidate = &items[other];
            if !item.shares_layer(candidate) {
                continue;
            }
            // Pads without a net never conflict with their own footprint's copper.
            if item.net == candidate.net && item.net.is_some() {
                continue;
            }
            if item.element() == candidate.element() {
                continue;
            }
            let gap = item.shape.gap(&candidate.shape);
            if gap >= clearance {
                continue;
            }
            let center_a = item.shape.center();
            let center_b = candidate.shape.center();
            let at = point_of((
                (center_a.0 + center_b.0) / 2.0,
                (center_a.1 + center_b.1) / 2.0,
            ));
            let net_a = item.net.clone().unwrap_or_default();
            let net_b = candidate.net.clone().unwrap_or_default();
            let elements = vec![item.element(), candidate.element()];
            let layer = shared_layer(item, candidate);
            if gap <= 0.0 {
                report.add(
                    "short",
                    Severity::Error,
                    at,
                    layer,
                    elements,
                    &[("a", net_a), ("b", net_b)],
                );
            } else {
                report.add(
                    "clearance",
                    Severity::Error,
                    at,
                    layer,
                    elements,
                    &[
                        ("a", net_a),
                        ("b", net_b),
                        ("actual", mm(gap)),
                        ("required", mm(clearance)),
                    ],
                );
            }
        }
    }
}

fn check_sizes(board: &Board, report: &mut Report) {
    let rules = &board.rules;
    for track in board.tracks.iter() {
        if track.width < rules.min_track_width {
            report.add(
                "track_width",
                Severity::Error,
                track.a,
                Some(track.layer),
                vec![track.id],
                &[
                    ("actual", mm(track.width as f64)),
                    ("required", mm(rules.min_track_width as f64)),
                ],
            );
        }
        if track.layer >= board.copper_layers {
            report.add(
                "invalid_layer",
                Severity::Error,
                track.a,
                Some(track.layer),
                vec![track.id],
                &[("layer", (track.layer + 1).to_string())],
            );
        }
    }
    for zone in board.zones.iter() {
        if zone.layer >= board.copper_layers {
            report.add(
                "invalid_layer",
                Severity::Error,
                zone.outline.first().copied().unwrap_or_default(),
                Some(zone.layer),
                vec![zone.id],
                &[("layer", (zone.layer + 1).to_string())],
            );
        }
    }
    for via in board.vias.iter() {
        if via.drill < rules.min_drill {
            report.add(
                "via_drill",
                Severity::Error,
                via.at,
                None,
                vec![via.id],
                &[
                    ("actual", mm(via.drill as f64)),
                    ("required", mm(rules.min_drill as f64)),
                ],
            );
        }
        let ring = (via.diameter - via.drill) as f64 / 2.0;
        if ring < rules.min_annular_ring as f64 {
            report.add(
                "annular_ring",
                Severity::Error,
                via.at,
                None,
                vec![via.id],
                &[
                    ("actual", mm(ring)),
                    ("required", mm(rules.min_annular_ring as f64)),
                ],
            );
        }
    }
}

fn check_holes(board: &Board, report: &mut Report) {
    let rules = &board.rules;
    // (position, drill, owner, element)
    let mut holes: Vec<(Point, i64, ElementId)> =
        board.vias.iter().map(|v| (v.at, v.drill, v.id)).collect();
    for pad in board.resolved_pads() {
        let Some(drill) = pad.drill else {
            continue;
        };
        holes.push((pad.at, drill, pad.instance));
        if drill < rules.min_drill {
            report.add(
                "pad_drill",
                Severity::Error,
                pad.at,
                None,
                vec![pad.instance],
                &[
                    ("actual", mm(drill as f64)),
                    ("required", mm(rules.min_drill as f64)),
                ],
            );
        }
        if drill > rules.max_drill {
            report.add(
                "drill_too_large",
                Severity::Error,
                pad.at,
                None,
                vec![pad.instance],
                &[
                    ("actual", mm(drill as f64)),
                    ("required", mm(rules.max_drill as f64)),
                ],
            );
        }
        if pad.plated {
            let smaller = pad.size.x.min(pad.size.y);
            let ring = (smaller - drill) as f64 / 2.0;
            if ring < rules.min_annular_ring as f64 {
                report.add(
                    "annular_ring",
                    Severity::Error,
                    pad.at,
                    None,
                    vec![pad.instance],
                    &[
                        ("actual", mm(ring)),
                        ("required", mm(rules.min_annular_ring as f64)),
                    ],
                );
            }
        }
    }
    for via in board.vias.iter() {
        if via.drill > rules.max_drill {
            report.add(
                "drill_too_large",
                Severity::Error,
                via.at,
                None,
                vec![via.id],
                &[
                    ("actual", mm(via.drill as f64)),
                    ("required", mm(rules.max_drill as f64)),
                ],
            );
        }
    }
    holes.sort_by_key(|hole| (hole.0.x, hole.0.y));
    for (i, a) in holes.iter().enumerate() {
        for b in &holes[i + 1..] {
            let reach = rules.min_hole_spacing + (a.1 + b.1) / 2;
            if b.0.x - a.0.x > reach {
                break;
            }
            let center = ((a.0.x - b.0.x) as f64).hypot((a.0.y - b.0.y) as f64);
            let spacing = center - (a.1 + b.1) as f64 / 2.0;
            if spacing < rules.min_hole_spacing as f64 {
                report.add(
                    "hole_spacing",
                    Severity::Error,
                    a.0,
                    None,
                    vec![a.2, b.2],
                    &[
                        ("actual", mm(spacing)),
                        ("required", mm(rules.min_hole_spacing as f64)),
                    ],
                );
            }
        }
    }
}

fn outline_segments(board: &Board) -> Vec<(Point, Point)> {
    let n = board.outline.len();
    (0..n)
        .map(|i| (board.outline[i], board.outline[(i + 1) % n]))
        .collect()
}

fn in_outline(board: &Board, p: Point) -> bool {
    let contour: Vec<[f64; 2]> = board
        .outline
        .iter()
        .map(|q| [q.x as f64, q.y as f64])
        .collect();
    tronic_pcb::geom::in_polygons((p.x as f64, p.y as f64), &[contour])
}

fn check_edge(board: &Board, report: &mut Report) {
    if board.outline.len() < 3 || polygon_is_invalid(&board.outline) {
        return;
    }
    let edge = board.rules.edge_clearance as f64;
    let segments = outline_segments(board);
    let pads = board.resolved_pads();
    let items = copper_items(board, &pads);
    for item in &items {
        // A part still parked outside the board is reported as such.
        if let ItemKind::Pad { .. } = item.kind {
            let center = item.shape.center();
            if !in_outline(board, point_of(center)) {
                continue;
            }
        }
        let distance = segments
            .iter()
            .map(|(a, b)| item.shape.distance_to_segment(*a, *b))
            .fold(f64::MAX, f64::min);
        let center = point_of(item.shape.center());
        let outside = !in_outline(board, center);
        if distance < edge || outside {
            report.add(
                "board_edge",
                Severity::Error,
                center,
                item.layer,
                vec![item.element()],
                &[
                    ("actual", mm(if outside { 0.0 } else { distance.max(0.0) })),
                    ("required", mm(edge)),
                ],
            );
        }
    }
    // Bare holes carry no copper but must keep their distance as well.
    for pad in pads.iter().filter(|pad| !pad.plated) {
        if !in_outline(board, pad.at) {
            continue;
        }
        let shape = pad_shape(pad);
        let distance = segments
            .iter()
            .map(|(a, b)| shape.distance_to_segment(*a, *b))
            .fold(f64::MAX, f64::min);
        if distance < edge {
            report.add(
                "board_edge",
                Severity::Error,
                pad.at,
                None,
                vec![pad.instance],
                &[("actual", mm(distance.max(0.0))), ("required", mm(edge))],
            );
        }
    }
}

fn check_placement(board: &Board, report: &mut Report) {
    let boxes: Vec<(&FootprintInstance, Point, Point)> = board
        .footprints
        .iter()
        .filter_map(|instance| {
            let (min, max) = board.courtyard(instance)?;
            Some((instance, min, max))
        })
        .collect();
    let has_outline = board.outline.len() >= 3 && !polygon_is_invalid(&board.outline);
    for (index, (a, a_min, a_max)) in boxes.iter().enumerate() {
        if has_outline {
            let corners = [
                *a_min,
                Point::new(a_max.x, a_min.y),
                *a_max,
                Point::new(a_min.x, a_max.y),
            ];
            if !corners.iter().all(|corner| in_outline(board, *corner)) {
                report.add(
                    "outside_board",
                    Severity::Error,
                    a.at,
                    None,
                    vec![a.id],
                    &[("reference", a.reference.clone())],
                );
            }
        }
        for (b, b_min, b_max) in &boxes[index + 1..] {
            if a.bottom == b.bottom
                && a_min.x < b_max.x
                && b_min.x < a_max.x
                && a_min.y < b_max.y
                && b_min.y < a_max.y
            {
                report.add(
                    "component_overlap",
                    Severity::Error,
                    a.at,
                    None,
                    vec![a.id, b.id],
                    &[("a", a.reference.clone()), ("b", b.reference.clone())],
                );
            }
        }
    }
}

fn check_nets(board: &Board, report: &mut Report) {
    let analysis = analyze(board);
    for wire in &analysis.airwires {
        report.add(
            "unconnected",
            Severity::Error,
            wire.a,
            None,
            vec![],
            &[("net", wire.net.clone())],
        );
    }
    let pad_nets: HashSet<String> = board
        .resolved_pads()
        .into_iter()
        .filter_map(|pad| pad.net)
        .collect();
    for track in board.tracks.iter() {
        if !pad_nets.contains(&track.net) {
            report.add(
                "unknown_net",
                Severity::Warning,
                track.a,
                Some(track.layer),
                vec![track.id],
                &[("net", track.net.clone())],
            );
        }
    }
    for via in board.vias.iter() {
        if !pad_nets.contains(&via.net) {
            report.add(
                "unknown_net",
                Severity::Warning,
                via.at,
                None,
                vec![via.id],
                &[("net", via.net.clone())],
            );
        }
    }
}
