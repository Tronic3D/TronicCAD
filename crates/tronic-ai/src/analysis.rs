//! Circuit analysis by fixed rules. It needs no model and no network, works the
//! same every time and is easy to check. Its findings are hints, not check
//! results: ERC and DRC stay separate.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use tronic_pcb::{board_view, Board, MM};
use tronic_schematic::{Connectivity, PinKind, Schematic};
use tronic_sim::{is_ground_name, parse_value, supply_voltage};

/// Forward voltage assumed for a LED where its colour is unknown.
const LED_FORWARD_VOLTS: f64 = 2.0;
const LED_MAX_AMPS: f64 = 0.02;
const LED_MIN_AMPS: f64 = 0.0005;
/// Below these, many low-cost manufacturers charge extra or refuse.
const TIGHT_TRACK_MM: f64 = 0.15;
const TIGHT_DRILL_MM: f64 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Stable identifier, also the key of the translated message.
    pub code: &'static str,
    pub level: Level,
    pub params: BTreeMap<&'static str, String>,
}

struct Part {
    reference: String,
    symbol: String,
    value: String,
    /// Pin number to net index.
    pins: BTreeMap<String, usize>,
    power_pins: Vec<String>,
}

struct Circuit {
    parts: Vec<Part>,
    net_names: Vec<BTreeSet<String>>,
}

impl Circuit {
    fn read(doc: &Schematic) -> Self {
        let connectivity = Connectivity::analyze(doc);
        let net_names = connectivity
            .nets
            .iter()
            .map(|net| {
                let mut names = net.supply_names.clone();
                names.insert(net.name.clone());
                names
            })
            .collect();
        let mut net_of: BTreeMap<(u64, String), usize> = BTreeMap::new();
        for (index, net) in connectivity.nets.iter().enumerate() {
            for pin in &net.pins {
                net_of.insert((pin.component, pin.number.clone()), index);
            }
        }
        let mut parts = Vec::new();
        for sheet in &doc.sheets {
            for component in &sheet.components {
                let Some(symbol) = doc.symbols.get(&component.symbol) else {
                    continue;
                };
                let mut pins = BTreeMap::new();
                for pin in &symbol.pins {
                    if let Some(net) = net_of.get(&(component.id, pin.number.clone())) {
                        pins.insert(pin.number.clone(), *net);
                    }
                }
                parts.push(Part {
                    reference: component.reference.clone(),
                    symbol: component.symbol.clone(),
                    value: component.value.clone(),
                    pins,
                    power_pins: symbol
                        .pins
                        .iter()
                        .filter(|pin| pin.kind == PinKind::PowerIn)
                        .map(|pin| pin.number.clone())
                        .collect(),
                });
            }
        }
        Self { parts, net_names }
    }

    fn is_ground(&self, net: usize) -> bool {
        self.net_names[net].iter().any(|name| is_ground_name(name))
    }

    fn voltage(&self, net: usize) -> Option<f64> {
        self.net_names[net]
            .iter()
            .find_map(|name| supply_voltage(name))
    }

    fn name(&self, net: usize) -> String {
        self.net_names[net]
            .iter()
            .next()
            .cloned()
            .unwrap_or_default()
    }

    fn on_net(&self, net: usize) -> impl Iterator<Item = &Part> {
        self.parts
            .iter()
            .filter(move |part| part.pins.values().any(|n| *n == net))
    }

    fn other_net(part: &Part, net: usize) -> Option<usize> {
        part.pins.values().copied().find(|n| *n != net)
    }
}

fn is_resistor(symbol: &str) -> bool {
    symbol == "device:resistor"
}

fn is_capacitor(symbol: &str) -> bool {
    symbol.starts_with("device:capacitor")
}

fn is_led(symbol: &str) -> bool {
    symbol == "device:led"
}

fn number(value: f64, digits: usize) -> String {
    format!("{value:.digits$}")
}

