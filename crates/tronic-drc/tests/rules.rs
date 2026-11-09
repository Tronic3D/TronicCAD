//! One failing case per rule, plus clean boards that must stay free of findings.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;
use tronic_drc::{check, Severity};
use tronic_pcb::*;
use tronic_schematic::netlist::{NetlistComponent, NetlistNet, NetlistNode};
use tronic_schematic::{Netlist, Point, Provenance};

fn p(x: i64, y: i64) -> Point {
    Point::new(x * MM, y * MM)
}

fn def(id: &str, pads: Vec<PadDef>) -> FootprintDef {
    FootprintDef {
        id: id.to_owned(),
        name: id.to_owned(),
        pads,
        silk: Vec::new(),
        courtyard: [p(-2, -1), p(2, 1)],
        provenance: Provenance {
            source: "t".to_owned(),
            author: "t".to_owned(),
            license: "t".to_owned(),
            reviewed: false,
        },
        body: Vec::new(),
        model: None,
    }
}

fn smd(number: &str, x: i64) -> PadDef {
    PadDef {
        number: number.to_owned(),
        at: p(x, 0),
        size: p(1, 1),
        shape: PadShape::Rect,
        drill: None,
        plated: true,
    }
}

fn library(id: &str) -> Option<FootprintDef> {
    let hole = |number: &str, drill: i64, plated: bool| PadDef {
        number: number.to_owned(),
        at: p(0, 0),
        size: p(2, 2),
        shape: PadShape::Round,
        drill: Some(drill),
        plated,
    };
    match id {
        "chip" => Some(def("chip", vec![smd("1", -1), smd("2", 1)])),
        "tiny_hole" => Some(def("tiny_hole", vec![hole("1", 100_000, true)])),
        "huge_hole" => Some(def("huge_hole", vec![hole("1", 9 * MM, false)])),
        "thin_ring" => {
            let mut pad = hole("1", 1_950_000, true);
            pad.size = p(2, 2);
            Some(def("thin_ring", vec![pad]))
        }
        "mount" => Some(def("mount", vec![hole("", 3 * MM, false)])),
        _ => None,
    }
}

fn netlist(parts: &[(&str, &str)], nets: &[(&str, &[(&str, &str)])]) -> Netlist {
    Netlist {
        components: parts
            .iter()
            .map(|(reference, footprint)| NetlistComponent {
                reference: (*reference).to_owned(),
                value: "v".to_owned(),
                symbol: "s".to_owned(),
                sheet: "1".to_owned(),
                footprint: Some((*footprint).to_owned()),
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

struct Bench(PcbEditor);

impl Bench {
    /// R1 at (10, 10), R2 at (20, 10) on a 40 x 30 mm board, joined by net B.
    fn new() -> Self {
        let mut editor = PcbEditor::default();
        editor
            .apply(PcbCommand::SetOutline {
                points: vec![p(0, 0), p(40, 0), p(40, 30), p(0, 30)],
            })
            .unwrap();
        editor.sync(
            &netlist(
                &[("R1", "chip"), ("R2", "chip")],
                &[
                    ("A", &[("R1", "1")]),
                    ("B", &[("R1", "2"), ("R2", "1")]),
                    ("C", &[("R2", "2")]),
                ],
            ),
            &library,
        );
        let mut bench = Self(editor);
        bench.place("R1", p(10, 10));
        bench.place("R2", p(20, 10));
        bench
    }

    fn place(&mut self, reference: &str, at: Point) {
        let instance = self
            .0
            .board()
            .footprints
            .iter()
            .find(|f| f.reference == reference)
            .unwrap()
            .clone();
        self.0
            .apply(PcbCommand::Move {
                ids: vec![instance.id],
                delta: Point::new(at.x - instance.at.x, at.y - instance.at.y),
            })
            .unwrap();
    }

    fn track(&mut self, net: &str, layer: u8, a: Point, b: Point) {
        self.width_track(net, layer, a, b, 250_000);
    }

    fn width_track(&mut self, net: &str, layer: u8, a: Point, b: Point, width: i64) {
        self.0
            .apply(PcbCommand::AddTracks {
                net: net.to_owned(),
                layer,
                points: vec![a, b],
                width,
            })
            .unwrap();
    }

    fn routed(mut self) -> Self {
        self.track("B", 0, p(11, 10), p(19, 10));
        self
    }

    fn codes(&self) -> Vec<&'static str> {
        let mut codes: Vec<_> = check(self.0.board()).into_iter().map(|v| v.code).collect();
        codes.sort_unstable();
        codes.dedup();
        codes
    }

    fn has(&self, code: &str) -> bool {
        self.codes().contains(&code)
    }

    fn set_rules(&mut self, change: impl FnOnce(&mut DesignRules)) {
        let mut rules = self.0.board().rules.clone();
        change(&mut rules);
        self.0.apply(PcbCommand::SetRules { rules }).unwrap();
    }

    fn add_part(&mut self, reference: &str, footprint: &str, at: Point) {
        let mut parts: Vec<(String, String)> = self
            .0
            .board()
            .footprints
            .iter()
            .map(|f| (f.reference.clone(), f.footprint.clone()))
            .collect();
        parts.push((reference.to_owned(), footprint.to_owned()));
        let parts: Vec<(&str, &str)> = parts
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let list = netlist(
            &parts,
            &[
                ("A", &[("R1", "1")]),
                ("B", &[("R1", "2"), ("R2", "1")]),
                ("C", &[("R2", "2")]),
            ],
        );
        self.0.sync(&list, &library);
        self.place(reference, at);
    }
}

#[test]
fn a_routed_board_is_clean() {
    assert_eq!(Bench::new().routed().codes(), Vec::<&str>::new());
}

#[test]
fn a_clean_board_on_several_layers_with_a_via_is_clean() {
    let mut b = Bench::new();
    for layers in [4u8, 6, 8] {
        b.0.apply(PcbCommand::SetLayerCount { layers }).unwrap();
    }
    b.track("B", 0, p(11, 10), p(15, 10));
    b.0.apply(PcbCommand::AddVia {
        net: "B".to_owned(),
        at: p(15, 10),
    })
    .unwrap();
    b.track("B", 3, p(15, 10), p(15, 14));
    b.0.apply(PcbCommand::AddVia {
        net: "B".to_owned(),
        at: p(15, 14),
    })
    .unwrap();
    b.track("B", 0, p(15, 14), p(19, 14));
    b.track("B", 0, p(19, 14), p(19, 10));
    assert_eq!(b.codes(), Vec::<&str>::new());
}

#[test]
fn a_clean_board_with_a_zone_is_clean() {
    let mut b = Bench::new();
    b.0.apply(PcbCommand::AddZone {
        net: "B".to_owned(),
        layer: 0,
        outline: vec![p(0, 0), p(40, 0), p(40, 30), p(0, 30)],
    })
    .unwrap();
    assert_eq!(b.codes(), Vec::<&str>::new());
}

#[test]
fn finds_unconnected_nets() {
    let b = Bench::new();
    let found: Vec<_> = check(b.0.board())
        .into_iter()
        .filter(|v| v.code == "unconnected")
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].params["net"], "B");
}

#[test]
fn finds_a_short_between_tracks() {
    let mut b = Bench::new().routed();
    b.track("X", 0, p(15, 5), p(15, 15));
    let found: Vec<_> = check(b.0.board())
        .into_iter()
        .filter(|v| v.code == "short")
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, Severity::Error);
    assert_eq!(found[0].layer, Some(0));
}

