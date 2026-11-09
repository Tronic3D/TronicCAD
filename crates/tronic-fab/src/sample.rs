//! A known board for tests and for checking the output with independent tools.

use std::collections::BTreeMap;
use std::sync::Arc;
use tronic_pcb::*;
use tronic_schematic::{Point, Provenance, Rotation};

fn p(x: i64, y: i64) -> Point {
    Point::new(x * MM, y * MM)
}

fn provenance() -> Provenance {
    Provenance {
        source: "test".to_owned(),
        author: "test".to_owned(),
        license: "test".to_owned(),
        reviewed: false,
    }
}

fn chip() -> FootprintDef {
    let pad = |number: &str, x: i64| PadDef {
        number: number.to_owned(),
        at: Point::new(x, 0),
        size: Point::new(MM, 1_300_000),
        shape: PadShape::Rect,
        drill: None,
        plated: true,
    };
    FootprintDef {
        id: "chip".to_owned(),
        name: "Chip 0805".to_owned(),
        pads: vec![pad("1", -950_000), pad("2", 950_000)],
        silk: vec![[p(-2, -1), p(2, -1)], [p(-2, 1), p(2, 1)]],
        courtyard: [p(-2, -1), p(2, 1)],
        provenance: provenance(),
        body: Vec::new(),
        model: None,
    }
}

fn header() -> FootprintDef {
    let pad = |number: &str, y: i64, shape| PadDef {
        number: number.to_owned(),
        at: Point::new(0, y),
        size: Point::new(1_700_000, 1_700_000),
        shape,
        drill: Some(MM),
        plated: true,
    };
    FootprintDef {
        id: "header".to_owned(),
        name: "Header 1x02".to_owned(),
        pads: vec![
            pad("1", -1_270_000, PadShape::Rect),
            pad("2", 1_270_000, PadShape::Round),
        ],
        silk: Vec::new(),
        courtyard: [p(-2, -3), p(2, 3)],
        provenance: provenance(),
        body: Vec::new(),
        model: None,
    }
}

fn hole() -> FootprintDef {
    FootprintDef {
        id: "hole".to_owned(),
        name: "Hole M3".to_owned(),
        pads: vec![PadDef {
            number: String::new(),
            at: p(0, 0),
            size: Point::new(3_200_000, 3_200_000),
            shape: PadShape::Round,
            drill: Some(3_200_000),
            plated: false,
        }],
        silk: Vec::new(),
        courtyard: [p(-3, -3), p(3, 3)],
        provenance: provenance(),
        body: Vec::new(),
        model: None,
    }
}

/// 50 x 40 mm: two chips (one on the bottom), a header, a mounting hole, tracks
/// on the outer layers, a via and a ground zone on the bottom.
pub fn sample_board(layers: u8) -> Board {
    let instance =
        |id, reference: &str, value: &str, footprint: &str, at, bottom, nets: &[(&str, &str)]| {
            FootprintInstance {
                id,
                reference: reference.to_owned(),
                value: value.to_owned(),
                footprint: footprint.to_owned(),
                at,
                rotation: Rotation::R0,
                bottom: bottom && layers > 1,
                pad_nets: nets
                    .iter()
                    .map(|(pad, net)| ((*pad).to_owned(), (*net).to_owned()))
                    .collect(),
            }
        };
    let footprints = vec![
        instance(
            1,
            "R1",
            "10k",
            "chip",
            p(15, 10),
            false,
            &[("1", "VCC"), ("2", "SIG")],
        ),
        instance(
            2,
            "C1",
            "100n",
            "chip",
            p(30, 10),
            true,
            &[("1", "SIG"), ("2", "GND")],
        ),
        instance(
            3,
            "J1",
            "Header",
            "header",
            p(10, 25),
            false,
            &[("1", "VCC"), ("2", "GND")],
        ),
        instance(4, "H1", "M3", "hole", p(44, 34), false, &[]),
    ];
    let definitions: BTreeMap<String, Arc<FootprintDef>> = [chip(), header(), hole()]
        .into_iter()
        .map(|definition| (definition.id.clone(), Arc::new(definition)))
        .collect();
    let bottom = layers - 1;
    let tracks = vec![
        Track {
            id: 5,
            net: "SIG".to_owned(),
            layer: 0,
            a: Point::new(15_950_000, 10 * MM),
            b: p(25, 10),
            width: 250_000,
        },
        Track {
            id: 6,
            net: "VCC".to_owned(),
            layer: 0,
            a: Point::new(14_050_000, 10 * MM),
            b: p(10, 10),
            width: 400_000,
        },
        Track {
            id: 7,
            net: "VCC".to_owned(),
            layer: 0,
            a: p(10, 10),
            b: Point::new(10 * MM, 23_730_000),
            width: 400_000,
        },
        Track {
            id: 8,
            net: "SIG".to_owned(),
            layer: bottom,
            a: p(25, 10),
            b: Point::new(29_050_000, 10 * MM),
            width: 250_000,
        },
    ];
    let vias = vec![Via {
        id: 9,
        net: "SIG".to_owned(),
        at: p(25, 10),
        drill: 300_000,
        diameter: 600_000,
    }];
    let zones = vec![Zone {
        id: 10,
        net: "GND".to_owned(),
        layer: bottom,
        outline: vec![p(1, 1), p(49, 1), p(49, 39), p(1, 39)],
    }];
    Board {
        next_id: 11,
        copper_layers: layers,
        rules: DesignRules::default(),
        outline: vec![p(0, 0), p(50, 0), p(50, 40), p(0, 40)],
        footprints: Arc::new(footprints),
        tracks: Arc::new(tracks),
        vias: Arc::new(vias),
        zones: Arc::new(zones),
        footprint_defs: definitions,
        mechanics: Mechanics::default(),
    }
}
