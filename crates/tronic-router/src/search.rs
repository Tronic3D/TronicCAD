//! Path search over nodes, layers and directions. Costs are in hundredths of a
//! grid step. Tracking the direction lets turns cost something, which keeps
//! routes straight and forbids sharp angles.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tronic_pcb::{Board, CopperItem, Shape};
use tronic_schematic::Point;

use crate::grid::RouteGrid;
use tronic_pcb::{NewTrack, NewVia};

const STRAIGHT: u32 = 100;
const DIAGONAL: u32 = 141;
/// A via costs as much as this many grid steps of detour.
const VIA_COST: u32 = 3000;
/// A share of extra cost against a layer's preferred direction.
const OFF_DIRECTION: u32 = 25;
/// Turn costs by 45 degree steps. Angles of 135 degrees and sharper are excluded.
const TURN: [u32; 3] = [0, 15, 80];
const MAX_EXPANSIONS: usize = 3_000_000;

const DIRECTIONS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];
const NO_DIRECTION: u8 = 8;

pub struct Connection<'a> {
    pub net: &'a str,
    pub net_index: u32,
    pub from: Point,
    pub to: Point,
}

pub struct SearchLimits<'a> {
    pub deadline: Instant,
    pub cancel: &'a AtomicBool,
}

/// A place on the grid where a route may begin or end.
#[derive(Debug, Clone, Copy)]
struct Terminal {
    layer: usize,
    node: usize,
}

/// Grid nodes that a straight, clear stub links to the copper at `point`.
fn terminals(
    grid: &RouteGrid,
    board: &Board,
    items: &[CopperItem],
    connection: &Connection<'_>,
    point: Point,
) -> Vec<Terminal> {
    let here = Shape::circle(point, 0);
    let own: Vec<&CopperItem> = items
        .iter()
        .filter(|item| item.net.as_deref() == Some(connection.net) && item.shape.gap(&here) <= 0.0)
        .collect();
    let width = board.rules.track_width;
    let clearance = board.rules.min_clearance as f64;
    let reach = grid.pitch * 3 / 2;
    let mut result = Vec::new();
    for layer in 0..grid.layers {
        let on_layer: Vec<&&CopperItem> = own
            .iter()
            .filter(|item| item.layer.is_none_or(|l| l as usize == layer))
            .collect();
        if on_layer.is_empty() {
            continue;
        }
        // Nodes under the copper itself, plus the nearest ones around the point.
        let mut bounds = (
            point.x as f64 - reach as f64,
            point.y as f64 - reach as f64,
            point.x as f64 + reach as f64,
            point.y as f64 + reach as f64,
        );
        for item in &on_layer {
            let (x0, y0, x1, y1) = item.shape.bounds();
            bounds = (
                bounds.0.min(x0),
                bounds.1.min(y0),
                bounds.2.max(x1),
                bounds.3.max(y1),
            );
        }
        let x0 =
            ((bounds.0 as i64 - grid.x0) / grid.pitch).clamp(0, grid.width as i64 - 1) as usize;
        let x1 =
            ((bounds.2 as i64 - grid.x0) / grid.pitch + 1).clamp(0, grid.width as i64 - 1) as usize;
        let y0 =
            ((bounds.1 as i64 - grid.y0) / grid.pitch).clamp(0, grid.height as i64 - 1) as usize;
        let y1 = ((bounds.3 as i64 - grid.y0) / grid.pitch + 1).clamp(0, grid.height as i64 - 1)
            as usize;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let node = y * grid.width + x;
                if !grid.may_use(layer, node, connection.net_index) {
                    continue;
                }
                let at = grid.position(x, y);
                let stub = Shape::track(point, at, width);
                let clear = items.iter().all(|item| {
                    item.net.as_deref() == Some(connection.net)
                        || item.layer.is_some_and(|l| l as usize != layer)
                        || item.shape.gap(&stub) >= clearance
                });
                if clear {
                    result.push(Terminal { layer, node });
                }
            }
        }
    }
    result
}

fn key(node: usize, layer: usize, direction: u8, grid: &RouteGrid) -> u64 {
    ((node + layer * grid.width * grid.height) as u64) * 9 + u64::from(direction)
}

fn unkey(value: u64, grid: &RouteGrid) -> (usize, usize) {
    let index = (value / 9) as usize;
    (
        index % (grid.width * grid.height),
        index / (grid.width * grid.height),
    )
}

fn estimate(grid: &RouteGrid, node: usize, target: Point) -> u32 {
    let p = grid.node_position(node);
    let dx = ((p.x - target.x).abs() / grid.pitch) as u32;
    let dy = ((p.y - target.y).abs() / grid.pitch) as u32;
    let (long, short) = if dx > dy { (dx, dy) } else { (dy, dx) };
    (long - short) * STRAIGHT + short * DIAGONAL
}