fn led_rules(circuit: &Circuit, out: &mut Vec<Finding>) {
    for led in circuit.parts.iter().filter(|p| is_led(&p.symbol)) {
        let series: Vec<&Part> = led
            .pins
            .values()
            .flat_map(|net| circuit.on_net(*net))
            .filter(|p| is_resistor(&p.symbol))
            .collect();
        let driven = led.pins.values().any(|net| {
            circuit.voltage(*net).is_some()
                || circuit.is_ground(*net)
                || circuit.on_net(*net).count() > 1
        });
        if series.is_empty() && driven {
            out.push(Finding {
                code: "led_without_resistor",
                level: Level::Warning,
                params: BTreeMap::from([("led", led.reference.clone())]),
            });
            continue;
        }
        let Some(resistor) = series.first() else {
            continue;
        };
        let Some(ohms) = parse_value(&resistor.value).filter(|v| *v > 0.0) else {
            continue;
        };
        let nets: Vec<usize> = led.pins.values().copied().collect();
        let Some(shared) = nets
            .iter()
            .copied()
            .find(|net| resistor.pins.values().any(|n| n == net))
        else {
            continue;
        };
        let (Some(far), Some(led_far)) = (
            Circuit::other_net(resistor, shared),
            Circuit::other_net(led, shared),
        ) else {
            continue;
        };
        let (Some(volts), true) = (circuit.voltage(far), circuit.is_ground(led_far)) else {
            continue;
        };
        let amps = (volts - LED_FORWARD_VOLTS) / ohms;
        let code = if amps > LED_MAX_AMPS {
            "led_current_high"
        } else if amps < LED_MIN_AMPS {
            "led_current_low"
        } else {
            "led_current"
        };
        out.push(Finding {
            code,
            level: if code == "led_current" {
                Level::Info
            } else {
                Level::Warning
            },
            params: BTreeMap::from([
                ("led", led.reference.clone()),
                ("resistor", resistor.reference.clone()),
                ("volts", number(volts, 2)),
                ("milliamps", number(amps * 1000.0, 1)),
                ("forward", number(LED_FORWARD_VOLTS, 1)),
            ]),
        });
    }
}

fn decoupling_rule(circuit: &Circuit, out: &mut Vec<Finding>) {
    for part in &circuit.parts {
        let mut supplies: Vec<usize> = part
            .power_pins
            .iter()
            .filter_map(|pin| part.pins.get(pin).copied())
            .filter(|net| !circuit.is_ground(*net))
            .collect();
        supplies.sort_unstable();
        supplies.dedup();
        for net in supplies {
            let decoupled = circuit.on_net(net).any(|other| {
                is_capacitor(&other.symbol)
                    && Circuit::other_net(other, net).is_some_and(|far| circuit.is_ground(far))
            });
            if !decoupled {
                out.push(Finding {
                    code: "no_decoupling",
                    level: Level::Warning,
                    params: BTreeMap::from([
                        ("part", part.reference.clone()),
                        ("net", circuit.name(net)),
                    ]),
                });
            }
        }
    }
}

fn divider_rule(circuit: &Circuit, out: &mut Vec<Finding>) {
    for (index, top) in circuit.parts.iter().enumerate() {
        if !is_resistor(&top.symbol) {
            continue;
        }
        for bottom in &circuit.parts[index + 1..] {
            if !is_resistor(&bottom.symbol) {
                continue;
            }
            let shared = top
                .pins
                .values()
                .copied()
                .find(|net| bottom.pins.values().any(|n| n == net));
            let Some(middle) = shared else {
                continue;
            };
            let (Some(a), Some(b)) = (
                Circuit::other_net(top, middle),
                Circuit::other_net(bottom, middle),
            ) else {
                continue;
            };
            let (upper, lower, source) = if circuit.voltage(a).is_some() && circuit.is_ground(b) {
                (top, bottom, a)
            } else if circuit.voltage(b).is_some() && circuit.is_ground(a) {
                (bottom, top, b)
            } else {
                continue;
            };
            let (Some(r1), Some(r2), Some(volts)) = (
                parse_value(&upper.value),
                parse_value(&lower.value),
                circuit.voltage(source),
            ) else {
                continue;
            };
            if r1 + r2 > 0.0 {
                out.push(Finding {
                    code: "divider",
                    level: Level::Info,
                    params: BTreeMap::from([
                        ("upper", upper.reference.clone()),
                        ("lower", lower.reference.clone()),
                        ("net", circuit.name(middle)),
                        ("volts", number(volts * r2 / (r1 + r2), 3)),
                    ]),
                });
            }
        }
    }
}