#[test]
fn tracks_on_different_layers_do_not_short() {
    let mut b = Bench::new().routed();
    b.track("X", 1, p(15, 5), p(15, 15));
    assert!(!b.has("short"));
    assert!(!b.has("clearance"));
}

#[test]
fn a_via_shorts_on_every_layer() {
    let mut b = Bench::new().routed();
    b.track("X", 1, p(15, 5), p(15, 9));
    b.0.apply(PcbCommand::AddVia {
        net: "X".to_owned(),
        at: p(15, 10),
    })
    .unwrap();
    assert!(b.has("short"));
}

#[test]
fn finds_too_little_clearance_with_measured_values() {
    let mut b = Bench::new().routed();
    // Tracks 0.25 mm wide, centres 0.35 mm apart: gap 0.100 mm.
    b.track(
        "X",
        0,
        Point::new(12 * MM, 10_350_000),
        Point::new(18 * MM, 10_350_000),
    );
    let found: Vec<_> = check(b.0.board())
        .into_iter()
        .filter(|v| v.code == "clearance")
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].params["actual"], "0.100");
    assert_eq!(found[0].params["required"], "0.200");
}

#[test]
fn exactly_the_clearance_passes() {
    let mut b = Bench::new().routed();
    // Gap of exactly 0.2 mm.
    b.track(
        "X",
        0,
        Point::new(12 * MM, 10_450_000),
        Point::new(18 * MM, 10_450_000),
    );
    assert!(!b.has("clearance"));
}

#[test]
fn finds_copper_too_close_to_a_pad() {
    let mut b = Bench::new().routed();
    // R1 pad 1 spans y 9.5 to 10.5 at x = 9 mm; the track ends 0.075 mm above it.
    b.track(
        "X",
        0,
        Point::new(9 * MM, 8 * MM),
        Point::new(9 * MM, 9_300_000),
    );
    assert!(b.has("clearance"));
}

#[test]
fn finds_thin_tracks() {
    let mut b = Bench::new().routed();
    b.width_track("B", 0, p(11, 12), p(19, 12), 100_000);
    assert!(b.has("track_width"));
}

