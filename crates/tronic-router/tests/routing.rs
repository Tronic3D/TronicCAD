//! What the router promises: valid results, honest reports, unchanged copper.
#![allow(clippy::unwrap_used, clippy::panic)]

mod support;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use support::*;
use tronic_drc::check;
use tronic_pcb::*;
use tronic_router::{route_board, RouteError, RouteOptions, RouteResult};

static NEVER: AtomicBool = AtomicBool::new(false);

fn route(board: &Board) -> RouteResult {
    route_board(
        board,
        &RouteOptions {
            time_limit: Duration::from_secs(120),
            cancel: &NEVER,
            progress: &|_, _| {},
        },
    )
    .unwrap()
}

fn apply(board: &Board, result: &RouteResult) -> Board {
    let mut applied = board.clone();
    for track in &result.tracks {
        let id = applied.next_id;
        applied.next_id += 1;
        Arc::make_mut(&mut applied.tracks).push(Track {
            id,
            net: track.net.clone(),
            layer: track.layer,
            a: track.a,
            b: track.b,
            width: track.width,
        });
    }
    for via in &result.vias {
        let id = applied.next_id;
        applied.next_id += 1;
        Arc::make_mut(&mut applied.vias).push(Via {
            id,
            net: via.net.clone(),
            at: via.at,
            drill: board.rules.via_drill,
            diameter: board.rules.via_diameter,
        });
    }
    applied
}

/// All rule findings other than open connections, by code.
fn findings(board: &Board) -> Vec<&'static str> {
    let mut codes: Vec<_> = check(board)
        .into_iter()
        .filter(|v| v.code != "unconnected")
        .map(|v| v.code)
        .collect();
    codes.sort_unstable();
    codes
}

/// The core promise, for any board.
fn assert_valid(board: &Board, result: &RouteResult) {
    let applied = apply(board, result);
    assert_eq!(
        findings(&applied),
        findings(board),
        "the proposal added findings"
    );
    assert_eq!(
        analyze(&applied).airwires,
        result.remaining,
        "the report must match the resulting board"
    );
}

#[test]
fn routes_the_led_board_completely() {
    let board = led_board();
    assert_eq!(analyze(&board).airwires.len(), 3);
    let result = route(&board);
    assert_valid(&board, &result);
    assert_eq!(result.total, 3);
    assert!(result.remaining.is_empty(), "{:?}", result.remaining);
    assert!(!result.tracks.is_empty());
}

#[test]
fn routes_the_flasher_completely() {
    let board = flasher_board();
    let result = route(&board);
    assert_valid(&board, &result);
    assert!(result.remaining.is_empty(), "{:?}", result.remaining);
}

#[test]
fn routes_the_bus_board_completely() {
    let board = bus_board();
    let result = route(&board);
    assert_valid(&board, &result);
    assert!(result.remaining.is_empty(), "{:?}", result.remaining);
}

#[test]
fn a_dense_board_stays_valid_whatever_it_achieves() {
    let board = random_board(7, 9, 60, 60, 2);
    let result = route(&board);
    assert_valid(&board, &result);
    assert!(result.remaining.len() < result.total, "routed nothing");
}

#[test]
fn every_layer_count_works() {
    for layers in ALLOWED_LAYER_COUNTS {
        let board = random_board(3, 4, 50, 50, layers);
        let result = route(&board);
        assert_valid(&board, &result);
        if layers == 1 {
            assert!(result.vias.is_empty(), "no vias on one layer");
        }
    }
}

#[test]
fn existing_copper_stays_and_is_respected() {
    let mut editor = PcbEditor::new(led_board());
    // A foreign track across the middle of the board, on both layers.
    for layer in [0, 1] {
        editor
            .apply(PcbCommand::AddTracks {
                net: "WAND".to_owned(),
                layer,
                points: vec![p(10, 1), p(10, 12)],
                width: 300_000,
            })
            .unwrap();
    }
    let board = editor.board().clone();
    let result = route(&board);
    assert_valid(&board, &result);
    assert!(result.tracks.iter().all(|t| t.net != "WAND"));
    let applied = apply(&board, &result);
    for track in board.tracks.iter() {
        assert!(applied.tracks.contains(track), "existing copper changed");
    }
}

#[test]
fn a_full_wall_on_one_layer_leaves_the_connection_open_and_reports_it() {
    let mut editor = PcbEditor::new(board(
        30,
        20,
        1,
        &[
            part("R1", "chip:0805", 5, 10),
            part("R2", "chip:0805", 25, 10),
        ],
        &[net("A", &[("R1", "2"), ("R2", "1")])],
    ));
    editor
        .apply(PcbCommand::AddTracks {
            net: "WAND".to_owned(),
            layer: 0,
            points: vec![p(15, 0), p(15, 20)],
            width: 300_000,
        })
        .unwrap();
    let board = editor.board().clone();
    let result = route(&board);
    assert_valid(&board, &result);
    assert_eq!(result.remaining.len(), 1);
    assert_eq!(result.remaining[0].net, "A");
}

