//! Defined test cases for collisions, clearances, heights and the scene.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;
use tronic_3d::*;
use tronic_pcb::*;
use tronic_schematic::{Point, Provenance, Rotation};

fn p(x: i64, y: i64) -> Point {
    Point::new(x * MM, y * MM)
}

fn tall_part(id: &str, half: i64, height: i64) -> FootprintDef {
    FootprintDef {
        id: id.to_owned(),
        name: id.to_owned(),
        pads: Vec::new(),
        silk: Vec::new(),
        courtyard: [p(-half, -half), p(half, half)],
        provenance: Provenance {
            source: "t".to_owned(),
            author: "t".to_owned(),
            license: "t".to_owned(),
            reviewed: false,
        },
        body: vec![BodyPart {
            kind: BodyKind::Box,
            center: p(0, 0),
            size: p(2 * half, 2 * half),
            z: 0,
            height: height * MM,
            color: [10, 10, 10],
        }],
        model: None,
    }
}

fn board(parts: &[(&str, FootprintDef, Point, bool)]) -> Board {
    let mut board = Board {
        outline: vec![p(0, 0), p(100, 0), p(100, 100), p(0, 100)],
        ..Board::default()
    };
    let mut instances = Vec::new();
    for (reference, definition, at, bottom) in parts {
        board
            .footprint_defs
            .insert(definition.id.clone(), Arc::new(definition.clone()));
        let id = board.next_id;
        board.next_id += 1;
        instances.push(FootprintInstance {
            id,
            reference: (*reference).to_owned(),
            value: String::new(),
            footprint: definition.id.clone(),
            at: *at,
            rotation: Rotation::R0,
            bottom: *bottom,
            pad_nets: BTreeMap::new(),
        });
    }
    board.footprints = Arc::new(instances);
    board
}

fn codes(board: &Board) -> Vec<&'static str> {
    let mut codes: Vec<_> = check_mechanics(board)
        .findings
        .iter()
        .map(|f| f.code)
        .collect();
    codes.sort_unstable();
    codes
}

#[test]
fn separate_parts_are_clean() {
    let a = tall_part("a", 2, 5);
    let b = board(&[
        ("A", a.clone(), p(10, 10), false),
        ("B", a, p(30, 10), false),
    ]);
    assert_eq!(codes(&b), Vec::<&str>::new());
}

#[test]
fn overlapping_parts_collide() {
    let a = tall_part("a", 2, 5);
    let b = board(&[
        ("A", a.clone(), p(10, 10), false),
        ("B", a, p(12, 10), false),
    ]);
    let report = check_mechanics(&b);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].code, "collision");
    assert_eq!(report.findings[0].params["a"], "A");
}

#[test]
fn parts_on_opposite_sides_do_not_collide() {
    let a = tall_part("a", 2, 5);
    let b = board(&[
        ("A", a.clone(), p(10, 10), false),
        ("B", a, p(10, 10), true),
    ]);
    assert_eq!(codes(&b), Vec::<&str>::new());
}

#[test]
fn a_low_part_under_a_high_overhang_does_not_collide() {
    // A part with a body only from 5 to 8 mm above the board, like a display on
    // spacers, and a 3 mm part underneath it. Courtyards overlap, bodies do not.
    let mut high = tall_part("high", 5, 3);
    high.body[0].z = 5 * MM;
    let low = tall_part("low", 1, 3);
    let b = board(&[("D", high, p(50, 50), false), ("U", low, p(50, 50), false)]);
    assert_eq!(codes(&b), Vec::<&str>::new());
}

#[test]
fn the_same_part_taller_than_the_overhang_collides() {
    let mut high = tall_part("high", 5, 3);
    high.body[0].z = 5 * MM;
    let low = tall_part("low", 1, 6);
    let b = board(&[("D", high, p(50, 50), false), ("U", low, p(50, 50), false)]);
    assert_eq!(codes(&b), ["collision"]);
}

#[test]
fn clearance_is_measured_with_its_value() {
    let a = tall_part("a", 2, 5);
    // 0.1 mm gap between the bodies, default requirement 0.25 mm.
    let at = Point::new(14 * MM + MM / 10, 10 * MM);
    let b = board(&[("A", a.clone(), p(10, 10), false), ("B", a, at, false)]);
    let report = check_mechanics(&b);
    assert_eq!(report.findings[0].code, "too_close");
    assert_eq!(report.findings[0].params["actual"], "0.10");
    assert_eq!(report.findings[0].params["required"], "0.25");
}

#[test]
fn exactly_the_clearance_passes() {
    let a = tall_part("a", 2, 5);
    let at = Point::new(14 * MM + MM / 4, 10 * MM);
    let b = board(&[("A", a.clone(), p(10, 10), false), ("B", a, at, false)]);
    assert_eq!(codes(&b), Vec::<&str>::new());
}

#[test]
fn heights_are_reported_per_side() {
    let b = board(&[
        ("A", tall_part("a", 2, 5), p(10, 10), false),
        ("B", tall_part("b", 2, 9), p(30, 10), false),
        ("C", tall_part("c", 2, 3), p(50, 10), true),
    ]);
    let report = check_mechanics(&b);
    assert_eq!(report.max_top, 9.0);
    assert_eq!(report.max_bottom, 3.0);
    assert_eq!(report.heights.len(), 3);
}

