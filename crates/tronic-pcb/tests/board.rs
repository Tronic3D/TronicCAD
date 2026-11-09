//! Behaviour of commands, the netlist sync, connectivity and zones.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;
use tronic_pcb::*;
use tronic_schematic::netlist::{NetlistComponent, NetlistNet, NetlistNode};
use tronic_schematic::{Netlist, Point, Provenance};

fn p(x: i64, y: i64) -> Point {
    Point::new(x * MM, y * MM)
}

/// Two surface pads, numbered 1 and 2, 2 mm apart.
fn two_pad() -> FootprintDef {
    let pad = |number: &str, x: i64| PadDef {
        number: number.to_owned(),
        at: p(x, 0),
        size: p(1, 1),
        shape: PadShape::Rect,
        drill: None,
        plated: true,
    };
    FootprintDef {
        id: "t:chip".to_owned(),
        name: "Chip".to_owned(),
        pads: vec![pad("1", -1), pad("2", 1)],
        silk: Vec::new(),
        courtyard: [p(-2, -1), p(2, 1)],
        provenance: Provenance {
            source: "test".to_owned(),
            author: "test".to_owned(),
            license: "test".to_owned(),
            reviewed: false,
        },
        body: Vec::new(),
        model: None,
    }
}

fn library(id: &str) -> Option<FootprintDef> {
    (id == "t:chip").then(two_pad)
}

