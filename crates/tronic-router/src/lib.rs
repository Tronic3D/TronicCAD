//! Automatic routing. Specification: `docs/specs/autorouter.md`.
//!
//! The result is a proposal: new copper only, existing copper stays untouched.
//! Whatever cannot be placed without a design rule violation is left out and
//! reported, so a proposal never makes a board worse.

mod grid;
mod search;

use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tronic_pcb::geom::polygon_is_invalid;
use tronic_pcb::{analyze, Airwire, Board, NewTrack, NewVia, Shape, Track, Via};

use grid::RouteGrid;
use search::{find_path, Connection, SearchLimits};

/// A connection that was blocked is retried after making room this many times.
const MAX_ATTEMPTS: u32 = 4;
const MAX_RIP_UPS: u32 = 400;

/// DRC findings a proposal must never add.
const GUARDED_CODES: [&str; 7] = [
    "short",
    "clearance",
    "hole_spacing",
    "board_edge",
    "annular_ring",
    "track_width",
    "via_drill",
];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RouteError {
    #[error("the board has no valid outline")]
    NoOutline,
    #[error("the board is too large for the routing grid")]
    BoardTooLarge,
}

pub struct RouteOptions<'a> {
    pub time_limit: Duration,
    pub cancel: &'a AtomicBool,
    /// Called with (finished connections, all connections).
    pub progress: &'a (dyn Fn(usize, usize) + Sync),
}

/// What the router proposes, and how far it got.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RouteResult {
    pub tracks: Vec<NewTrack>,
    pub vias: Vec<NewVia>,
    /// Open connections before routing.
    pub total: usize,
    /// Connections still open with this proposal applied, measured on the
    /// resulting board.
    pub remaining: Vec<Airwire>,
    pub cancelled: bool,
    pub timed_out: bool,
    pub rip_ups: u32,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone)]
struct Route {
    connection: usize,
    tracks: Vec<NewTrack>,
    vias: Vec<NewVia>,
}

/// The board with a set of routes added, and the ids given to each route's copper.
fn board_with(board: &Board, routes: &[Route]) -> (Board, HashMap<u64, usize>) {
    let mut result = board.clone();
    let mut owner = HashMap::new();
    let tracks = Arc::make_mut(&mut result.tracks);
    let mut next = result.next_id;
    let mut vias: Vec<Via> = Vec::new();
    for (index, route) in routes.iter().enumerate() {
        for track in &route.tracks {
            tracks.push(Track {
                id: next,
                net: track.net.clone(),
                layer: track.layer,
                a: track.a,
                b: track.b,
                width: track.width,
            });
            owner.insert(next, index);
            next += 1;
        }
        for via in &route.vias {
            vias.push(Via {
                id: next,
                net: via.net.clone(),
                at: via.at,
                drill: board.rules.via_drill,
                diameter: board.rules.via_diameter,
            });
            owner.insert(next, index);
            next += 1;
        }
    }
    Arc::make_mut(&mut result.vias).extend(vias);
    result.next_id = next;
    (result, owner)
}

/// Removes every route that adds a guarded DRC finding, until none is left.
fn drop_violating_routes(board: &Board, routes: &mut Vec<Route>) {
    let guarded = |violation: &tronic_drc::Violation| GUARDED_CODES.contains(&violation.code);
    loop {
        let (candidate, owner) = board_with(board, routes);
        let mut bad = HashSet::new();
        for violation in tronic_drc::check(&candidate).iter().filter(|v| guarded(v)) {
            for element in &violation.elements {
                if let Some(route) = owner.get(element) {
                    bad.insert(*route);
                }
            }
        }
        if bad.is_empty() {
            return;
        }
        let mut index = 0;
        routes.retain(|_| {
            let keep = !bad.contains(&index);
            index += 1;
            keep
        });
    }
}

