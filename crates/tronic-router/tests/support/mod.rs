//! Reference boards, shared by the tests and the benchmark.
#![allow(dead_code, clippy::unwrap_used)]

use tronic_library::find_footprint;
use tronic_pcb::{Board, PcbCommand, PcbEditor, MM};
use tronic_schematic::netlist::{NetlistComponent, NetlistNet, NetlistNode};
use tronic_schematic::{Netlist, Point};

pub fn p(x: i64, y: i64) -> Point {
    Point::new(x * MM, y * MM)
}

pub struct Part {
    pub reference: String,
    pub footprint: &'static str,
    pub at: Point,
}

pub fn part(reference: &str, footprint: &'static str, x: i64, y: i64) -> Part {
    Part {
        reference: reference.to_owned(),
        footprint,
        at: p(x, y),
    }
}

/// Builds a placed board from parts and nets given as (reference, pad).
pub fn board(
    width: i64,
    height: i64,
    layers: u8,
    parts: &[Part],
    nets: &[(String, Vec<(String, String)>)],
) -> Board {
    let netlist = Netlist {
        components: parts
            .iter()
            .map(|part| NetlistComponent {
                reference: part.reference.clone(),
                value: "v".to_owned(),
                symbol: "s".to_owned(),
                sheet: "1".to_owned(),
                footprint: Some(part.footprint.to_owned()),
            })
            .collect(),
        nets: nets
            .iter()
            .map(|(name, nodes)| NetlistNet {
                name: name.clone(),
                nodes: nodes
                    .iter()
                    .map(|(reference, pin)| NetlistNode {
                        reference: reference.clone(),
                        pin: pin.clone(),
                        pin_name: String::new(),
                    })
                    .collect(),
            })
            .collect(),
    };
    let mut editor = PcbEditor::default();
    editor.apply(PcbCommand::SetLayerCount { layers }).unwrap();
    editor
        .apply(PcbCommand::SetOutline {
            points: vec![p(0, 0), p(width, 0), p(width, height), p(0, height)],
        })
        .unwrap();
    editor.sync(&netlist, &find_footprint);
    for spec in parts {
        let instance = editor
            .board()
            .footprints
            .iter()
            .find(|f| f.reference == spec.reference)
            .unwrap()
            .clone();
        editor
            .apply(PcbCommand::Move {
                ids: vec![instance.id],
                delta: Point::new(spec.at.x - instance.at.x, spec.at.y - instance.at.y),
            })
            .unwrap();
    }
    editor.board().clone()
}

pub fn net(name: &str, nodes: &[(&str, &str)]) -> (String, Vec<(String, String)>) {
    (
        name.to_owned(),
        nodes
            .iter()
            .map(|(reference, pin)| ((*reference).to_owned(), (*pin).to_owned()))
            .collect(),
    )
}

/// Resistor and LED on a connector.
pub fn led_board() -> Board {
    board(
        30,
        20,
        2,
        &[
            part("J1", "header:1x02", 5, 10),
            part("R1", "chip:0805", 15, 6),
            part("D1", "chip:0805", 24, 14),
        ],
        &[
            net("VCC", &[("J1", "1"), ("R1", "1")]),
            net("LED", &[("R1", "2"), ("D1", "1")]),
            net("GND", &[("D1", "2"), ("J1", "2")]),
        ],
    )
}

/// A 555 flasher, a typical small project.
pub fn flasher_board() -> Board {
    board(
        50,
        40,
        2,
        &[
            part("U1", "soic:8", 25, 20),
            part("R1", "chip:0805", 12, 8),
            part("R2", "chip:0805", 25, 8),
            part("C1", "chip:0805", 12, 32),
            part("C2", "chip:0805", 25, 32),
            part("R3", "chip:0805", 38, 15),
            part("D1", "chip:0805", 38, 25),
            part("J1", "header:1x02", 6, 20),
        ],
        &[
            net("VCC", &[("J1", "1"), ("U1", "8"), ("U1", "4"), ("R1", "1")]),
            net(
                "GND",
                &[
                    ("J1", "2"),
                    ("U1", "1"),
                    ("C1", "2"),
                    ("C2", "2"),
                    ("D1", "2"),
                ],
            ),
            net("DIS", &[("U1", "7"), ("R1", "2"), ("R2", "1")]),
            net("THR", &[("U1", "6"), ("U1", "2"), ("R2", "2"), ("C1", "1")]),
            net("CTRL", &[("U1", "5"), ("C2", "1")]),
            net("OUT", &[("U1", "3"), ("R3", "1")]),
            net("LED", &[("R3", "2"), ("D1", "1")]),
        ],
    )
}

/// Eight chips sharing four bus lines and a connector.
pub fn bus_board() -> Board {
    let mut parts = vec![part("J1", "header:1x08", 5, 30)];
    for i in 0..8 {
        parts.push(part(
            &format!("U{}", i + 1),
            "soic:8",
            20 + (i % 4) * 18,
            15 + (i / 4) * 30,
        ));
    }
    let mut nets = Vec::new();
    for line in 1..=4 {
        let mut nodes = vec![("J1".to_owned(), line.to_string())];
        for chip in 1..=8 {
            nodes.push((format!("U{chip}"), line.to_string()));
        }
        nets.push((format!("BUS{line}"), nodes));
    }
    for chip in 1..=4 {
        nets.push(net(
            &format!("SEL{chip}"),
            &[("J1", &(4 + chip).to_string()), (&format!("U{chip}"), "5")],
        ));
    }
    board(90, 60, 2, &parts, &nets)
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: i64) -> i64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) % bound as u64) as i64
    }
}

/// Chips in a raster with randomly joined pins, from a fixed seed.
pub fn random_board(seed: u64, chips: i64, width: i64, height: i64, layers: u8) -> Board {
    let mut rng = Lcg(seed);
    let columns = (chips as f64).sqrt().ceil() as i64;
    let rows = (chips + columns - 1) / columns;
    let mut parts = Vec::new();
    for i in 0..chips {
        parts.push(part(
            &format!("U{}", i + 1),
            "soic:8",
            width * (2 * (i % columns) + 1) / (2 * columns),
            height * (2 * (i / columns) + 1) / (2 * rows),
        ));
    }
    let mut pins: Vec<(String, String)> = (0..chips)
        .flat_map(|i| (1..=8).map(move |pin| (format!("U{}", i + 1), pin.to_string())))
        .collect();
    let mut nets = Vec::new();
    let mut number = 0;
    while pins.len() >= 2 {
        number += 1;
        let size = 2 + (rng.next(4) == 0) as i64;
        let mut nodes = Vec::new();
        for _ in 0..size {
            if pins.is_empty() {
                break;
            }
            let index = rng.next(pins.len() as i64) as usize;
            nodes.push(pins.swap_remove(index));
        }
        nets.push((format!("N{number}"), nodes));
    }
    board(width, height, layers, &parts, &nets)
}