/// Finds a route, or None if the search space or its limits are exhausted.
pub fn find_path(
    grid: &RouteGrid,
    board: &Board,
    items: &[CopperItem],
    connection: &Connection<'_>,
    limits: &SearchLimits<'_>,
) -> Option<(Vec<NewTrack>, Vec<NewVia>)> {
    let starts = terminals(grid, board, items, connection, connection.from);
    let goals: HashSet<(usize, usize)> = terminals(grid, board, items, connection, connection.to)
        .into_iter()
        .map(|t| (t.layer, t.node))
        .collect();
    if starts.is_empty() || goals.is_empty() {
        return None;
    }

    let mut heap = BinaryHeap::new();
    let mut best: HashMap<u64, u32> = HashMap::new();
    let mut parent: HashMap<u64, u64> = HashMap::new();
    let net = connection.net_index;
    for start in &starts {
        let state = key(start.node, start.layer, NO_DIRECTION, grid);
        best.insert(state, 0);
        heap.push(Reverse((estimate(grid, start.node, connection.to), state)));
    }

    let (width, height) = (grid.width as i32, grid.height as i32);
    let mut expansions = 0usize;
    let mut found = None;
    while let Some(Reverse((_, state))) = heap.pop() {
        expansions += 1;
        if expansions > MAX_EXPANSIONS {
            return None;
        }
        if expansions.is_multiple_of(4096)
            && (Instant::now() >= limits.deadline || limits.cancel.load(Ordering::Relaxed))
        {
            return None;
        }
        let (node, layer) = unkey(state, grid);
        let direction = (state % 9) as u8;
        if goals.contains(&(layer, node)) {
            found = Some(state);
            break;
        }
        let cost = best[&state];
        let (x, y) = ((node % grid.width) as i32, (node / grid.width) as i32);

        let mut relax = |next: u64, next_node: usize, extra: u32, heap: &mut BinaryHeap<_>| {
            let candidate = cost.saturating_add(extra);
            if best.get(&next).is_none_or(|known| candidate < *known) {
                best.insert(next, candidate);
                parent.insert(next, state);
                let priority = candidate + estimate(grid, next_node, connection.to);
                heap.push(Reverse((priority, next)));
            }
        };

        for (index, (dx, dy)) in DIRECTIONS.iter().enumerate() {
            let turn = if direction == NO_DIRECTION {
                0
            } else {
                let difference = (index as i32 - direction as i32).rem_euclid(8);
                let steps = difference.min(8 - difference) as usize;
                if steps >= TURN.len() {
                    continue;
                }
                TURN[steps]
            };
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= width || ny >= height {
                continue;
            }
            let next_node = ny as usize * grid.width + nx as usize;
            if !grid.may_use(layer, next_node, net) {
                continue;
            }
            // A diagonal step must not squeeze between two blocked neighbours.
            if *dx != 0 && *dy != 0 {
                let side_a = y as usize * grid.width + nx as usize;
                let side_b = ny as usize * grid.width + x as usize;
                if !grid.may_use(layer, side_a, net) && !grid.may_use(layer, side_b, net) {
                    continue;
                }
            }
            let mut step = if *dx != 0 && *dy != 0 {
                DIAGONAL
            } else {
                STRAIGHT
            };
            if grid.layers > 1 {
                let horizontal_layer = layer % 2 == 0;
                let off = (horizontal_layer && *dx == 0) || (!horizontal_layer && *dy == 0);
                if off {
                    step += OFF_DIRECTION;
                }
            }
            relax(
                key(next_node, layer, index as u8, grid),
                next_node,
                step + turn,
                &mut heap,
            );
        }
        if grid.layers > 1 && grid.may_place_via(node, net) {
            for other in 0..grid.layers {
                if other != layer && grid.may_use(other, node, net) {
                    relax(
                        key(node, other, NO_DIRECTION, grid),
                        node,
                        VIA_COST,
                        &mut heap,
                    );
                }
            }
        }
    }

    let mut state = found?;
    let mut path = vec![unkey(state, grid)];
    while let Some(previous) = parent.get(&state) {
        state = *previous;
        path.push(unkey(state, grid));
    }
    path.reverse();
    Some(build_copper(grid, board, connection, &path))
}

/// Turns grid steps into tracks and vias, with stubs to the exact end points.
fn build_copper(
    grid: &RouteGrid,
    board: &Board,
    connection: &Connection<'_>,
    path: &[(usize, usize)],
) -> (Vec<NewTrack>, Vec<NewVia>) {
    let width = board.rules.track_width;
    let mut tracks = Vec::new();
    let mut vias = Vec::new();
    let mut add_run = |layer: usize, points: &[Point]| {
        // Merges points on one straight line.
        let mut corners: Vec<Point> = Vec::with_capacity(points.len());
        for point in points {
            if corners.last() == Some(point) {
                continue;
            }
            if corners.len() >= 2 {
                let a = corners[corners.len() - 2];
                let b = corners[corners.len() - 1];
                let collinear = (b.x - a.x) * (point.y - b.y) == (b.y - a.y) * (point.x - b.x)
                    && (b.x - a.x) * (point.x - b.x) + (b.y - a.y) * (point.y - b.y) > 0;
                if collinear {
                    corners.pop();
                }
            }
            corners.push(*point);
        }
        for pair in corners.windows(2) {
            tracks.push(NewTrack {
                net: connection.net.to_owned(),
                layer: layer as u8,
                a: pair[0],
                b: pair[1],
                width,
            });
        }
    };

    let mut current_layer = path[0].1;
    let mut run = vec![connection.from, grid.node_position(path[0].0)];
    for (node, layer) in &path[1..] {
        let at = grid.node_position(*node);
        if *layer != current_layer {
            add_run(current_layer, &run);
            vias.push(NewVia {
                net: connection.net.to_owned(),
                at,
            });
            current_layer = *layer;
            run = vec![at];
        } else {
            run.push(at);
        }
    }
    run.push(connection.to);
    add_run(current_layer, &run);
    (tracks, vias)
}