fn conflicts(a: &Route, b: &Route, board: &Board) -> bool {
    let clearance = board.rules.min_clearance as f64;
    let via_shape = |via: &NewVia| Shape::circle(via.at, board.rules.via_diameter);
    let track_shape = |track: &NewTrack| Shape::track(track.a, track.b, track.width);
    for first in &a.tracks {
        for second in &b.tracks {
            if first.layer == second.layer
                && first.net != second.net
                && track_shape(first).gap(&track_shape(second)) < clearance
            {
                return true;
            }
        }
        for via in &b.vias {
            if first.net != via.net && track_shape(first).gap(&via_shape(via)) < clearance {
                return true;
            }
        }
    }
    let spacing = (board.rules.min_hole_spacing + board.rules.via_drill) as f64;
    for first in &a.vias {
        for second in &b.tracks {
            if first.net != second.net && via_shape(first).gap(&track_shape(second)) < clearance {
                return true;
            }
        }
        for second in &b.vias {
            let distance =
                ((first.at.x - second.at.x) as f64).hypot((first.at.y - second.at.y) as f64);
            if distance < spacing
                || (first.net != second.net && via_shape(first).gap(&via_shape(second)) < clearance)
            {
                return true;
            }
        }
    }
    false
}

fn rebuild(base: &RouteGrid, routes: &[Route]) -> RouteGrid {
    let mut grid = base.clone();
    for route in routes {
        grid.add_route(&route.tracks, &route.vias);
    }
    grid
}

/// Routes all open connections of a board.
pub fn route_board(board: &Board, options: &RouteOptions<'_>) -> Result<RouteResult, RouteError> {
    if polygon_is_invalid(&board.outline) {
        return Err(RouteError::NoOutline);
    }
    let started = Instant::now();
    let deadline = started + options.time_limit;
    let analysis = analyze(board);

    let mut wires = analysis.airwires.clone();
    // Short connections first: they have the fewest ways around each other.
    wires.sort_by_key(|wire| {
        let length = ((wire.a.x - wire.b.x) as f64).hypot((wire.a.y - wire.b.y) as f64);
        (length as i64, wire.net.clone(), wire.a, wire.b)
    });
    let total = wires.len();

    let nets: BTreeMap<String, u32> = board
        .resolved_pads()
        .into_iter()
        .filter_map(|pad| pad.net)
        .chain(board.tracks.iter().map(|t| t.net.clone()))
        .chain(board.vias.iter().map(|v| v.net.clone()))
        .chain(wires.iter().map(|w| w.net.clone()))
        .enumerate()
        .map(|(index, net)| (net, index as u32))
        .collect();

    let base = RouteGrid::new(board, &nets).ok_or(RouteError::BoardTooLarge)?;
    let mut map = base.clone();
    let mut routes: Vec<Route> = Vec::new();
    let mut queue: VecDeque<usize> = (0..total).collect();
    let mut attempts = vec![0u32; total];
    let mut rip_ups = 0u32;
    let mut cancelled = false;
    let mut timed_out = false;
    let limits = SearchLimits {
        deadline,
        cancel: options.cancel,
    };
    let mut finished = 0;

    while let Some(index) = queue.pop_front() {
        if options.cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        let wire = &wires[index];
        let connection = Connection {
            net: wire.net.as_str(),
            net_index: nets[&wire.net],
            from: wire.a,
            to: wire.b,
        };
        let items = &analysis.items;
        if let Some((tracks, vias)) = find_path(&map, board, items, &connection, &limits) {
            map.add_route(&tracks, &vias);
            routes.push(Route {
                connection: index,
                tracks,
                vias,
            });
            finished += 1;
            (options.progress)(finished, total);
            continue;
        }
        attempts[index] += 1;
        if attempts[index] >= MAX_ATTEMPTS || rip_ups >= MAX_RIP_UPS {
            continue;
        }
        // Blocked: find a way ignoring routed copper, then move what is in the way.
        let Some((tracks, vias)) = find_path(&base, board, items, &connection, &limits) else {
            continue;
        };
        let candidate = Route {
            connection: index,
            tracks,
            vias,
        };
        let mut kept = Vec::with_capacity(routes.len());
        for route in routes.drain(..) {
            if conflicts(&candidate, &route, board) {
                rip_ups += 1;
                finished -= 1;
                queue.push_back(route.connection);
            } else {
                kept.push(route);
            }
        }
        routes = kept;
        routes.push(candidate);
        map = rebuild(&base, &routes);
        finished += 1;
        (options.progress)(finished, total);
    }

    drop_violating_routes(board, &mut routes);
    let (result_board, _) = board_with(board, &routes);
    let remaining = analyze(&result_board).airwires;
    let mut tracks = Vec::new();
    let mut vias = Vec::new();
    for route in routes {
        tracks.extend(route.tracks);
        vias.extend(route.vias);
    }
    Ok(RouteResult {
        tracks,
        vias,
        total,
        remaining,
        cancelled,
        timed_out,
        rip_ups,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}