fn netlist(entries: &[(&str, &str, Option<&str>)], nets: &[(&str, &[(&str, &str)])]) -> Netlist {
    Netlist {
        components: entries
            .iter()
            .map(|(reference, value, footprint)| NetlistComponent {
                reference: (*reference).to_owned(),
                value: (*value).to_owned(),
                symbol: "s".to_owned(),
                sheet: "1".to_owned(),
                footprint: footprint.map(str::to_owned),
            })
            .collect(),
        nets: nets
            .iter()
            .map(|(name, nodes)| NetlistNet {
                name: (*name).to_owned(),
                nodes: nodes
                    .iter()
                    .map(|(reference, pin)| NetlistNode {
                        reference: (*reference).to_owned(),
                        pin: (*pin).to_owned(),
                        pin_name: String::new(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn basic_netlist() -> Netlist {
    netlist(
        &[("R1", "1k", Some("t:chip")), ("R2", "2k", Some("t:chip"))],
        &[
            ("A", &[("R1", "1")]),
            ("B", &[("R1", "2"), ("R2", "1")]),
            ("C", &[("R2", "2")]),
        ],
    )
}

fn editor() -> PcbEditor {
    let mut editor = PcbEditor::default();
    editor.sync(&basic_netlist(), &library);
    editor
}

fn place(e: &mut PcbEditor, reference: &str, at: Point) {
    let instance = e
        .board()
        .footprints
        .iter()
        .find(|f| f.reference == reference)
        .unwrap();
    let (id, delta) = (
        instance.id,
        Point::new(at.x - instance.at.x, at.y - instance.at.y),
    );
    e.apply(PcbCommand::Move {
        ids: vec![id],
        delta,
    })
    .unwrap();
}

fn track(e: &mut PcbEditor, net: &str, layer: u8, points: &[Point]) -> Vec<u64> {
    e.apply(PcbCommand::AddTracks {
        net: net.to_owned(),
        layer,
        points: points.to_vec(),
        width: 250_000,
    })
    .unwrap()
    .0
    .created
}

fn square() -> Vec<Point> {
    vec![p(-20, -20), p(40, -20), p(40, 20), p(-20, 20)]
}

mod sync {
    use super::*;

    #[test]
    fn adds_parts_with_nets_on_their_pads() {
        let e = editor();
        let board = e.board();
        assert_eq!(board.footprints.len(), 2);
        let r1 = &board.footprints[0];
        assert_eq!(r1.reference, "R1");
        assert_eq!(r1.pad_nets["1"], "A");
        assert_eq!(r1.pad_nets["2"], "B");
        assert!(compare_with_netlist(board, &basic_netlist()).is_empty());
    }

    #[test]
    fn new_parts_do_not_overlap_and_land_outside_the_board() {
        let mut e = editor();
        let board = e.board();
        let (a, b) = (&board.footprints[0], &board.footprints[1]);
        let (a_min, a_max) = board.courtyard(a).unwrap();
        let (b_min, _) = board.courtyard(b).unwrap();
        assert!(a_max.y < b_min.y || a_max.x < b_min.x, "{a_min:?}");

        e.apply(PcbCommand::SetOutline { points: square() })
            .unwrap();
        e.sync(
            &netlist(
                &[
                    ("R1", "1k", Some("t:chip")),
                    ("R2", "2k", Some("t:chip")),
                    ("R3", "3k", Some("t:chip")),
                ],
                &[],
            ),
            &library,
        );
        let r3 = e
            .board()
            .footprints
            .iter()
            .find(|f| f.reference == "R3")
            .unwrap();
        assert!(r3.at.x > 40 * MM);
    }

    #[test]
    fn is_idempotent_and_keeps_placement() {
        let mut e = editor();
        place(&mut e, "R1", p(5, 5));
        let before = e.board().clone();
        let (report, changed) = e.sync(&basic_netlist(), &library);
        assert_eq!(report, SyncReport::default());
        assert!(!changed);
        assert_eq!(e.board(), &before);
    }

    #[test]
    fn updates_values_and_nets_without_moving() {
        let mut e = editor();
        place(&mut e, "R1", p(5, 5));
        let changed = netlist(
            &[("R1", "47k", Some("t:chip")), ("R2", "2k", Some("t:chip"))],
            &[
                ("NEU", &[("R1", "1"), ("R2", "2")]),
                ("B", &[("R1", "2"), ("R2", "1")]),
            ],
        );
        let (report, _) = e.sync(&changed, &library);
        assert!(report.updated.contains(&"R1".to_owned()));
        let r1 = &e.board().footprints[0];
        assert_eq!(r1.at, p(5, 5));
        assert_eq!(r1.value, "47k");
        assert_eq!(r1.pad_nets["1"], "NEU");
        assert!(compare_with_netlist(e.board(), &changed).is_empty());
    }

    #[test]
    fn removes_dropped_parts_and_keeps_their_tracks() {
        let mut e = editor();
        track(&mut e, "C", 0, &[p(0, 0), p(5, 0)]);
        let reduced = netlist(&[("R1", "1k", Some("t:chip"))], &[("A", &[("R1", "1")])]);
        let (report, _) = e.sync(&reduced, &library);
        assert_eq!(report.removed, ["R2"]);
        assert_eq!(e.board().footprints.len(), 1);
        assert_eq!(e.board().tracks.len(), 1);
        assert!(compare_with_netlist(e.board(), &reduced).is_empty());
    }

    #[test]
    fn reports_what_it_cannot_do() {
        let mut e = PcbEditor::default();
        let list = netlist(
            &[
                ("U1", "x", None),
                ("U2", "y", Some("unbekannt")),
                ("R1", "1k", Some("t:chip")),
            ],
            &[("A", &[("R1", "1"), ("R1", "9")])],
        );
        let (report, _) = e.sync(&list, &library);
        assert_eq!(report.no_footprint, ["U1"]);
        assert_eq!(
            report.unknown_footprint,
            [("U2".to_owned(), "unbekannt".to_owned())]
        );
        assert_eq!(report.missing_pads, [("R1".to_owned(), "9".to_owned())]);
        let deviations = compare_with_netlist(e.board(), &list);
        assert!(deviations.contains(&Deviation::MissingFootprint {
            reference: "U1".to_owned()
        }));
        assert!(deviations
            .iter()
            .any(|d| matches!(d, Deviation::PinNetDiffers { pin, .. } if pin == "9")));
    }

    #[test]
    fn compare_finds_changes_made_behind_its_back() {
        let e = editor();
        let other = netlist(
            &[("R1", "9k", Some("t:chip")), ("R9", "1k", Some("t:chip"))],
            &[("A", &[("R1", "2")])],
        );
        let deviations = compare_with_netlist(e.board(), &other);
        assert!(deviations.contains(&Deviation::ValueDiffers {
            reference: "R1".to_owned()
        }));
        assert!(deviations.contains(&Deviation::MissingFootprint {
            reference: "R9".to_owned()
        }));
        assert!(deviations.contains(&Deviation::ExtraFootprint {
            reference: "R2".to_owned()
        }));
    }

    #[test]
    fn sync_is_one_undo_step() {
        let mut e = PcbEditor::default();
        e.sync(&basic_netlist(), &library);
        assert!(e.can_undo());
        e.undo().unwrap();
        assert!(e.board().footprints.is_empty());
    }
}

mod commands {
    use super::*;

    #[test]
    fn pads_follow_rotation_and_side() {
        let mut e = editor();
        place(&mut e, "R1", p(10, 10));
        let id = e.board().footprints[0].id;
        let pad = |e: &PcbEditor| e.board().resolved_pads()[0].clone();
        assert_eq!(pad(&e).at, p(9, 10));
        assert_eq!(pad(&e).layer, Some(0));

        e.apply(PcbCommand::Rotate { ids: vec![id] }).unwrap();
        assert_eq!(pad(&e).at, p(10, 9));

        e.apply(PcbCommand::Rotate { ids: vec![id] }).unwrap();
        e.apply(PcbCommand::Rotate { ids: vec![id] }).unwrap();
        e.apply(PcbCommand::Rotate { ids: vec![id] }).unwrap();
        e.apply(PcbCommand::Flip { ids: vec![id] }).unwrap();
        assert_eq!(pad(&e).at, p(11, 10), "mirrored on the bottom side");
        assert_eq!(pad(&e).layer, Some(1));
    }

    #[test]
    fn rotation_swaps_rectangular_pad_sizes() {
        let mut e = PcbEditor::default();
        let mut def = two_pad();
        def.pads[0].size = p(3, 1);
        let list = basic_netlist();
        e.sync(&list, &move |_| Some(def.clone()));
        let id = e.board().footprints[0].id;
        e.apply(PcbCommand::Rotate { ids: vec![id] }).unwrap();
        assert_eq!(e.board().resolved_pads()[0].size, p(1, 3));
    }

    #[test]
    fn bad_commands_change_nothing() {
        let mut e = editor();
        let before = e.board().clone();
        for command in [
            PcbCommand::AddTracks {
                net: String::new(),
                layer: 0,
                points: vec![p(0, 0), p(1, 0)],
                width: 1,
            },
            PcbCommand::AddTracks {
                net: "A".to_owned(),
                layer: 9,
                points: vec![p(0, 0), p(1, 0)],
                width: 1,
            },
            PcbCommand::AddTracks {
                net: "A".to_owned(),
                layer: 0,
                points: vec![p(0, 0), p(1, 0)],
                width: 0,
            },
            PcbCommand::SetOutline {
                points: vec![p(0, 0), p(1, 1)],
            },
            PcbCommand::SetOutline {
                points: vec![p(0, 0), p(10, 10), p(10, 0), p(0, 10)],
            },
            PcbCommand::AddZone {
                net: "A".to_owned(),
                layer: 0,
                outline: vec![p(0, 0)],
            },
            PcbCommand::SetLayerCount { layers: 3 },
        ] {
            assert!(e.apply(command).is_err());
            assert_eq!(e.board(), &before);
        }
    }

    #[test]
    fn undo_and_redo_restore_exact_states() {
        let mut e = editor();
        let start = e.board().clone();
        track(&mut e, "A", 0, &[p(0, 0), p(1, 0), p(1, 1)]);
        let after = e.board().clone();
        assert_eq!(after.tracks.len(), 2);
        e.undo().unwrap();
        assert_eq!(e.board(), &start);
        e.redo().unwrap();
        assert_eq!(e.board(), &after);
    }

    #[test]
    fn deleting_does_not_remove_parts() {
        let mut e = editor();
        let part = e.board().footprints[0].id;
        let ids = track(&mut e, "A", 0, &[p(0, 0), p(1, 0)]);
        e.apply(PcbCommand::Delete {
            ids: vec![part, ids[0]],
        })
        .unwrap();
        assert_eq!(e.board().footprints.len(), 2);
        assert!(e.board().tracks.is_empty());
    }

    #[test]
    fn all_required_layer_counts_work_and_keep_the_bottom() {
        let mut e = editor();
        track(&mut e, "A", 1, &[p(0, 0), p(1, 0)]);
        for layers in [4u8, 6, 8] {
            e.apply(PcbCommand::SetLayerCount { layers }).unwrap();
            assert_eq!(e.board().tracks[0].layer, layers - 1, "bottom stays bottom");
            track(&mut e, "A", layers - 2, &[p(2, 2), p(3, 3)]);
            let last = *e
                .board()
                .tracks
                .last()
                .unwrap()
                .id
                .to_string()
                .as_bytes()
                .first()
                .unwrap();
            let _ = last;
            e.undo().unwrap();
        }
        e.apply(PcbCommand::SetLayerCount { layers: 2 }).unwrap();
        assert_eq!(e.board().tracks[0].layer, 1);
        assert_eq!(
            e.apply(PcbCommand::SetLayerCount { layers: 1 })
                .unwrap_err(),
            PcbError::LayerInUse(1)
        );
    }

    #[test]
    fn inner_layers_cannot_be_dropped_while_in_use() {
        let mut e = editor();
        e.apply(PcbCommand::SetLayerCount { layers: 4 }).unwrap();
        track(&mut e, "A", 2, &[p(0, 0), p(1, 0)]);
        assert_eq!(
            e.apply(PcbCommand::SetLayerCount { layers: 2 })
                .unwrap_err(),
            PcbError::LayerInUse(2)
        );
    }

    #[test]
    fn single_layer_boards_have_no_bottom_side() {
        let mut e = editor();
        e.apply(PcbCommand::SetLayerCount { layers: 1 }).unwrap();
        let id = e.board().footprints[0].id;
        assert_eq!(
            e.apply(PcbCommand::Flip { ids: vec![id] })
                .unwrap_err()
                .to_string(),
            PcbError::SingleLayerBoard.to_string()
        );
        assert_eq!(e.board().bottom_layer(), 0);
    }

    #[test]
    fn validation_rejects_broken_files() {
        let good = editor().board().clone();
        good.validate().unwrap();

        let mut layers = good.clone();
        layers.copper_layers = 3;
        assert!(layers.validate().is_err());

        let mut defs = good.clone();
        defs.footprint_defs = BTreeMap::new();
        assert!(defs.validate().is_err());

        let mut ids = good.clone();
        ids.next_id = 1;
        assert!(ids.validate().is_err());

        let mut width = good;
        Arc::make_mut(&mut width.tracks).push(Track {
            id: 0,
            net: "A".to_owned(),
            layer: 0,
            a: p(0, 0),
            b: p(1, 0),
            width: 0,
        });
        assert!(width.validate().is_err());
    }
}

mod connectivity {
    use super::*;

    fn routed() -> PcbEditor {
        let mut e = editor();
        place(&mut e, "R1", p(0, 0));
        place(&mut e, "R2", p(10, 0));
        e
    }

    #[test]
    fn missing_connections_appear_as_airwires() {
        let e = routed();
        let wires = analyze(e.board()).airwires;
        assert_eq!(wires.len(), 1);
        assert_eq!(wires[0].net, "B");
        assert_eq!(wires[0].a, p(1, 0));
        assert_eq!(wires[0].b, p(9, 0));
    }

    #[test]
    fn a_track_closes_the_connection() {
        let mut e = routed();
        track(&mut e, "B", 0, &[p(1, 0), p(9, 0)]);
        assert!(analyze(e.board()).airwires.is_empty());
        assert_eq!(analyze(e.board()).net_lengths["B"], 8 * MM);
    }

    #[test]
    fn a_track_of_another_net_does_not_connect() {
        let mut e = routed();
        track(&mut e, "X", 0, &[p(1, 0), p(9, 0)]);
        let wires: Vec<_> = analyze(e.board()).airwires;
        assert!(wires.iter().any(|w| w.net == "B"));
    }

    #[test]
    fn a_track_on_another_layer_needs_vias() {
        let mut e = routed();
        track(&mut e, "B", 1, &[p(1, 0), p(9, 0)]);
        assert!(!analyze(e.board()).airwires.is_empty());

        for at in [p(1, 0), p(9, 0)] {
            e.apply(PcbCommand::AddVia {
                net: "B".to_owned(),
                at,
            })
            .unwrap();
        }
        assert!(analyze(e.board()).airwires.is_empty());
    }

    #[test]
    fn a_zone_connects_what_lies_on_it() {
        let mut e = routed();
        e.apply(PcbCommand::SetOutline { points: square() })
            .unwrap();
        e.apply(PcbCommand::AddZone {
            net: "B".to_owned(),
            layer: 0,
            outline: square(),
        })
        .unwrap();
        let analysis = analyze(e.board());
        assert!(analysis.airwires.is_empty(), "{:?}", analysis.airwires);
        assert_eq!(analysis.fills[0].shapes.len(), 1);
    }
}

mod zones {
    use super::*;
    use tronic_pcb::geom::in_polygons;

    fn contours(shape: &[Vec<Point>]) -> Vec<Vec<[f64; 2]>> {
        shape
            .iter()
            .map(|c| c.iter().map(|p| [p.x as f64, p.y as f64]).collect())
            .collect()
    }

    fn inside(fill: &ZoneFill, at: Point) -> bool {
        fill.shapes
            .iter()
            .any(|shape| in_polygons((at.x as f64, at.y as f64), &contours(shape)))
    }

    fn board() -> PcbEditor {
        let mut e = editor();
        place(&mut e, "R1", p(0, 0));
        place(&mut e, "R2", p(10, 0));
        e.apply(PcbCommand::SetOutline { points: square() })
            .unwrap();
        e.apply(PcbCommand::AddZone {
            net: "B".to_owned(),
            layer: 0,
            outline: square(),
        })
        .unwrap();
        e
    }

    #[test]
    fn keeps_clear_of_other_nets_with_the_clearance() {
        let e = board();
        let fill = &fill_zones(e.board())[0];
        // R1 pad 1 (net A) at (-1, 0), 1 mm wide: its centre stays free.
        assert!(!inside(fill, p(-1, 0)));
        // Just outside pad plus clearance is filled again.
        let clearance = e.board().rules.min_clearance;
        let free = Point::new(-MM - MM / 2 - clearance - 50_000, 0);
        assert!(inside(fill, free));
        let forbidden = Point::new(-MM - MM / 2 - clearance + 50_000, 0);
        assert!(!inside(fill, forbidden));
        // Own net is covered.
        assert!(inside(fill, p(1, 0)));
    }

    #[test]
    fn keeps_the_edge_clearance_and_stays_on_the_board() {
        let mut e = board();
        e.apply(PcbCommand::AddZone {
            net: "C".to_owned(),
            layer: 1,
            outline: vec![p(-50, -50), p(90, -50), p(90, 50), p(-50, 50)],
        })
        .unwrap();
        track(&mut e, "C", 1, &[p(0, 5), p(1, 5)]);
        let fill = &fill_zones(e.board())[1];
        assert!(!fill.shapes.is_empty());
        assert!(!inside(fill, p(-30, 0)), "outside the board");
        let edge = e.board().rules.edge_clearance;
        assert!(!inside(fill, Point::new(-20 * MM + edge - 10_000, 0)));
        assert!(inside(fill, Point::new(-20 * MM + edge + 100_000, 0)));
    }

    #[test]
    fn drops_islands_without_own_copper() {
        let mut e = board();
        // A wall of another net cuts the zone in two; only one side has B.
        track(&mut e, "X", 0, &[p(20, -30), p(20, 30)]);
        let fill = &fill_zones(e.board())[0];
        assert!(inside(fill, p(5, 10)));
        assert!(!inside(fill, p(30, 0)), "island removed");
    }

    #[test]
    fn a_zone_without_own_copper_stays_empty() {
        let mut e = PcbEditor::default();
        e.apply(PcbCommand::SetOutline { points: square() })
            .unwrap();
        e.apply(PcbCommand::AddZone {
            net: "GND".to_owned(),
            layer: 0,
            outline: square(),
        })
        .unwrap();
        assert!(fill_zones(e.board())[0].shapes.is_empty());
    }

    #[test]
    fn a_later_zone_keeps_clear_of_an_earlier_one() {
        let mut e = board();
        track(&mut e, "C", 0, &[p(30, 0), p(31, 0)]);
        e.apply(PcbCommand::AddZone {
            net: "C".to_owned(),
            layer: 0,
            outline: vec![p(25, -10), p(35, -10), p(35, 10), p(25, 10)],
        })
        .unwrap();
        let fills = fill_zones(e.board());
        // The first zone covers this area, so the second has nothing left.
        assert!(fills[1].shapes.is_empty());
    }

    #[test]
    fn needs_a_board_outline() {
        let mut e = editor();
        e.apply(PcbCommand::AddZone {
            net: "B".to_owned(),
            layer: 0,
            outline: square(),
        })
        .unwrap();
        assert!(fill_zones(e.board())[0].shapes.is_empty());
    }
}
