//! The built-in symbols. All drawings are original to this project.

use tronic_schematic::PinDirection::{Down, Left, Right, Up};
use tronic_schematic::PinKind::{
    self, Bidirectional, Input, NotConnected, OpenCollector, Output, Passive, PowerIn, PowerOut,
    SupplyPort,
};
use tronic_schematic::{SymbolDef, SymbolKind};

use crate::draw::*;

/// All categories demanded by the product specification, in display order.
pub const CATEGORIES: [&str; 18] = [
    "resistors",
    "capacitors",
    "inductors",
    "diodes",
    "leds",
    "transistors",
    "mosfets",
    "opamps",
    "microcontrollers",
    "processors",
    "regulators",
    "relays",
    "sensors",
    "displays",
    "connectors",
    "logic",
    "supply",
    "simulation",
];

fn two_pin_v(
    id: &str,
    name: &str,
    category: &str,
    prefix: &str,
    value: &str,
    graphics: Vec<tronic_schematic::Graphic>,
) -> SymbolDef {
    symbol(
        Meta {
            id,
            name,
            category,
            prefix,
            value,
        },
        graphics,
        vec![
            pin("1", "1", Passive, (0, -4), Down, 2),
            pin("2", "2", Passive, (0, 4), Up, 2),
        ],
    )
}

fn diode_graphics() -> Vec<tronic_schematic::Graphic> {
    vec![
        polygon(&[(-1.5, -1.0), (1.5, -1.0), (0.0, 1.0)], false),
        line(&[(-1.5, 1.0), (1.5, 1.0)]),
        line(&[(0.0, -2.0), (0.0, -1.0)]),
        line(&[(0.0, 1.0), (0.0, 2.0)]),
    ]
}

fn diode(id: &str, name: &str, category: &str, value: &str, extra: &[(f64, f64)]) -> SymbolDef {
    let mut graphics = diode_graphics();
    if !extra.is_empty() {
        graphics.push(line(extra));
    }
    symbol(
        Meta {
            id,
            name,
            category,
            prefix: "D",
            value,
        },
        graphics,
        vec![
            pin("1", "A", Passive, (0, -4), Down, 2),
            pin("2", "K", Passive, (0, 4), Up, 2),
        ],
    )
}

fn bjt(id: &str, name: &str, value: &str, npn: bool) -> SymbolDef {
    let arrow = if npn {
        polygon(&[(2.2, 2.2), (1.0, 2.0), (1.8, 1.2)], true)
    } else {
        polygon(&[(0.0, 0.8), (1.2, 0.9), (0.5, 1.8)], true)
    };
    symbol(
        Meta {
            id,
            name,
            category: "transistors",
            prefix: "Q",
            value,
        },
        vec![
            circle((1.0, 0.0), 3.0, false),
            line(&[(0.0, -2.0), (0.0, 2.0)]),
            line(&[(0.0, -0.8), (2.0, -2.0)]),
            line(&[(0.0, 0.8), (2.0, 2.0)]),
            arrow,
        ],
        vec![
            pin("1", "B", Input, (-4, 0), Right, 4),
            pin("2", "E", Passive, (2, 4), Up, 2),
            pin("3", "C", Passive, (2, -4), Down, 2),
        ],
    )
}

fn mosfet(id: &str, name: &str, value: &str, n_channel: bool) -> SymbolDef {
    let arrow = if n_channel {
        polygon(&[(0.6, 0.0), (1.6, -0.5), (1.6, 0.5)], true)
    } else {
        polygon(&[(1.6, 0.0), (0.6, -0.5), (0.6, 0.5)], true)
    };
    symbol(
        Meta {
            id,
            name,
            category: "mosfets",
            prefix: "Q",
            value,
        },
        vec![
            circle((1.0, 0.0), 3.0, false),
            line(&[(-0.4, -1.5), (-0.4, 1.5)]),
            line(&[(0.3, -1.8), (0.3, -0.8)]),
            line(&[(0.3, -0.5), (0.3, 0.5)]),
            line(&[(0.3, 0.8), (0.3, 1.8)]),
            line(&[(0.3, -1.3), (2.0, -1.3), (2.0, -2.0)]),
            line(&[(0.3, 0.0), (2.0, 0.0), (2.0, 1.3), (0.3, 1.3)]),
            line(&[(2.0, 1.3), (2.0, 2.0)]),
            arrow,
        ],
        vec![
            pin("1", "G", Input, (-4, 0), Right, 4),
            pin("2", "S", Passive, (2, 4), Up, 2),
            pin("3", "D", Passive, (2, -4), Down, 2),
        ],
    )
}