#[test]
fn height_limits_apply_per_side() {
    let mut b = board(&[
        ("A", tall_part("a", 2, 5), p(10, 10), false),
        ("C", tall_part("c", 2, 3), p(50, 10), true),
    ]);
    b.mechanics.max_height_top = Some(4 * MM);
    b.mechanics.max_height_bottom = Some(4 * MM);
    let report = check_mechanics(&b);
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].code, "too_high");
    assert_eq!(report.findings[0].params["reference"], "A");
    assert_eq!(report.findings[0].params["actual"], "5.00");
    assert_eq!(report.findings[0].params["limit"], "4.00");
}

#[test]
fn parts_without_a_model_are_named_and_use_their_courtyard() {
    let mut plain = tall_part("plain", 2, 5);
    plain.body.clear();
    let b = board(&[("A", plain, p(10, 10), false)]);
    let report = check_mechanics(&b);
    assert_eq!(report.findings[0].code, "no_model");
    assert_eq!(report.heights[0].height, 1.0);
    assert!(!report.heights[0].has_model);
    let scene = build_scene(&b);
    assert!(!scene.parts[0].has_model);
    assert_eq!(scene.parts[0].meshes[0].mesh.triangle_count(), 12);
}

#[test]
fn rotation_turns_a_long_part_into_collision() {
    let mut long = tall_part("long", 2, 5);
    long.body[0].size = p(20, 2);
    let a = tall_part("a", 2, 5);
    let mut b = board(&[("L", long, p(50, 50), false), ("A", a, p(50, 60), false)]);
    assert_eq!(codes(&b), Vec::<&str>::new());
    Arc::make_mut(&mut b.footprints)[0].rotation = Rotation::R90;
    assert_eq!(codes(&b), ["collision"]);
}

#[test]
fn an_imported_model_counts_with_its_bounding_box() {
    let mut part = tall_part("m", 1, 1);
    part.body.clear();
    let mesh = parse_model(
        "stl",
        b"solid t\nvertex -5 -5 0\nvertex 5 -5 0\nvertex 0 5 12\nendsolid t\n",
    )
    .unwrap();
    part.model = Some(Arc::new(ImportedModel {
        name: "t.stl".to_owned(),
        positions: mesh.positions,
        scale: 1.0,
        z_offset: 0.0,
    }));
    let other = tall_part("a", 2, 3);
    let b = board(&[
        ("M", part, p(50, 50), false),
        ("A", other, p(53, 50), false),
    ]);
    let report = check_mechanics(&b);
    assert_eq!(report.heights[0].height, 12.0);
    assert!(report.findings.iter().any(|f| f.code == "collision"));
}

#[test]
fn the_scene_places_parts_on_the_board() {
    let b = board(&[
        ("A", tall_part("a", 2, 5), p(10, 20), false),
        ("B", tall_part("b", 2, 3), p(30, 20), true),
    ]);
    let scene = build_scene(&b);
    assert_eq!(scene.thickness, 1.6);
    assert_eq!(scene.outline[1], [100.0, 0.0]);

    let (min, max) = scene.parts[0].meshes[0].mesh.bounds().unwrap();
    assert_eq!((min[0], max[0]), (8.0, 12.0));
    assert_eq!((min[1], max[1]), (-22.0, -18.0), "Y is turned upwards");
    assert_eq!((min[2], max[2]), (0.0, 5.0));

    let (min, max) = scene.parts[1].meshes[0].mesh.bounds().unwrap();
    assert!(
        (min[2] - -4.6).abs() < 1e-5 && (max[2] - -1.6).abs() < 1e-5,
        "{min:?} {max:?}"
    );
}

#[test]
fn the_board_body_is_closed_and_has_the_right_size() {
    let scene = build_scene(&board(&[]));
    let (min, max) = scene.board.bounds().unwrap();
    assert_eq!(min, [0.0, -100.0, -1.6]);
    assert_eq!(max, [100.0, 0.0, 0.0]);
    // Two caps of two triangles, four walls of two.
    assert_eq!(scene.board.triangle_count(), 12);
}

#[test]
fn a_board_without_outline_has_no_body_but_still_parts() {
    let mut b = board(&[("A", tall_part("a", 2, 5), p(10, 10), false)]);
    b.outline.clear();
    let scene = build_scene(&b);
    assert_eq!(scene.board.triangle_count(), 0);
    assert_eq!(scene.parts.len(), 1);
}

#[test]
fn through_hole_pins_reach_parts_on_the_other_side() {
    let dip = tronic_library::find_footprint("dip:8").unwrap();
    let chip = tronic_library::find_footprint("chip:0805").unwrap();
    let pin = dip.pads[0].at;
    let at = p(50, 50);
    let under_pin = Point::new(at.x + pin.x, at.y + pin.y);
    // Under the package between the pin rows it is free, under a pin it is not.
    let free = board(&[("U", dip.clone(), at, false), ("C", chip.clone(), at, true)]);
    assert!(!codes(&free).contains(&"collision"));
    let hit = board(&[("U", dip, at, false), ("C", chip, under_pin, true)]);
    assert_eq!(codes(&hit), ["collision"]);
}