#[test]
fn finds_small_via_drills_and_thin_rings() {
    let mut b = Bench::new().routed();
    b.set_rules(|rules| {
        rules.via_drill = 100_000;
        rules.via_diameter = 150_000;
    });
    b.0.apply(PcbCommand::AddVia {
        net: "B".to_owned(),
        at: p(15, 10),
    })
    .unwrap();
    assert!(b.has("via_drill"));
    assert!(b.has("annular_ring"));
}

#[test]
fn finds_problems_with_pad_holes() {
    let mut b = Bench::new().routed();
    b.add_part("H1", "tiny_hole", p(30, 5));
    assert!(b.has("pad_drill"));

    let mut b = Bench::new().routed();
    b.add_part("H2", "huge_hole", p(30, 15));
    assert!(b.has("drill_too_large"));

    let mut b = Bench::new().routed();
    b.add_part("H3", "thin_ring", p(30, 5));
    assert!(b.has("annular_ring"));
}

#[test]
fn finds_holes_too_close_together() {
    let mut b = Bench::new().routed();
    b.0.apply(PcbCommand::AddVia {
        net: "B".to_owned(),
        at: p(15, 10),
    })
    .unwrap();
    b.0.apply(PcbCommand::AddVia {
        net: "B".to_owned(),
        at: Point::new(15_500_000, 10 * MM),
    })
    .unwrap();
    let found: Vec<_> = check(b.0.board())
        .into_iter()
        .filter(|v| v.code == "hole_spacing")
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].params["actual"], "0.200");
}

#[test]
fn finds_copper_too_close_to_the_edge() {
    let mut b = Bench::new().routed();
    b.track(
        "B",
        0,
        Point::new(100_000, 5 * MM),
        Point::new(100_000, 8 * MM),
    );
    let found: Vec<_> = check(b.0.board())
        .into_iter()
        .filter(|v| v.code == "board_edge")
        .collect();
    assert_eq!(found.len(), 1);
}

#[test]
fn finds_copper_outside_the_board() {
    let mut b = Bench::new().routed();
    b.track("B", 0, p(50, 5), p(55, 5));
    assert!(b.has("board_edge"));
}

#[test]
fn finds_a_bare_hole_too_close_to_the_edge() {
    let mut b = Bench::new().routed();
    b.add_part("H1", "mount", Point::new(1_600_000, 15 * MM));
    let found: Vec<_> = check(b.0.board())
        .into_iter()
        .filter(|v| v.code == "board_edge")
        .collect();
    assert_eq!(found.len(), 1);
}

#[test]
fn finds_a_missing_and_an_invalid_outline() {
    let mut b = Bench::new().routed();
    b.0.apply(PcbCommand::SetOutline { points: vec![] })
        .unwrap();
    assert!(b.has("outline_missing"));
    let mut board = b.0.board().clone();
    board.outline = vec![p(0, 0), p(10, 10), p(10, 0), p(0, 10)];
    let codes: Vec<_> = check(&board).into_iter().map(|v| v.code).collect();
    assert!(codes.contains(&"outline_invalid"));
}

#[test]
fn finds_parts_outside_the_board() {
    let mut b = Bench::new().routed();
    b.place("R2", p(60, 10));
    let found: Vec<_> = check(b.0.board())
        .into_iter()
        .filter(|v| v.code == "outside_board")
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].params["reference"], "R2");
}

#[test]
fn finds_overlapping_parts_on_the_same_side_only() {
    let mut b = Bench::new();
    b.place("R2", Point::new(12 * MM, 10 * MM));
    assert!(b.has("component_overlap"));

    let id = b.0.board().footprints[1].id;
    b.0.apply(PcbCommand::Flip { ids: vec![id] }).unwrap();
    assert!(!b.has("component_overlap"));
}

#[test]
fn finds_copper_on_a_layer_that_does_not_exist() {
    let b = Bench::new().routed();
    let mut board = b.0.board().clone();
    Arc::make_mut(&mut board.tracks)[0].layer = 7;
    let codes: Vec<_> = check(&board).into_iter().map(|v| v.code).collect();
    assert!(codes.contains(&"invalid_layer"));
}

#[test]
fn warns_about_copper_of_an_unknown_net() {
    let mut b = Bench::new().routed();
    b.track("GESPENST", 0, p(30, 5), p(35, 5));
    let found: Vec<_> = check(b.0.board())
        .into_iter()
        .filter(|v| v.code == "unknown_net")
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].severity, Severity::Warning);
}

#[test]
fn stricter_rules_turn_a_clean_board_into_findings() {
    let mut b = Bench::new().routed();
    b.set_rules(|rules| rules.min_track_width = 300_000);
    assert!(b.has("track_width"));
}

#[test]
fn the_result_is_deterministic() {
    let mut b = Bench::new();
    b.track("X", 0, p(15, 5), p(15, 15));
    assert_eq!(check(b.0.board()), check(b.0.board()));
}