fn logic_gate(id: &str, name: &str, sign: &str, inverted: bool, inputs: usize) -> SymbolDef {
    let mut graphics = vec![rect((-3.0, -4.0), (3.0, 4.0)), text((-1.0, 0.4), sign)];
    if inverted {
        graphics.push(circle((3.5, 0.0), 0.5, false));
    }
    let mut pins = Vec::new();
    let rows: &[i64] = if inputs == 1 { &[0] } else { &[-2, 2] };
    for (index, row) in rows.iter().enumerate() {
        let number = (index + 1).to_string();
        let label = ["A", "B"][index];
        pins.push(pin(&number, label, Input, (-6, *row), Right, 3));
    }
    let output = (rows.len() + 1).to_string();
    pins.push(pin(&output, "Y", Output, (6, 0), Left, 2));
    let vcc = (rows.len() + 2).to_string();
    let gnd = (rows.len() + 3).to_string();
    pins.push(pin(&vcc, "VCC", PowerIn, (0, -6), Down, 2));
    pins.push(pin(&gnd, "GND", PowerIn, (0, 6), Up, 2));
    symbol(
        Meta {
            id,
            name,
            category: "logic",
            prefix: "U",
            value: name,
        },
        graphics,
        pins,
    )
}

fn supply(id: &str, name: &str, value: &str, ground: bool) -> SymbolDef {
    let (graphics, pin_at, direction) = if ground {
        (
            vec![
                line(&[(0.0, 1.0), (0.0, 2.0)]),
                line(&[(-1.5, 2.0), (1.5, 2.0)]),
                line(&[(-1.0, 2.6), (1.0, 2.6)]),
                line(&[(-0.5, 3.2), (0.5, 3.2)]),
            ],
            (0, 0),
            Down,
        )
    } else {
        (
            vec![
                line(&[(0.0, -1.0), (0.0, -2.0)]),
                line(&[(-1.5, -2.0), (1.5, -2.0)]),
            ],
            (0, 0),
            Up,
        )
    };
    let mut definition = symbol(
        Meta {
            id,
            name,
            category: "supply",
            prefix: "#PWR",
            value,
        },
        graphics,
        vec![pin("1", value, SupplyPort, pin_at, direction, 1)],
    );
    definition.pins[0].hide_number = true;
    definition.kind = SymbolKind::SupplyPort;
    definition
}

fn part(
    id: &str,
    name: &str,
    category: &str,
    prefix: &str,
    pins: &[(&str, &str, PinKind, PinSide)],
) -> SymbolDef {
    rectangular_part(&CustomPartSpec {
        id: id.to_owned(),
        name: name.to_owned(),
        category: category.to_owned(),
        reference_prefix: prefix.to_owned(),
        value: name.to_owned(),
        pins: pins
            .iter()
            .map(|&(number, name, kind, side)| SpecPin {
                number: number.to_owned(),
                name: name.to_owned(),
                kind,
                side,
            })
            .collect(),
        provenance: None,
    })
}

fn connector(count: usize) -> SymbolDef {
    let id = format!("connector:1x{count:02}");
    let name = format!("Conn 1x{count:02}");
    let pins: Vec<SpecPin> = (1..=count)
        .map(|n| SpecPin {
            number: n.to_string(),
            name: n.to_string(),
            kind: Passive,
            side: PinSide::Right,
        })
        .collect();
    rectangular_part(&CustomPartSpec {
        id,
        name: name.clone(),
        category: "connectors".to_owned(),
        reference_prefix: "J".to_owned(),
        value: name,
        pins,
        provenance: None,
    })
}

