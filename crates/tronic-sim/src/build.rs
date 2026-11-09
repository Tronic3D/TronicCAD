//! Turns a schematic into a circuit, and says exactly what it did.

use serde::Serialize;
use std::collections::BTreeMap;
use tronic_schematic::{Connectivity, Schematic, SymbolKind};

use crate::circuit::*;
use crate::value::{parse_source, parse_value};

const GROUND_NAMES: [&str; 6] = ["GND", "0", "AGND", "DGND", "GNDA", "VSS"];

/// One modelled part with the parameters used, for the user to read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelEntry {
    pub reference: String,
    /// Which model: the interface translates this.
    pub kind: &'static str,
    /// Value and parameters, in a form that needs no translation.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct BuildReport {
    pub models: Vec<ModelEntry>,
    /// Parts that do something electrically but have no simulation model.
    pub unsupported: Vec<String>,
    /// Connectors and the like, which are simply open in the simulation.
    pub ignored: Vec<String>,
    /// Parts whose value could not be read: (reference, value).
    pub bad_values: Vec<(String, String)>,
    /// Supply nets without a known voltage.
    pub unknown_supplies: Vec<String>,
    /// Voltages assumed for supply nets.
    pub supplies: Vec<(String, f64)>,
    pub has_ground: bool,
}

impl BuildReport {
    /// Whether results would be missing parts or supplies.
    pub fn is_complete(&self) -> bool {
        self.unsupported.is_empty()
            && self.bad_values.is_empty()
            && self.unknown_supplies.is_empty()
            && self.has_ground
    }
}

/// Whether a net name means ground.
pub fn is_ground_name(name: &str) -> bool {
    GROUND_NAMES.contains(&name)
}

/// The voltage a supply net name states, like `+5V`, `3V3` or `-12V`.
pub fn supply_voltage(name: &str) -> Option<f64> {
    let cleaned = name.trim_start_matches('+');
    if !cleaned.chars().any(|c| c == 'V' || c == 'v') {
        return None;
    }
    parse_value(cleaned)
}

const D_SIGNAL: DiodeModel = DiodeModel {
    is: 2.52e-9,
    n: 1.752,
    bv: f64::INFINITY,
};
const D_SCHOTTKY: DiodeModel = DiodeModel {
    is: 1e-6,
    n: 1.05,
    bv: f64::INFINITY,
};
const D_LED: DiodeModel = DiodeModel {
    is: 1e-18,
    n: 2.0,
    bv: f64::INFINITY,
};
const Q_DEFAULT: BjtModel = BjtModel {
    is: 1e-14,
    bf: 100.0,
    br: 1.0,
};
const M_DEFAULT: MosModel = MosModel {
    kp: 0.1,
    vto: 2.0,
    lambda: 0.0,
};
const OPAMP_GAIN: f64 = 1e5;