fn filter_rule(circuit: &Circuit, out: &mut Vec<Finding>) {
    for resistor in circuit.parts.iter().filter(|p| is_resistor(&p.symbol)) {
        let Some(ohms) = parse_value(&resistor.value) else {
            continue;
        };
        for net in resistor.pins.values() {
            // A capacitor on a supply is decoupling, not a filter with this resistor.
            if circuit.voltage(*net).is_some() || circuit.is_ground(*net) {
                continue;
            }
            for capacitor in circuit.on_net(*net).filter(|p| is_capacitor(&p.symbol)) {
                let grounded =
                    Circuit::other_net(capacitor, *net).is_some_and(|far| circuit.is_ground(far));
                let Some(farads) = parse_value(&capacitor.value) else {
                    continue;
                };
                if grounded && ohms > 0.0 && farads > 0.0 {
                    out.push(Finding {
                        code: "rc_corner",
                        level: Level::Info,
                        params: BTreeMap::from([
                            ("resistor", resistor.reference.clone()),
                            ("capacitor", capacitor.reference.clone()),
                            (
                                "hertz",
                                number(1.0 / (std::f64::consts::TAU * ohms * farads), 1),
                            ),
                        ]),
                    });
                }
            }
        }
    }
}

fn transistor_rule(circuit: &Circuit, out: &mut Vec<Finding>) {
    for part in &circuit.parts {
        if !matches!(part.symbol.as_str(), "device:npn" | "device:pnp") {
            continue;
        }
        // Pin 1 is the base.
        let Some(base) = part.pins.get("1").copied() else {
            continue;
        };
        let protected = circuit.on_net(base).any(|other| is_resistor(&other.symbol));
        if !protected {
            out.push(Finding {
                code: "base_without_resistor",
                level: Level::Warning,
                params: BTreeMap::from([("part", part.reference.clone())]),
            });
        }
    }
}

fn opamp_rule(circuit: &Circuit, out: &mut Vec<Finding>) {
    for part in circuit.parts.iter().filter(|p| p.symbol == "analog:opamp") {
        // Pin 1 is the output, pin 2 the inverting input.
        let (Some(output), Some(inverting)) =
            (part.pins.get("1").copied(), part.pins.get("2").copied())
        else {
            continue;
        };
        let feedback = inverting == output
            || circuit.on_net(output).any(|other| {
                other.reference != part.reference
                    && other.pins.values().any(|net| *net == inverting)
            });
        if !feedback {
            out.push(Finding {
                code: "opamp_no_feedback",
                level: Level::Warning,
                params: BTreeMap::from([("part", part.reference.clone())]),
            });
        }
    }
}

fn board_rules(board: &Board, out: &mut Vec<Finding>) {
    let view = board_view(board);
    if view.footprints.is_empty() {
        return;
    }
    let mm = |nm: i64| nm as f64 / MM as f64;
    if view.outline.len() < 3 {
        out.push(Finding {
            code: "board_no_outline",
            level: Level::Warning,
            params: BTreeMap::new(),
        });
    }
    if !view.airwires.is_empty() {
        out.push(Finding {
            code: "board_open",
            level: Level::Warning,
            params: BTreeMap::from([("count", view.airwires.len().to_string())]),
        });
    }
    if mm(board.rules.min_track_width) < TIGHT_TRACK_MM {
        out.push(Finding {
            code: "fab_thin_tracks",
            level: Level::Info,
            params: BTreeMap::from([
                ("value", number(mm(board.rules.min_track_width), 3)),
                ("limit", number(TIGHT_TRACK_MM, 2)),
            ]),
        });
    }
    if mm(board.rules.via_drill) < TIGHT_DRILL_MM {
        out.push(Finding {
            code: "fab_small_vias",
            level: Level::Info,
            params: BTreeMap::from([
                ("value", number(mm(board.rules.via_drill), 3)),
                ("limit", number(TIGHT_DRILL_MM, 2)),
            ]),
        });
    }
    let holes = view
        .footprints
        .iter()
        .filter(|part| part.footprint.starts_with("hole:"))
        .count();
    if holes == 0 {
        out.push(Finding {
            code: "board_no_mounting_holes",
            level: Level::Info,
            params: BTreeMap::new(),
        });
    }
}

/// Everything the rules find, warnings first.
pub fn analyze(doc: &Schematic, board: &Board) -> Vec<Finding> {
    let circuit = Circuit::read(doc);
    let mut out = Vec::new();
    led_rules(&circuit, &mut out);
    decoupling_rule(&circuit, &mut out);
    transistor_rule(&circuit, &mut out);
    opamp_rule(&circuit, &mut out);
    divider_rule(&circuit, &mut out);
    filter_rule(&circuit, &mut out);
    board_rules(board, &mut out);
    out.sort_by_key(|finding| finding.level);
    out
}