const L: PinSide = PinSide::Left;
const R: PinSide = PinSide::Right;

/// All built-in symbols.
pub fn builtin() -> Vec<SymbolDef> {
    let resistor_body = || vec![rect((-1.0, -2.0), (1.0, 2.0))];
    let mut pot = symbol(
        Meta {
            id: "device:potentiometer",
            name: "Potentiometer",
            category: "resistors",
            prefix: "RV",
            value: "10k",
        },
        vec![
            rect((-1.0, -2.0), (1.0, 2.0)),
            polygon(&[(1.0, 0.0), (2.0, -0.5), (2.0, 0.5)], true),
        ],
        vec![
            pin("1", "1", Passive, (0, -4), Down, 2),
            pin("2", "W", Passive, (4, 0), Left, 2),
            pin("3", "3", Passive, (0, 4), Up, 2),
        ],
    );
    pot.description = "Variable resistor with wiper".to_owned();

    let opamp = symbol(
        Meta {
            id: "analog:opamp",
            name: "Op-amp",
            category: "opamps",
            prefix: "U",
            value: "OpAmp",
        },
        vec![
            polygon(&[(-4.0, -4.0), (4.0, 0.0), (-4.0, 4.0)], false),
            text((-3.4, -1.5), "+"),
            text((-3.4, 2.5), "−"),
        ],
        vec![
            pin("1", "OUT", Output, (6, 0), Left, 2),
            pin("2", "IN-", Input, (-6, 2), Right, 2),
            pin("3", "IN+", Input, (-6, -2), Right, 2),
            pin("4", "V-", PowerIn, (0, 4), Up, 2),
            pin("8", "V+", PowerIn, (0, -4), Down, 2),
        ],
    );

    let regulator = symbol(
        Meta {
            id: "power:regulator_linear",
            name: "Linear regulator",
            category: "regulators",
            prefix: "U",
            value: "Regulator",
        },
        vec![rect((-4.0, -3.0), (4.0, 3.0))],
        vec![
            pin("1", "IN", PowerIn, (-6, 0), Right, 2),
            pin("2", "GND", PowerIn, (0, 5), Up, 2),
            pin("3", "OUT", PowerOut, (6, 0), Left, 2),
        ],
    );
    let mut regulator_7805 = regulator.clone();
    regulator_7805.id = "power:7805".to_owned();
    regulator_7805.name = "7805".to_owned();
    regulator_7805.default_value = "7805".to_owned();

    let relay = symbol(
        Meta {
            id: "relay:spdt",
            name: "Relay SPDT",
            category: "relays",
            prefix: "K",
            value: "Relay",
        },
        vec![
            rect((-7.0, -2.0), (-3.0, 2.0)),
            line(&[(-7.0, 2.0), (-3.0, -2.0)]),
            line(&[(4.0, 2.0), (6.5, -1.5)]),
        ],
        vec![
            pin("1", "A1", Passive, (-5, -4), Down, 2),
            pin("2", "A2", Passive, (-5, 4), Up, 2),
            pin("3", "COM", Passive, (4, 4), Up, 2),
            pin("4", "NO", Passive, (8, -4), Down, 2),
            pin("5", "NC", Passive, (2, -4), Down, 2),
        ],
    );

    let mut mcu_pins: Vec<(String, String, PinKind, PinSide)> = vec![
        ("1".into(), "VDD".into(), PowerIn, L),
        ("2".into(), "VSS".into(), PowerIn, L),
        ("3".into(), "RESET".into(), Input, L),
        ("4".into(), "NC".into(), NotConnected, L),
    ];
    for bit in 0..8 {
        mcu_pins.push((format!("{}", 5 + bit), format!("PA{bit}"), Bidirectional, L));
    }
    for bit in 0..12 {
        mcu_pins.push((
            format!("{}", 13 + bit),
            format!("PB{bit}"),
            Bidirectional,
            R,
        ));
    }
    let mcu_spec: Vec<(&str, &str, PinKind, PinSide)> = mcu_pins
        .iter()
        .map(|(n, a, k, s)| (n.as_str(), a.as_str(), *k, *s))
        .collect();

    let mut cpu_pins: Vec<(String, String, PinKind, PinSide)> = vec![
        ("1".into(), "VDD".into(), PowerIn, L),
        ("2".into(), "VSS".into(), PowerIn, L),
        ("3".into(), "CLK".into(), Input, L),
        ("4".into(), "RESET".into(), Input, L),
        ("5".into(), "RD".into(), Output, L),
        ("6".into(), "WR".into(), Output, L),
    ];
    for bit in 0..16 {
        cpu_pins.push((format!("{}", 7 + bit), format!("A{bit}"), Output, R));
    }
    for bit in 0..8 {
        cpu_pins.push((format!("{}", 23 + bit), format!("D{bit}"), Bidirectional, L));
    }
    let cpu_spec: Vec<(&str, &str, PinKind, PinSide)> = cpu_pins
        .iter()
        .map(|(n, a, k, s)| (n.as_str(), a.as_str(), *k, *s))
        .collect();

    let mut lcd_pins: Vec<(String, String, PinKind, PinSide)> = vec![
        ("1".into(), "VSS".into(), PowerIn, L),
        ("2".into(), "VDD".into(), PowerIn, L),
        ("3".into(), "V0".into(), Input, L),
        ("4".into(), "RS".into(), Input, L),
        ("5".into(), "RW".into(), Input, L),
        ("6".into(), "E".into(), Input, L),
        ("15".into(), "LED+".into(), Passive, L),
        ("16".into(), "LED-".into(), Passive, L),
    ];
    for bit in 0..8 {
        lcd_pins.push((format!("{}", 7 + bit), format!("D{bit}"), Bidirectional, R));
    }
    let lcd_spec: Vec<(&str, &str, PinKind, PinSide)> = lcd_pins
        .iter()
        .map(|(n, a, k, s)| (n.as_str(), a.as_str(), *k, *s))
        .collect();

    let mut symbols = vec![
        two_pin_v(
            "device:resistor",
            "Resistor",
            "resistors",
            "R",
            "10k",
            resistor_body(),
        ),
        pot,
        two_pin_v(
            "device:capacitor",
            "Capacitor",
            "capacitors",
            "C",
            "100n",
            vec![
                line(&[(-2.0, -0.5), (2.0, -0.5)]),
                line(&[(-2.0, 0.5), (2.0, 0.5)]),
                line(&[(0.0, -2.0), (0.0, -0.5)]),
                line(&[(0.0, 0.5), (0.0, 2.0)]),
            ],
        ),
        two_pin_v(
            "device:capacitor_polarized",
            "Capacitor, polarized",
            "capacitors",
            "C",
            "10u",
            vec![
                line(&[(-2.0, -0.5), (2.0, -0.5)]),
                rect((-2.0, 0.5), (2.0, 1.0)),
                text((-1.8, -1.0), "+"),
                line(&[(0.0, -2.0), (0.0, -0.5)]),
                line(&[(0.0, 1.0), (0.0, 2.0)]),
            ],
        ),
        two_pin_v(
            "device:inductor",
            "Inductor",
            "inductors",
            "L",
            "10u",
            vec![
                arc((0.0, -1.5), 0.5, -90, 90),
                arc((0.0, -0.5), 0.5, -90, 90),
                arc((0.0, 0.5), 0.5, -90, 90),
                arc((0.0, 1.5), 0.5, -90, 90),
            ],
        ),
        diode("device:diode", "Diode", "diodes", "1N4148", &[]),
        diode(
            "device:diode_zener",
            "Zener diode",
            "diodes",
            "Zener",
            &[(-1.5, 1.0), (-1.5, 0.6)],
        ),
        diode(
            "device:diode_schottky",
            "Schottky diode",
            "diodes",
            "Schottky",
            &[(-1.5, 0.6), (-1.5, 1.0), (1.5, 1.0), (1.5, 1.4)],
        ),
        {
            let mut led = diode(
                "device:led",
                "LED",
                "leds",
                "LED",
                &[(2.0, -0.5), (3.2, -1.5)],
            );
            led.graphics.push(line(&[(2.0, 0.5), (3.2, -0.5)]));
            led
        },
        bjt("device:npn", "NPN transistor", "NPN", true),
        bjt("device:pnp", "PNP transistor", "PNP", false),
        mosfet("device:nmos", "N-channel MOSFET", "NMOS", true),
        mosfet("device:pmos", "P-channel MOSFET", "PMOS", false),
        opamp,
        part(
            "analog:lm358",
            "LM358",
            "opamps",
            "U",
            &[
                ("1", "OUT1", Output, R),
                ("2", "IN1-", Input, L),
                ("3", "IN1+", Input, L),
                ("4", "V-", PowerIn, L),
                ("5", "IN2+", Input, L),
                ("6", "IN2-", Input, L),
                ("7", "OUT2", Output, R),
                ("8", "V+", PowerIn, R),
            ],
        ),
        part(
            "mcu:generic_24",
            "Microcontroller, generic 24 pin",
            "microcontrollers",
            "U",
            &mcu_spec,
        ),
        part(
            "cpu:generic_30",
            "Processor, generic 30 pin",
            "processors",
            "U",
            &cpu_spec,
        ),
        regulator,
        regulator_7805,
        relay,
        part(
            "sensor:analog",
            "Sensor, analog output",
            "sensors",
            "U",
            &[
                ("1", "VCC", PowerIn, L),
                ("2", "GND", PowerIn, L),
                ("3", "OUT", Output, R),
            ],
        ),
        part(
            "sensor:i2c",
            "Sensor, I2C",
            "sensors",
            "U",
            &[
                ("1", "VCC", PowerIn, L),
                ("2", "GND", PowerIn, L),
                ("3", "SDA", Bidirectional, R),
                ("4", "SCL", Input, R),
            ],
        ),
        part(
            "display:i2c",
            "Display, I2C",
            "displays",
            "DS",
            &[
                ("1", "VCC", PowerIn, L),
                ("2", "GND", PowerIn, L),
                ("3", "SDA", Bidirectional, R),
                ("4", "SCL", Input, R),
            ],
        ),
        part(
            "display:char_16x2",
            "Character LCD 16x2",
            "displays",
            "DS",
            &lcd_spec,
        ),
        part(
            "timer:ne555",
            "NE555",
            "logic",
            "U",
            &[
                ("1", "GND", PowerIn, L),
                ("2", "TRIG", Input, L),
                ("4", "RESET", Input, L),
                ("5", "CTRL", Input, L),
                ("8", "VCC", PowerIn, R),
                ("3", "OUT", Output, R),
                ("6", "THR", Input, R),
                ("7", "DIS", OpenCollector, R),
            ],
        ),
        logic_gate("logic:not", "Inverter", "1", true, 1),
        logic_gate("logic:and2", "AND 2", "&", false, 2),
        logic_gate("logic:nand2", "NAND 2", "&", true, 2),
        logic_gate("logic:or2", "OR 2", "≥1", false, 2),
        logic_gate("logic:nor2", "NOR 2", "≥1", true, 2),
        logic_gate("logic:xor2", "XOR 2", "=1", false, 2),
        supply("supply:gnd", "GND", "GND", true),
        supply("supply:vcc", "VCC", "VCC", false),
        supply("supply:5v", "+5V", "+5V", false),
        supply("supply:3v3", "+3V3", "+3V3", false),
        supply("supply:12v", "+12V", "+12V", false),
    ];
    for count in [2, 3, 4, 6, 8, 10] {
        symbols.push(connector(count));
    }
    for (id, name, value, sign) in [
        ("sim:vsource", "Voltage source", "SIN(0 1 1k)", "V"),
        ("sim:isource", "Current source", "1m", "I"),
    ] {
        symbols.push(symbol(
            Meta {
                id,
                name,
                category: "simulation",
                prefix: sign,
                value,
            },
            vec![circle((0.0, 0.0), 2.0, false), text((0.0, 0.0), sign)],
            vec![
                pin("1", "+", Passive, (0, -4), Down, 2),
                pin("2", "-", Passive, (0, 4), Up, 2),
            ],
        ));
    }
    // Default footprints. The pin numbers of these symbols match the pad
    // numbers. Symbols without a matching package stay unassigned.
    for symbol in &mut symbols {
        let footprint = match symbol.id.as_str() {
            "device:resistor"
            | "device:capacitor"
            | "device:inductor"
            | "device:diode"
            | "device:diode_zener"
            | "device:diode_schottky"
            | "device:led" => Some("chip:0805".to_owned()),
            "device:capacitor_polarized" => Some("chip:1206".to_owned()),
            "device:potentiometer" | "sensor:analog" => Some("header:1x03".to_owned()),
            "device:npn" | "device:pnp" | "device:nmos" | "device:pmos" => {
                Some("sot:23".to_owned())
            }
            "analog:opamp" | "analog:lm358" | "timer:ne555" => Some("soic:8".to_owned()),
            "power:7805" | "power:regulator_linear" => Some("to:220_vertical".to_owned()),
            "mcu:generic_24" => Some("soic:24w".to_owned()),
            "sensor:i2c" | "display:i2c" => Some("header:1x04".to_owned()),
            "display:char_16x2" => Some("header:1x16".to_owned()),
            id => id
                .strip_prefix("connector:1x")
                .map(|count| format!("header:1x{count}")),
        };
        symbol.part.footprint = footprint;
    }
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashSet};
    use tronic_schematic::{validate_symbol, GRID};

    #[test]
    fn every_required_category_has_a_symbol() {
        let symbols = builtin();
        for category in CATEGORIES {
            assert!(
                symbols.iter().any(|symbol| symbol.category == category),
                "{category}"
            );
        }
    }

    #[test]
    fn categories_are_all_known() {
        let known: BTreeSet<_> = CATEGORIES.iter().copied().collect();
        for symbol in builtin() {
            assert!(known.contains(symbol.category.as_str()), "{}", symbol.id);
        }
    }

    #[test]
    fn symbols_pass_validation_with_unique_ids() {
        let mut ids = HashSet::new();
        for symbol in builtin() {
            validate_symbol(&symbol).unwrap_or_else(|error| {
                assert!(error.is_empty(), "{error}");
            });
            assert!(ids.insert(symbol.id.clone()), "duplicate id {}", symbol.id);
        }
    }

    #[test]
    fn pins_lie_on_the_grid_so_that_wires_can_reach_them() {
        for symbol in builtin() {
            for pin in &symbol.pins {
                assert_eq!(pin.at.x % GRID, 0, "{} pin {}", symbol.id, pin.number);
                assert_eq!(pin.at.y % GRID, 0, "{} pin {}", symbol.id, pin.number);
            }
        }
    }

    #[test]
    fn pins_of_one_symbol_do_not_overlap() {
        for symbol in builtin() {
            let mut points = HashSet::new();
            for pin in &symbol.pins {
                assert!(points.insert(pin.at), "{} pin {}", symbol.id, pin.number);
            }
        }
    }

    #[test]
    fn every_symbol_has_provenance_and_none_claims_a_review() {
        for symbol in builtin() {
            assert_eq!(symbol.provenance.source, SOURCE);
            assert!(!symbol.provenance.license.is_empty());
            assert!(!symbol.provenance.reviewed, "{}", symbol.id);
        }
    }

    #[test]
    fn known_pinouts_match_their_common_assignment() {
        let symbols = builtin();
        let name_of = |id: &str, number: &str| -> String {
            symbols
                .iter()
                .find(|symbol| symbol.id == id)
                .and_then(|symbol| symbol.pins.iter().find(|pin| pin.number == number))
                .map(|pin| pin.name.clone())
                .unwrap_or_default()
        };
        assert_eq!(name_of("timer:ne555", "1"), "GND");
        assert_eq!(name_of("timer:ne555", "3"), "OUT");
        assert_eq!(name_of("timer:ne555", "8"), "VCC");
        assert_eq!(name_of("analog:lm358", "4"), "V-");
        assert_eq!(name_of("analog:lm358", "8"), "V+");
        assert_eq!(name_of("power:7805", "1"), "IN");
        assert_eq!(name_of("power:7805", "3"), "OUT");
    }
}
