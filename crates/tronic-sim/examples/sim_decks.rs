//! Writes circuits as SPICE decks together with TronicCAD's results, for
//! `scripts/verify_sim.py`, which runs the decks through ngspice and compares.
//! Usage: `cargo run -p tronic-sim --example sim_decks -- <directory>`

#![allow(clippy::panic)]

use tronic_sim::*;

struct Case {
    name: &'static str,
    circuit: Circuit,
    analysis: Analysis,
    command: String,
}

fn pulse(v1: f64, v2: f64, width: f64, period: f64) -> Source {
    Source {
        dc: 0.0,
        dc_given: false,
        ac: 0.0,
        wave: Some(Waveform::Pulse {
            v1,
            v2,
            delay: 0.0,
            rise: 1e-7,
            fall: 1e-7,
            width,
            period,
        }),
    }
}

fn dc(volts: f64) -> Source {
    Source::dc(volts)
}

const DIODE: DiodeModel = DiodeModel {
    is: 2.52e-9,
    n: 1.752,
    bv: f64::INFINITY,
};
const NPN: BjtModel = BjtModel {
    is: 1e-14,
    bf: 100.0,
    br: 1.0,
};
const NMOS: MosModel = MosModel {
    kp: 0.1,
    vto: 2.0,
    lambda: 0.0,
};

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let op = |name, circuit| Case {
        name,
        circuit,
        analysis: Analysis::OperatingPoint,
        command: "op".to_owned(),
    };

    let mut c = Circuit::new();
    let (a, b, m) = (c.node("a"), c.node("b"), c.node("m"));
    c.add(
        "V1",
        Element::Voltage {
            plus: a,
            minus: GROUND,
            source: dc(12.0),
        },
    );
    c.add(
        "V2",
        Element::Voltage {
            plus: b,
            minus: GROUND,
            source: dc(6.0),
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a,
            b: m,
            ohms: 2000.0,
        },
    );
    c.add(
        "R2",
        Element::Resistor {
            a: b,
            b: m,
            ohms: 1000.0,
        },
    );
    c.add(
        "R3",
        Element::Resistor {
            a: m,
            b: GROUND,
            ohms: 4000.0,
        },
    );
    out.push(op("network", c));

    let mut c = Circuit::new();
    let (i, d) = (c.node("in"), c.node("d"));
    c.add(
        "V1",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: dc(5.0),
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: i,
            b: d,
            ohms: 1000.0,
        },
    );
    c.add(
        "D1",
        Element::Diode {
            anode: d,
            cathode: GROUND,
            model: DIODE,
        },
    );
    out.push(op("diode", c));

    let mut c = Circuit::new();
    let (i, d) = (c.node("in"), c.node("d"));
    c.add(
        "V1",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: dc(12.0),
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: i,
            b: d,
            ohms: 1000.0,
        },
    );
    c.add(
        "Z1",
        Element::Diode {
            anode: GROUND,
            cathode: d,
            model: DiodeModel {
                is: 1e-14,
                n: 1.0,
                bv: 5.1,
            },
        },
    );
    out.push(op("zener", c));

    let mut c = Circuit::new();
    let (vcc, base, coll) = (c.node("vcc"), c.node("base"), c.node("coll"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vcc,
            minus: GROUND,
            source: dc(12.0),
        },
    );
    c.add(
        "RB",
        Element::Resistor {
            a: vcc,
            b: base,
            ohms: 1e6,
        },
    );
    c.add(
        "RC",
        Element::Resistor {
            a: vcc,
            b: coll,
            ohms: 4700.0,
        },
    );
    c.add(
        "Q1",
        Element::Bjt {
            collector: coll,
            base,
            emitter: GROUND,
            pnp: false,
            model: NPN,
        },
    );
    out.push(op("bjt_npn", c));

    let mut c = Circuit::new();
    let (vcc, base, coll) = (c.node("vcc"), c.node("base"), c.node("coll"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vcc,
            minus: GROUND,
            source: dc(12.0),
        },
    );
    c.add(
        "RB",
        Element::Resistor {
            a: base,
            b: GROUND,
            ohms: 1e6,
        },
    );
    c.add(
        "RC",
        Element::Resistor {
            a: coll,
            b: GROUND,
            ohms: 4700.0,
        },
    );
    c.add(
        "Q1",
        Element::Bjt {
            collector: coll,
            base,
            emitter: vcc,
            pnp: true,
            model: NPN,
        },
    );
    out.push(op("bjt_pnp", c));

    let mut c = Circuit::new();
    let (vcc, base, coll) = (c.node("vcc"), c.node("base"), c.node("coll"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vcc,
            minus: GROUND,
            source: dc(5.0),
        },
    );
    c.add(
        "RB",
        Element::Resistor {
            a: vcc,
            b: base,
            ohms: 1000.0,
        },
    );
    c.add(
        "RC",
        Element::Resistor {
            a: vcc,
            b: coll,
            ohms: 100.0,
        },
    );
    c.add(
        "Q1",
        Element::Bjt {
            collector: coll,
            base,
            emitter: GROUND,
            pnp: false,
            model: NPN,
        },
    );
    out.push(op("bjt_saturated", c));

    let mut c = Circuit::new();
    let (vdd, gate, drain) = (c.node("vdd"), c.node("gate"), c.node("drain"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vdd,
            minus: GROUND,
            source: dc(12.0),
        },
    );
    c.add(
        "V2",
        Element::Voltage {
            plus: gate,
            minus: GROUND,
            source: dc(3.0),
        },
    );
    c.add(
        "RD",
        Element::Resistor {
            a: vdd,
            b: drain,
            ohms: 100.0,
        },
    );
    c.add(
        "M1",
        Element::Mos {
            drain,
            gate,
            source: GROUND,
            p_channel: false,
            model: NMOS,
        },
    );
    out.push(op("nmos_saturation", c));

    let mut c = Circuit::new();
    let (vdd, drain) = (c.node("vdd"), c.node("drain"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vdd,
            minus: GROUND,
            source: dc(12.0),
        },
    );
    c.add(
        "RD",
        Element::Resistor {
            a: vdd,
            b: drain,
            ohms: 1000.0,
        },
    );
    c.add(
        "M1",
        Element::Mos {
            drain,
            gate: vdd,
            source: GROUND,
            p_channel: false,
            model: NMOS,
        },
    );
    out.push(op("nmos_triode", c));

    let mut c = Circuit::new();
    let (vdd, drain) = (c.node("vdd"), c.node("drain"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vdd,
            minus: GROUND,
            source: dc(5.0),
        },
    );
    c.add(
        "RL",
        Element::Resistor {
            a: drain,
            b: GROUND,
            ohms: 1000.0,
        },
    );
    c.add(
        "M1",
        Element::Mos {
            drain,
            gate: GROUND,
            source: vdd,
            p_channel: true,
            model: NMOS,
        },
    );
    out.push(op("pmos_switch", c));

    let mut c = Circuit::new();
    let (i, inv, o) = (c.node("in"), c.node("inv"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: dc(0.5),
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: i,
            b: inv,
            ohms: 1000.0,
        },
    );
    c.add(
        "R2",
        Element::Resistor {
            a: inv,
            b: o,
            ohms: 10000.0,
        },
    );
    c.add(
        "U1",
        Element::Opamp {
            output: o,
            plus: GROUND,
            minus: inv,
            gain: 1e5,
        },
    );
    out.push(op("opamp_inverting", c));

    let mut c = Circuit::new();
    let (vcc, base, coll, i) = (c.node("vcc"), c.node("base"), c.node("coll"), c.node("in"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vcc,
            minus: GROUND,
            source: dc(12.0),
        },
    );
    c.add(
        "V2",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: dc(0.0),
        },
    );
    c.add(
        "RB",
        Element::Resistor {
            a: i,
            b: base,
            ohms: 10000.0,
        },
    );
    c.add(
        "RC",
        Element::Resistor {
            a: vcc,
            b: coll,
            ohms: 1000.0,
        },
    );
    c.add(
        "Q1",
        Element::Bjt {
            collector: coll,
            base,
            emitter: GROUND,
            pnp: false,
            model: NPN,
        },
    );
    out.push(Case {
        name: "bjt_dc_sweep",
        circuit: c,
        analysis: Analysis::DcSweep {
            source: "V2".to_owned(),
            start: 0.0,
            stop: 3.0,
            step: 0.05,
        },
        command: "dc V2 0 3 0.05".to_owned(),
    });

    let mut c = Circuit::new();
    let (i, o) = (c.node("in"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: Source {
                dc: 0.0,
                dc_given: false,
                ac: 1.0,
                wave: None,
            },
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: i,
            b: o,
            ohms: 1000.0,
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a: o,
            b: GROUND,
            farads: 1e-6,
        },
    );
    out.push(Case {
        name: "rc_ac",
        circuit: c,
        analysis: Analysis::Ac {
            start: 1.0,
            stop: 1e6,
            points_per_decade: 10,
        },
        command: "ac dec 10 1 1e6".to_owned(),
    });

    let mut c = Circuit::new();
    let (i, a, o) = (c.node("in"), c.node("a"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: Source {
                dc: 0.0,
                dc_given: false,
                ac: 1.0,
                wave: None,
            },
        },
    );
    c.add(
        "L1",
        Element::Inductor {
            a: i,
            b: a,
            henries: 1e-3,
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a,
            b: o,
            farads: 1e-6,
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: o,
            b: GROUND,
            ohms: 10.0,
        },
    );
    out.push(Case {
        name: "rlc_ac",
        circuit: c,
        analysis: Analysis::Ac {
            start: 100.0,
            stop: 100e3,
            points_per_decade: 50,
        },
        command: "ac dec 50 100 100e3".to_owned(),
    });

    let mut c = Circuit::new();
    let (vcc, base, coll, i) = (c.node("vcc"), c.node("base"), c.node("coll"), c.node("in"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vcc,
            minus: GROUND,
            source: dc(12.0),
        },
    );
    c.add(
        "V2",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: Source {
                dc: 0.0,
                dc_given: false,
                ac: 1e-3,
                wave: None,
            },
        },
    );
    c.add(
        "RB",
        Element::Resistor {
            a: vcc,
            b: base,
            ohms: 1e6,
        },
    );
    c.add(
        "RC",
        Element::Resistor {
            a: vcc,
            b: coll,
            ohms: 4700.0,
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a: i,
            b: base,
            farads: 1e-6,
        },
    );
    c.add(
        "Q1",
        Element::Bjt {
            collector: coll,
            base,
            emitter: GROUND,
            pnp: false,
            model: NPN,
        },
    );
    out.push(Case {
        name: "bjt_ac",
        circuit: c,
        analysis: Analysis::Ac {
            start: 1.0,
            stop: 1e5,
            points_per_decade: 10,
        },
        command: "ac dec 10 1 1e5".to_owned(),
    });

    let mut c = Circuit::new();
    let (i, o) = (c.node("in"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: pulse(0.0, 5.0, 5e-3, 10e-3),
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: i,
            b: o,
            ohms: 1000.0,
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a: o,
            b: GROUND,
            farads: 1e-6,
        },
    );
    out.push(Case {
        name: "rc_tran",
        circuit: c,
        analysis: Analysis::Transient {
            stop: 15e-3,
            step: 5e-6,
        },
        command: "tran 5u 15m 0 5u".to_owned(),
    });

    let mut c = Circuit::new();
    let (i, a, o) = (c.node("in"), c.node("a"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: pulse(0.0, 1.0, 10e-3, 20e-3),
        },
    );
    c.add(
        "L1",
        Element::Inductor {
            a: i,
            b: a,
            henries: 1e-3,
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a,
            b: o,
            farads: 1e-6,
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: o,
            b: GROUND,
            ohms: 10.0,
        },
    );
    out.push(Case {
        name: "rlc_tran",
        circuit: c,
        analysis: Analysis::Transient {
            stop: 2e-3,
            step: 1e-6,
        },
        command: "tran 1u 2m 0 1u".to_owned(),
    });

    let mut c = Circuit::new();
    let (i, o) = (c.node("in"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: Source {
                dc: 0.0,
                dc_given: false,
                ac: 0.0,
                wave: Some(Waveform::Sine {
                    offset: 0.0,
                    amplitude: 5.0,
                    frequency: 1000.0,
                    delay: 0.0,
                }),
            },
        },
    );
    c.add(
        "D1",
        Element::Diode {
            anode: i,
            cathode: o,
            model: DIODE,
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a: o,
            b: GROUND,
            farads: 10e-6,
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: o,
            b: GROUND,
            ohms: 10e3,
        },
    );
    out.push(Case {
        name: "rectifier_tran",
        circuit: c,
        analysis: Analysis::Transient {
            stop: 10e-3,
            step: 2e-6,
        },
        command: "tran 2u 10m 0 2u".to_owned(),
    });

    let mut c = Circuit::new();
    let (vdd, i, o) = (c.node("vdd"), c.node("in"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vdd,
            minus: GROUND,
            source: dc(5.0),
        },
    );
    c.add(
        "V2",
        Element::Voltage {
            plus: i,
            minus: GROUND,
            source: pulse(0.0, 5.0, 50e-6, 100e-6),
        },
    );
    c.add(
        "RD",
        Element::Resistor {
            a: vdd,
            b: o,
            ohms: 1000.0,
        },
    );
    c.add(
        "CL",
        Element::Capacitor {
            a: o,
            b: GROUND,
            farads: 10e-9,
        },
    );
    c.add(
        "M1",
        Element::Mos {
            drain: o,
            gate: i,
            source: GROUND,
            p_channel: false,
            model: NMOS,
        },
    );
    out.push(Case {
        name: "nmos_switch_tran",
        circuit: c,
        analysis: Analysis::Transient {
            stop: 200e-6,
            step: 0.02e-6,
        },
        command: "tran 0.02u 200u 0 0.02u".to_owned(),
    });
    let mut c = Circuit::new();
    let (a, b) = (c.node("a"), c.node("b"));
    let stated = parse_source("DC 10 AC 1 PULSE(0 10 0 1u 1u 1 2)").unwrap_or_default();
    let wave_only = parse_source("PULSE(3 10 0 1u 1u 1 2)").unwrap_or_default();
    c.add(
        "V1",
        Element::Voltage {
            plus: a,
            minus: GROUND,
            source: stated,
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a,
            b: GROUND,
            ohms: 1000.0,
        },
    );
    c.add(
        "V2",
        Element::Voltage {
            plus: b,
            minus: GROUND,
            source: wave_only,
        },
    );
    c.add(
        "R2",
        Element::Resistor {
            a: b,
            b: GROUND,
            ohms: 1000.0,
        },
    );
    out.push(Case {
        name: "dc_beside_wave_op",
        circuit: c.clone(),
        analysis: Analysis::OperatingPoint,
        command: "op".to_owned(),
    });
    out.push(Case {
        name: "dc_beside_wave_sweep",
        circuit: c.clone(),
        analysis: Analysis::DcSweep {
            source: "V1".to_owned(),
            start: 0.0,
            stop: 5.0,
            step: 1.0,
        },
        command: "dc V1 0 5 1".to_owned(),
    });
    out.push(Case {
        name: "dc_beside_wave_tran",
        circuit: c,
        analysis: Analysis::Transient {
            stop: 1e-4,
            step: 1e-6,
        },
        command: "tran 1u 100u 0 1u".to_owned(),
    });
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let directory = args.next().unwrap_or_else(|| ".".to_owned());
    std::fs::create_dir_all(&directory).unwrap_or_else(|e| panic!("{e}"));
    for case in cases() {
        let result = simulate(&case.circuit, &case.analysis)
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        let deck = case.circuit.to_spice(case.name);
        let vectors: Vec<String> = (1..case.circuit.nodes.len())
            .map(|i| format!("n{i}"))
            .collect();
        let body = serde_json::json!({
            "name": case.name,
            "command": case.command,
            "kind": result.x_unit,
            "nodes": case.circuit.nodes[1..],
            "vectors": vectors,
            "x": result.x,
            "traces": result.traces,
            "deck": deck,
        });
        std::fs::write(format!("{directory}/{}.json", case.name), body.to_string())
            .unwrap_or_else(|e| panic!("{e}"));
    }
}