/// Builds the circuit. `supplies` gives voltages for nets whose name states none.
pub fn circuit_from_schematic(
    doc: &Schematic,
    supplies: &BTreeMap<String, f64>,
) -> (Circuit, BuildReport) {
    let connectivity = Connectivity::analyze(doc);
    let mut report = BuildReport::default();
    let mut circuit = Circuit::new();

    // Nets by component and pin; all ground names become node 0.
    let is_ground = |net: &tronic_schematic::Net| {
        net.supply_names
            .iter()
            .chain(std::iter::once(&net.name))
            .any(|name| GROUND_NAMES.contains(&name.as_str()))
    };
    let mut node_of: BTreeMap<(u64, String), usize> = BTreeMap::new();
    for net in &connectivity.nets {
        let ground = is_ground(net);
        report.has_ground |= ground && !net.pins.is_empty();
        let node = if ground {
            GROUND
        } else {
            circuit.node(&net.name)
        };
        for pin in &net.pins {
            node_of.insert((pin.component, pin.number.clone()), node);
        }
        if ground {
            continue;
        }
        for supply in &net.supply_names {
            let known = supplies
                .get(supply)
                .copied()
                .or_else(|| supply_voltage(supply));
            match known {
                Some(volts) => {
                    report.supplies.push((supply.clone(), volts));
                    circuit.add(
                        &format!("V_{supply}"),
                        Element::Voltage {
                            plus: node,
                            minus: GROUND,
                            source: Source::dc(volts),
                        },
                    );
                }
                None => report.unknown_supplies.push(supply.clone()),
            }
        }
    }

    for sheet in &doc.sheets {
        for component in &sheet.components {
            let Some(symbol) = doc.symbols.get(&component.symbol) else {
                continue;
            };
            if symbol.kind == SymbolKind::SupplyPort {
                continue;
            }
            let reference = component.reference.as_str();
            let pin = |number: &str| node_of.get(&(component.id, number.to_owned())).copied();
            let mut model = |kind: &'static str, detail: String| {
                report.models.push(ModelEntry {
                    reference: reference.to_owned(),
                    kind,
                    detail,
                });
            };
            let value = component.value.as_str();
            let two = pin("1").zip(pin("2"));
            match symbol.id.as_str() {
                "device:resistor"
                | "device:capacitor"
                | "device:capacitor_polarized"
                | "device:inductor" => {
                    let (Some((a, b)), Some(parsed)) = (two, parse_value(value)) else {
                        report
                            .bad_values
                            .push((reference.to_owned(), value.to_owned()));
                        continue;
                    };
                    if parsed <= 0.0 {
                        report
                            .bad_values
                            .push((reference.to_owned(), value.to_owned()));
                        continue;
                    }
                    let element = match symbol.id.as_str() {
                        "device:resistor" => Element::Resistor { a, b, ohms: parsed },
                        "device:inductor" => Element::Inductor {
                            a,
                            b,
                            henries: parsed,
                        },
                        _ => Element::Capacitor {
                            a,
                            b,
                            farads: parsed,
                        },
                    };
                    model("ideal", value.to_owned());
                    circuit.add(reference, element);
                }
                "device:potentiometer" => {
                    let (Some(a), Some(w), Some(b), Some(total)) =
                        (pin("1"), pin("W"), pin("3"), parse_value(value))
                    else {
                        // The wiper's number depends on the symbol; check it too.
                        match (pin("1"), pin("2"), pin("3"), parse_value(value)) {
                            (Some(a), Some(w), Some(b), Some(total)) if total > 0.0 => {
                                model("potentiometer", value.to_owned());
                                circuit.add(
                                    reference,
                                    Element::Resistor {
                                        a,
                                        b: w,
                                        ohms: total / 2.0,
                                    },
                                );
                                circuit.add(
                                    &format!("{reference}_b"),
                                    Element::Resistor {
                                        a: w,
                                        b,
                                        ohms: total / 2.0,
                                    },
                                );
                            }
                            _ => report
                                .bad_values
                                .push((reference.to_owned(), value.to_owned())),
                        }
                        continue;
                    };
                    model("potentiometer", value.to_owned());
                    circuit.add(
                        reference,
                        Element::Resistor {
                            a,
                            b: w,
                            ohms: total / 2.0,
                        },
                    );
                    circuit.add(
                        &format!("{reference}_b"),
                        Element::Resistor {
                            a: w,
                            b,
                            ohms: total / 2.0,
                        },
                    );
                }
                "device:diode" | "device:diode_schottky" | "device:diode_zener" | "device:led" => {
                    let Some((anode, cathode)) = two else {
                        continue;
                    };
                    let chosen = match symbol.id.as_str() {
                        "device:led" => D_LED,
                        "device:diode_schottky" => D_SCHOTTKY,
                        "device:diode_zener" => DiodeModel {
                            bv: parse_value(value).filter(|v| *v > 0.0).unwrap_or(5.1),
                            ..D_SIGNAL
                        },
                        _ => D_SIGNAL,
                    };
                    model(
                        "diode",
                        format!(
                            "Is = {:e} A, n = {}{}",
                            chosen.is,
                            chosen.n,
                            if chosen.bv.is_finite() {
                                format!(", Bv = {} V", chosen.bv)
                            } else {
                                String::new()
                            }
                        ),
                    );
                    circuit.add(
                        reference,
                        Element::Diode {
                            anode,
                            cathode,
                            model: chosen,
                        },
                    );
                }
                "device:npn" | "device:pnp" => {
                    let (Some(base), Some(emitter), Some(collector)) =
                        (pin("1"), pin("2"), pin("3"))
                    else {
                        continue;
                    };
                    model(
                        "bjt",
                        format!(
                            "Is = {:e} A, Bf = {}, Br = {}",
                            Q_DEFAULT.is, Q_DEFAULT.bf, Q_DEFAULT.br
                        ),
                    );
                    circuit.add(
                        reference,
                        Element::Bjt {
                            collector,
                            base,
                            emitter,
                            pnp: symbol.id == "device:pnp",
                            model: Q_DEFAULT,
                        },
                    );
                }
                "device:nmos" | "device:pmos" => {
                    let (Some(gate), Some(source), Some(drain)) = (pin("1"), pin("2"), pin("3"))
                    else {
                        continue;
                    };
                    model(
                        "mos",
                        format!("Kp = {} A/V², Vto = {} V", M_DEFAULT.kp, M_DEFAULT.vto),
                    );
                    circuit.add(
                        reference,
                        Element::Mos {
                            drain,
                            gate,
                            source,
                            p_channel: symbol.id == "device:pmos",
                            model: M_DEFAULT,
                        },
                    );
                }
                "analog:opamp" => {
                    let (Some(output), Some(minus), Some(plus)) = (pin("1"), pin("2"), pin("3"))
                    else {
                        continue;
                    };
                    model("opamp", format!("A = {OPAMP_GAIN:e}"));
                    circuit.add(
                        reference,
                        Element::Opamp {
                            output,
                            plus,
                            minus,
                            gain: OPAMP_GAIN,
                        },
                    );
                }
                "sim:vsource" | "sim:isource" => {
                    let (Some((plus, minus)), Some(source)) = (two, parse_source(value)) else {
                        report
                            .bad_values
                            .push((reference.to_owned(), value.to_owned()));
                        continue;
                    };
                    model("source", value.to_owned());
                    let element = if symbol.id == "sim:vsource" {
                        Element::Voltage {
                            plus,
                            minus,
                            source,
                        }
                    } else {
                        Element::Current {
                            plus,
                            minus,
                            source,
                        }
                    };
                    circuit.add(reference, element);
                }
                id if id.starts_with("connector:") => report.ignored.push(reference.to_owned()),
                _ => report.unsupported.push(reference.to_owned()),
            }
        }
    }
    (circuit, report)
}