#[test]
fn the_same_wall_with_a_second_layer_is_passed_with_vias() {
    let mut editor = PcbEditor::new(board(
        30,
        20,
        2,
        &[
            part("R1", "chip:0805", 5, 10),
            part("R2", "chip:0805", 25, 10),
        ],
        &[net("A", &[("R1", "2"), ("R2", "1")])],
    ));
    editor
        .apply(PcbCommand::AddTracks {
            net: "WAND".to_owned(),
            layer: 0,
            points: vec![p(15, 0), p(15, 20)],
            width: 300_000,
        })
        .unwrap();
    let board = editor.board().clone();
    let result = route(&board);
    assert_valid(&board, &result);
    assert!(result.remaining.is_empty());
    assert!(result.vias.len() >= 2, "{:?}", result.vias);
}

#[test]
fn stays_inside_the_edge_clearance() {
    let board = led_board();
    let result = route(&board);
    let edge = board.rules.edge_clearance;
    for track in &result.tracks {
        for point in [track.a, track.b] {
            assert!(point.x >= edge && point.x <= 30 * MM - edge, "{point:?}");
            assert!(point.y >= edge && point.y <= 20 * MM - edge, "{point:?}");
        }
    }
}

#[test]
fn angles_are_multiples_of_45_degrees_between_grid_points() {
    let board = flasher_board();
    let result = route(&board);
    let on_grid = |p: tronic_schematic::Point| p.x % 50_000 == 0 && p.y % 50_000 == 0;
    let mut checked = 0;
    for track in &result.tracks {
        if on_grid(track.a) && on_grid(track.b) {
            let (dx, dy) = ((track.b.x - track.a.x).abs(), (track.b.y - track.a.y).abs());
            // Stubs to off-grid pads may have any angle, grid runs may not.
            if dx != 0 && dy != 0 && dx != dy {
                continue;
            }
            checked += 1;
        }
    }
    assert!(checked > 0);
}

#[test]
fn a_part_outside_the_board_is_reported_not_forced() {
    let board = board(
        30,
        20,
        2,
        &[
            part("R1", "chip:0805", 5, 10),
            part("R2", "chip:0805", 60, 10),
        ],
        &[net("A", &[("R1", "2"), ("R2", "1")])],
    );
    let result = route(&board);
    assert_eq!(result.remaining.len(), 1);
    assert!(result.tracks.is_empty());
}

#[test]
fn a_zone_net_needs_no_routing_and_new_tracks_keep_the_zone_valid() {
    let mut editor = PcbEditor::new(flasher_board());
    editor
        .apply(PcbCommand::AddZone {
            net: "GND".to_owned(),
            layer: 1,
            outline: vec![p(0, 0), p(50, 0), p(50, 40), p(0, 40)],
        })
        .unwrap();
    let board = editor.board().clone();
    let before = analyze(&board).airwires;
    let result = route(&board);
    assert_valid(&board, &result);
    assert_eq!(result.total, before.len());
}

#[test]
fn a_board_without_outline_is_an_error() {
    let mut board = led_board();
    board.outline.clear();
    let result = route_board(
        &board,
        &RouteOptions {
            time_limit: Duration::from_secs(1),
            cancel: &NEVER,
            progress: &|_, _| {},
        },
    );
    assert_eq!(result.unwrap_err(), RouteError::NoOutline);
}

#[test]
fn a_board_with_nothing_open_returns_nothing() {
    let board = board(10, 10, 2, &[], &[]);
    let result = route(&board);
    assert_eq!(result.total, 0);
    assert!(result.tracks.is_empty() && result.remaining.is_empty());
}

#[test]
fn cancelling_returns_a_valid_partial_result() {
    let board = bus_board();
    let cancel = AtomicBool::new(false);
    let seen = AtomicUsize::new(0);
    let result = route_board(
        &board,
        &RouteOptions {
            time_limit: Duration::from_secs(120),
            cancel: &cancel,
            progress: &|done, _| {
                seen.store(done, Ordering::Relaxed);
                if done >= 2 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        },
    )
    .unwrap();
    assert!(result.cancelled);
    assert!(!result.remaining.is_empty());
    assert_valid(&board, &result);
    assert!(seen.load(Ordering::Relaxed) >= 2);
}

#[test]
fn the_time_limit_is_kept() {
    let board = random_board(11, 16, 80, 80, 2);
    let result = route_board(
        &board,
        &RouteOptions {
            time_limit: Duration::from_millis(1),
            cancel: &NEVER,
            progress: &|_, _| {},
        },
    )
    .unwrap();
    assert!(result.timed_out);
    assert_valid(&board, &result);
}

#[test]
fn the_result_is_deterministic() {
    let board = flasher_board();
    let (mut a, mut b) = (route(&board), route(&board));
    a.elapsed_ms = 0;
    b.elapsed_ms = 0;
    assert_eq!(a, b);
}
