//! Reference circuits with results calculated by hand or in closed form.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use tronic_library::builtin;
use tronic_schematic::{EditCommand, Editor, ElementId, Point};
use tronic_sim::*;

fn trace<'a>(result: &'a SimResult, name: &str) -> &'a [f64] {
    &result
        .traces
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("no trace {name}"))
        .values
}

fn assert_near(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} is not within {tolerance} of {expected}"
    );
}

fn op(circuit: &Circuit) -> SimResult {
    simulate(circuit, &Analysis::OperatingPoint).unwrap()
}

/// `V1` on node `in`.
fn with_source(volts: f64) -> (Circuit, usize) {
    let mut c = Circuit::new();
    let input = c.node("in");
    c.add(
        "V1",
        Element::Voltage {
            plus: input,
            minus: GROUND,
            source: Source::dc(volts),
        },
    );
    (c, input)
}

#[test]
fn voltage_divider() {
    let (mut c, input) = with_source(10.0);
    let out = c.node("out");
    c.add(
        "R1",
        Element::Resistor {
            a: input,
            b: out,
            ohms: 1000.0,
        },
    );
    c.add(
        "R2",
        Element::Resistor {
            a: out,
            b: GROUND,
            ohms: 3000.0,
        },
    );
    let r = op(&c);
    assert_near(trace(&r, "V(out)")[0], 7.5, 1e-6, "divider");
    assert_near(trace(&r, "I(R1)")[0], 2.5e-3, 1e-9, "current");
    // SPICE sign: current through the source from plus to minus.
    assert_near(trace(&r, "I(V1)")[0], -2.5e-3, 1e-9, "source current");
}

#[test]
fn current_source_into_resistor() {
    let mut c = Circuit::new();
    let n = c.node("n");
    c.add(
        "I1",
        Element::Current {
            plus: GROUND,
            minus: n,
            source: Source::dc(2e-3),
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: n,
            b: GROUND,
            ohms: 500.0,
        },
    );
    assert_near(trace(&op(&c), "V(n)")[0], 1.0, 1e-6, "I times R");
}

#[test]
fn superposition_of_two_sources() {
    let mut c = Circuit::new();
    let (a, b, m) = (c.node("a"), c.node("b"), c.node("m"));
    c.add(
        "V1",
        Element::Voltage {
            plus: a,
            minus: GROUND,
            source: Source::dc(12.0),
        },
    );
    c.add(
        "V2",
        Element::Voltage {
            plus: b,
            minus: GROUND,
            source: Source::dc(6.0),
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
    // Millman: (12/2k + 6/1k) / (1/2k + 1/1k + 1/4k)
    let expected = (12.0 / 2000.0 + 6.0 / 1000.0) / (1.0 / 2000.0 + 1.0 / 1000.0 + 1.0 / 4000.0);
    assert_near(trace(&op(&c), "V(m)")[0], expected, 1e-6, "Millman");
}

#[test]
fn inductor_is_a_short_and_capacitor_open_at_dc() {
    let (mut c, input) = with_source(5.0);
    let (a, b) = (c.node("a"), c.node("b"));
    c.add(
        "L1",
        Element::Inductor {
            a: input,
            b: a,
            henries: 1e-3,
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a,
            b: GROUND,
            ohms: 100.0,
        },
    );
    c.add("R2", Element::Resistor { a, b, ohms: 100.0 });
    c.add(
        "C1",
        Element::Capacitor {
            a: b,
            b: GROUND,
            farads: 1e-6,
        },
    );
    let r = op(&c);
    assert_near(trace(&r, "V(a)")[0], 5.0, 1e-6, "short");
    assert_near(trace(&r, "I(L1)")[0], 0.05, 1e-6, "inductor current");
    assert_near(
        trace(&r, "V(b)")[0],
        5.0,
        1e-3,
        "no current into the capacitor",
    );
}

#[test]
fn diode_follows_the_shockley_equation() {
    let model = DiodeModel {
        is: 1e-14,
        n: 1.0,
        bv: f64::INFINITY,
    };
    let (mut c, input) = with_source(5.0);
    let d = c.node("d");
    c.add(
        "R1",
        Element::Resistor {
            a: input,
            b: d,
            ohms: 1000.0,
        },
    );
    c.add(
        "D1",
        Element::Diode {
            anode: d,
            cathode: GROUND,
            model,
        },
    );
    let r = op(&c);
    let (v, i) = (trace(&r, "V(d)")[0], trace(&r, "I(D1)")[0]);
    assert_near(i, (5.0 - v) / 1000.0, 1e-9, "KCL");
    let expected = 1e-14 * ((v / 0.0258651f64).exp() - 1.0);
    assert_near(i, expected, i * 1e-3, "Shockley");
    assert!(v > 0.6 && v < 0.75, "{v}");
}

#[test]
fn reverse_diode_blocks_and_zener_breaks_down() {
    let (mut c, input) = with_source(-5.0);
    let d = c.node("d");
    c.add(
        "R1",
        Element::Resistor {
            a: input,
            b: d,
            ohms: 1000.0,
        },
    );
    c.add(
        "D1",
        Element::Diode {
            anode: d,
            cathode: GROUND,
            model: DiodeModel {
                is: 1e-14,
                n: 1.0,
                bv: f64::INFINITY,
            },
        },
    );
    assert_near(trace(&op(&c), "V(d)")[0], -5.0, 1e-6, "blocks");

    let (mut c, input) = with_source(12.0);
    let d = c.node("d");
    c.add(
        "R1",
        Element::Resistor {
            a: input,
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
    let v = trace(&op(&c), "V(d)")[0];
    assert!(v > 5.1 && v < 5.8, "zener voltage {v}");
}

#[test]
fn common_emitter_stage_matches_hand_calculation() {
    let model = BjtModel {
        is: 1e-14,
        bf: 100.0,
        br: 1.0,
    };
    let mut c = Circuit::new();
    let (vcc, base, coll) = (c.node("vcc"), c.node("base"), c.node("coll"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vcc,
            minus: GROUND,
            source: Source::dc(12.0),
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
            model,
        },
    );
    let r = op(&c);
    let vb = trace(&r, "V(base)")[0];
    let vc = trace(&r, "V(coll)")[0];
    let ib = (12.0 - vb) / 1e6;
    let ic = (12.0 - vc) / 4700.0;
    assert!(vb > 0.55 && vb < 0.75, "{vb}");
    assert_near(ic / ib, 100.0, 1.0, "current gain in the active region");
}

#[test]
fn pnp_mirrors_npn() {
    let model = BjtModel {
        is: 1e-14,
        bf: 100.0,
        br: 1.0,
    };
    let mut c = Circuit::new();
    let (vcc, base, coll) = (c.node("vcc"), c.node("base"), c.node("coll"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vcc,
            minus: GROUND,
            source: Source::dc(12.0),
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
            model,
        },
    );
    let r = op(&c);
    let vb = trace(&r, "V(base)")[0];
    let vc = trace(&r, "V(coll)")[0];
    assert_near((vc / 4700.0) / (vb / 1e6), 100.0, 1.0, "gain");
    assert!(12.0 - vb > 0.55 && 12.0 - vb < 0.75, "{vb}");
}

#[test]
fn mosfet_saturation_and_triode_match_the_formula() {
    let model = MosModel {
        kp: 0.1,
        vto: 2.0,
        lambda: 0.0,
    };
    // Saturation: Id = Kp/2 (Vgs - Vt)^2 = 0.05 * 1 = 50 mA with Vgs = 3.
    let mut c = Circuit::new();
    let (vdd, gate, drain) = (c.node("vdd"), c.node("gate"), c.node("drain"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vdd,
            minus: GROUND,
            source: Source::dc(12.0),
        },
    );
    c.add(
        "V2",
        Element::Voltage {
            plus: gate,
            minus: GROUND,
            source: Source::dc(3.0),
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
            model,
        },
    );
    assert_near(trace(&op(&c), "I(RD)")[0], 0.05, 1e-6, "saturation");

    // Triode: solve Id = Kp ((Vgs-Vt) Vds - Vds^2/2), Vds = 12 - 1000 Id, Vgs = 12.
    let mut c = Circuit::new();
    let (vdd, drain) = (c.node("vdd"), c.node("drain"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vdd,
            minus: GROUND,
            source: Source::dc(12.0),
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
            model,
        },
    );
    let vds = trace(&op(&c), "V(drain)")[0];
    let id = (12.0 - vds) / 1000.0;
    assert_near(id, 0.1 * (10.0 * vds - vds * vds / 2.0), 1e-6, "triode");

    // Cut off below the threshold.
    let mut c = Circuit::new();
    let (vdd, gate, drain) = (c.node("vdd"), c.node("gate"), c.node("drain"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vdd,
            minus: GROUND,
            source: Source::dc(12.0),
        },
    );
    c.add(
        "V2",
        Element::Voltage {
            plus: gate,
            minus: GROUND,
            source: Source::dc(1.0),
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
            model,
        },
    );
    assert_near(trace(&op(&c), "V(drain)")[0], 12.0, 1e-3, "cut off");
}

#[test]
fn p_channel_mosfet_works_as_high_side_switch() {
    let model = MosModel {
        kp: 0.1,
        vto: 2.0,
        lambda: 0.0,
    };
    let mut c = Circuit::new();
    let (vdd, drain) = (c.node("vdd"), c.node("drain"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vdd,
            minus: GROUND,
            source: Source::dc(5.0),
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
            model,
        },
    );
    assert!(trace(&op(&c), "V(drain)")[0] > 4.9);
}

#[test]
fn inverting_amplifier_has_gain_minus_r2_over_r1() {
    let mut c = Circuit::new();
    let (input, inv, out) = (c.node("in"), c.node("inv"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: input,
            minus: GROUND,
            source: Source::dc(0.5),
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: input,
            b: inv,
            ohms: 1000.0,
        },
    );
    c.add(
        "R2",
        Element::Resistor {
            a: inv,
            b: out,
            ohms: 10000.0,
        },
    );
    c.add(
        "U1",
        Element::Opamp {
            output: out,
            plus: GROUND,
            minus: inv,
            gain: 1e5,
        },
    );
    assert_near(trace(&op(&c), "V(out)")[0], -5.0, 1e-3, "gain");
}

#[test]
fn dc_sweep_of_a_divider_is_a_straight_line() {
    let (mut c, input) = with_source(0.0);
    let out = c.node("out");
    c.add(
        "R1",
        Element::Resistor {
            a: input,
            b: out,
            ohms: 1000.0,
        },
    );
    c.add(
        "R2",
        Element::Resistor {
            a: out,
            b: GROUND,
            ohms: 1000.0,
        },
    );
    let r = simulate(
        &c,
        &Analysis::DcSweep {
            source: "V1".to_owned(),
            start: 0.0,
            stop: 10.0,
            step: 1.0,
        },
    )
    .unwrap();
    assert_eq!(r.x.len(), 11);
    for (x, v) in r.x.iter().zip(trace(&r, "V(out)")) {
        assert_near(*v, x / 2.0, 1e-6, "line");
    }
}

#[test]
fn dc_sweep_shows_the_diode_knee() {
    let (mut c, input) = with_source(0.0);
    let d = c.node("d");
    c.add(
        "R1",
        Element::Resistor {
            a: input,
            b: d,
            ohms: 100.0,
        },
    );
    c.add(
        "D1",
        Element::Diode {
            anode: d,
            cathode: GROUND,
            model: DiodeModel {
                is: 1e-14,
                n: 1.0,
                bv: f64::INFINITY,
            },
        },
    );
    let r = simulate(
        &c,
        &Analysis::DcSweep {
            source: "V1".to_owned(),
            start: 0.0,
            stop: 5.0,
            step: 0.25,
        },
    )
    .unwrap();
    let v = trace(&r, "V(d)");
    assert!(v[1] > 0.24, "follows below the knee: {}", v[1]);
    assert!(v[20] < 0.85, "clamps above the knee: {}", v[20]);
}

#[test]
fn rc_low_pass_has_its_corner_at_one_over_two_pi_rc() {
    let (r_value, c_value) = (1000.0, 1e-6);
    let corner = 1.0 / (std::f64::consts::TAU * r_value * c_value);
    let mut c = Circuit::new();
    let (input, out) = (c.node("in"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: input,
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
            a: input,
            b: out,
            ohms: r_value,
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a: out,
            b: GROUND,
            farads: c_value,
        },
    );
    let r = simulate(
        &c,
        &Analysis::Ac {
            start: 1.0,
            stop: 1e6,
            points_per_decade: 20,
        },
    )
    .unwrap();
    for (index, f) in r.x.iter().enumerate() {
        let expected = 1.0 / (1.0 + (f / corner).powi(2)).sqrt();
        assert_near(trace(&r, "V(out)")[index], expected, 1e-5, "magnitude");
        let phase = -(f / corner).atan().to_degrees();
        assert_near(trace(&r, "P(out)")[index], phase, 1e-3, "phase");
    }
}

#[test]
fn series_rlc_resonates_at_its_frequency() {
    let (l, cap, res) = (1e-3, 1e-6, 10.0);
    let f0 = 1.0 / (std::f64::consts::TAU * f64::sqrt(l * cap));
    let mut c = Circuit::new();
    let (input, a, out) = (c.node("in"), c.node("a"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: input,
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
            a: input,
            b: a,
            henries: l,
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a,
            b: out,
            farads: cap,
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: out,
            b: GROUND,
            ohms: res,
        },
    );
    let r = simulate(
        &c,
        &Analysis::Ac {
            start: 100.0,
            stop: 100e3,
            points_per_decade: 200,
        },
    )
    .unwrap();
    let v = trace(&r, "V(out)");
    let peak = (0..v.len()).max_by(|a, b| v[*a].total_cmp(&v[*b])).unwrap();
    assert_near(r.x[peak], f0, f0 * 0.012, "resonance");
    assert_near(v[peak], 1.0, 1e-2, "all voltage across R at resonance");
}

#[test]
fn common_emitter_ac_gain_matches_small_signal_theory() {
    let model = BjtModel {
        is: 1e-14,
        bf: 100.0,
        br: 1.0,
    };
    let mut c = Circuit::new();
    let (vcc, base, coll, input) = (c.node("vcc"), c.node("base"), c.node("coll"), c.node("in"));
    c.add(
        "V1",
        Element::Voltage {
            plus: vcc,
            minus: GROUND,
            source: Source::dc(12.0),
        },
    );
    c.add(
        "V2",
        Element::Voltage {
            plus: input,
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
            a: input,
            b: base,
            farads: 1e-3,
        },
    );
    c.add(
        "Q1",
        Element::Bjt {
            collector: coll,
            base,
            emitter: GROUND,
            pnp: false,
            model,
        },
    );
    let dc = op(&c);
    let ic = trace(&dc, "I(RC)")[0];
    let gm = ic / 0.0258651;
    let r = simulate(
        &c,
        &Analysis::Ac {
            start: 1000.0,
            stop: 2000.0,
            points_per_decade: 1,
        },
    )
    .unwrap();
    assert_near(
        trace(&r, "V(coll)")[0],
        gm * 4700.0 * 1e-3,
        0.02 * gm * 4.7,
        "gain",
    );
}

#[test]
fn rc_charge_follows_the_exponential() {
    let (res, cap) = (1000.0, 1e-6);
    let tau = res * cap;
    let mut c = Circuit::new();
    let (input, out) = (c.node("in"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: input,
            minus: GROUND,
            source: Source {
                dc: 0.0,
                dc_given: false,
                ac: 0.0,
                wave: Some(Waveform::Pulse {
                    v1: 0.0,
                    v2: 5.0,
                    delay: 0.0,
                    rise: 1e-9,
                    fall: 1e-9,
                    width: 1.0,
                    period: 2.0,
                }),
            },
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: input,
            b: out,
            ohms: res,
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a: out,
            b: GROUND,
            farads: cap,
        },
    );
    let r = simulate(
        &c,
        &Analysis::Transient {
            stop: 5.0 * tau,
            step: tau / 200.0,
        },
    )
    .unwrap();
    let v = trace(&r, "V(out)");
    for (index, t) in r.x.iter().enumerate().skip(10) {
        assert_near(v[index], 5.0 * (1.0 - (-t / tau).exp()), 0.03, "charge");
    }
    assert_near(
        *v.last().unwrap(),
        5.0 * (1.0 - (-5f64).exp()),
        0.005,
        "end value",
    );
}

#[test]
fn rl_current_rises_with_its_time_constant() {
    let (res, l) = (100.0, 10e-3);
    let tau = l / res;
    let mut c = Circuit::new();
    let (input, a) = (c.node("in"), c.node("a"));
    c.add(
        "V1",
        Element::Voltage {
            plus: input,
            minus: GROUND,
            source: Source {
                dc: 0.0,
                dc_given: false,
                ac: 0.0,
                wave: Some(Waveform::Pulse {
                    v1: 0.0,
                    v2: 1.0,
                    delay: 0.0,
                    rise: 1e-9,
                    fall: 1e-9,
                    width: 1.0,
                    period: 2.0,
                }),
            },
        },
    );
    c.add(
        "L1",
        Element::Inductor {
            a: input,
            b: a,
            henries: l,
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a,
            b: GROUND,
            ohms: res,
        },
    );
    let r = simulate(
        &c,
        &Analysis::Transient {
            stop: 5.0 * tau,
            step: tau / 200.0,
        },
    )
    .unwrap();
    let i = trace(&r, "I(L1)");
    for (index, t) in r.x.iter().enumerate().skip(10) {
        assert_near(i[index], 0.01 * (1.0 - (-t / tau).exp()), 2e-4, "rise");
    }
}

#[test]
fn lc_oscillation_keeps_period_and_energy() {
    let (l, cap) = (1e-3, 1e-6);
    let period = std::f64::consts::TAU * f64::sqrt(l * cap);
    let mut c = Circuit::new();
    let (input, a) = (c.node("in"), c.node("a"));
    c.add(
        "V1",
        Element::Voltage {
            plus: input,
            minus: GROUND,
            source: Source {
                dc: 0.0,
                dc_given: false,
                ac: 0.0,
                wave: Some(Waveform::Pulse {
                    v1: 0.0,
                    v2: 1.0,
                    delay: 0.0,
                    rise: 1e-9,
                    fall: 1e-9,
                    width: 10.0,
                    period: 20.0,
                }),
            },
        },
    );
    c.add(
        "L1",
        Element::Inductor {
            a: input,
            b: a,
            henries: l,
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a,
            b: GROUND,
            farads: cap,
        },
    );
    let r = simulate(
        &c,
        &Analysis::Transient {
            stop: 5.0 * period,
            step: period / 400.0,
        },
    )
    .unwrap();
    let v = trace(&r, "V(a)");
    // Undamped step response: 1 - cos(wt), so swings between 0 and 2.
    let max = v.iter().copied().fold(0.0, f64::max);
    assert_near(max, 2.0, 0.05, "amplitude");
    let half = r.x.iter().position(|t| *t >= period / 2.0).unwrap();
    assert_near(v[half], 2.0, 0.02, "half period");
}

#[test]
fn half_wave_rectifier_charges_to_the_peak_minus_a_diode() {
    let mut c = Circuit::new();
    let (input, out) = (c.node("in"), c.node("out"));
    c.add(
        "V1",
        Element::Voltage {
            plus: input,
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
            anode: input,
            cathode: out,
            model: DiodeModel {
                is: 1e-14,
                n: 1.0,
                bv: f64::INFINITY,
            },
        },
    );
    c.add(
        "C1",
        Element::Capacitor {
            a: out,
            b: GROUND,
            farads: 10e-6,
        },
    );
    c.add(
        "R1",
        Element::Resistor {
            a: out,
            b: GROUND,
            ohms: 100e3,
        },
    );
    let r = simulate(
        &c,
        &Analysis::Transient {
            stop: 10e-3,
            step: 5e-6,
        },
    )
    .unwrap();
    let v = *trace(&r, "V(out)").last().unwrap();
    assert!(v > 4.1 && v < 4.5, "{v}");
}

#[test]
fn bad_circuits_and_settings_are_errors_not_nonsense() {
    assert_eq!(
        simulate(&Circuit::new(), &Analysis::OperatingPoint),
        Err(SimError::Empty)
    );

    let mut floating = Circuit::new();
    let (a, b) = (floating.node("a"), floating.node("b"));
    floating.add("R1", Element::Resistor { a, b, ohms: 1.0 });
    assert_eq!(
        simulate(&floating, &Analysis::OperatingPoint),
        Err(SimError::NoGround)
    );

    let (c, _) = with_source(1.0);
    for bad in [
        Analysis::Transient {
            stop: 0.0,
            step: 1.0,
        },
        Analysis::Transient {
            stop: 1.0,
            step: 1e-12,
        },
        Analysis::Ac {
            start: 0.0,
            stop: 1.0,
            points_per_decade: 1,
        },
        Analysis::DcSweep {
            source: "V1".to_owned(),
            start: 0.0,
            stop: 1.0,
            step: 0.0,
        },
        Analysis::DcSweep {
            source: "V1".to_owned(),
            start: 0.0,
            stop: 1.0,
            step: -1.0,
        },
    ] {
        assert_eq!(
            simulate(&c, &bad).unwrap_err(),
            SimError::InvalidSettings,
            "{bad:?}"
        );
    }
    assert!(matches!(
        simulate(
            &c,
            &Analysis::DcSweep {
                source: "X".to_owned(),
                start: 0.0,
                stop: 1.0,
                step: 1.0
            }
        ),
        Err(SimError::UnknownSource(_))
    ));

    // Two ideal sources in parallel with different values cannot be solved.
    let mut conflict = Circuit::new();
    let n = conflict.node("n");
    conflict.add(
        "V1",
        Element::Voltage {
            plus: n,
            minus: GROUND,
            source: Source::dc(1.0),
        },
    );
    conflict.add(
        "V2",
        Element::Voltage {
            plus: n,
            minus: GROUND,
            source: Source::dc(2.0),
        },
    );
    assert_eq!(
        simulate(&conflict, &Analysis::OperatingPoint),
        Err(SimError::Singular)
    );
}

mod from_schematic {
    use super::*;

    const SHEET: ElementId = 1;

    struct Bench(Editor);

    impl Bench {
        fn new() -> Self {
            let mut editor = Editor::default();
            editor.add_symbols(builtin());
            Self(editor)
        }

        fn place(&mut self, symbol: &str, x: i64, y: i64, value: &str) -> ElementId {
            let id = self
                .0
                .apply(EditCommand::PlaceComponent {
                    sheet: SHEET,
                    symbol: symbol.to_owned(),
                    at: Point::grid(x, y),
                    rotation: Default::default(),
                    mirror: false,
                })
                .unwrap()
                .created[0];
            let component = self.0.doc().sheets[0]
                .components
                .iter()
                .find(|c| c.id == id)
                .unwrap()
                .clone();
            if !value.is_empty() {
                self.0
                    .apply(EditCommand::SetComponent {
                        sheet: SHEET,
                        id,
                        reference: component.reference,
                        value: value.to_owned(),
                    })
                    .unwrap();
            }
            id
        }

        fn wire(&mut self, a: (i64, i64), b: (i64, i64)) {
            self.0
                .apply(EditCommand::AddWires {
                    sheet: SHEET,
                    points: vec![Point::grid(a.0, a.1), Point::grid(b.0, b.1)],
                })
                .unwrap();
        }

        fn build(&self) -> (Circuit, BuildReport) {
            circuit_from_schematic(self.0.doc(), &BTreeMap::new())
        }
    }

    /// +5V - R1 - R2 - GND, with both resistors in a column.
    fn divider() -> Bench {
        let mut b = Bench::new();
        b.place("supply:5v", 0, -8, "");
        b.wire((0, -8), (0, -4));
        b.place("device:resistor", 0, 0, "1k");
        b.place("device:resistor", 0, 8, "3k");
        b.place("supply:gnd", 0, 12, "");
        b
    }

    #[test]
    fn a_divider_from_the_schematic_gives_the_right_voltage() {
        let b = divider();
        let (circuit, report) = b.build();
        assert!(report.is_complete(), "{report:?}");
        assert_eq!(report.supplies, [("+5V".to_owned(), 5.0)]);
        let result = op(&circuit);
        let middle = result
            .traces
            .iter()
            .find(|t| t.name.starts_with("V(") && t.name != "V(+5V)")
            .unwrap();
        assert_near(middle.values[0], 3.75, 1e-6, "divider");
    }

    #[test]
    fn missing_values_and_models_are_reported_not_hidden() {
        let mut b = divider();
        b.place("device:resistor", 20, 0, "abc");
        b.place("timer:ne555", 40, 0, "");
        b.place("connector:1x02", 60, 0, "");
        let (_, report) = b.build();
        assert!(!report.is_complete());
        assert_eq!(report.bad_values, [("R3".to_owned(), "abc".to_owned())]);
        assert_eq!(report.unsupported, ["U1"]);
        assert_eq!(report.ignored, ["J1"]);
    }

    #[test]
    fn a_supply_without_a_stated_voltage_is_asked_for() {
        let mut b = Bench::new();
        b.place("supply:vcc", 0, -4, "");
        b.place("device:resistor", 0, 0, "1k");
        b.place("supply:gnd", 0, 4, "");
        let (_, report) = b.build();
        assert_eq!(report.unknown_supplies, ["VCC"]);

        let supplies = BTreeMap::from([("VCC".to_owned(), 3.3)]);
        let (circuit, report) = circuit_from_schematic(b.0.doc(), &supplies);
        assert!(report.is_complete());
        let result = op(&circuit);
        assert_near(
            trace(&result, "I(V_VCC)")[0],
            -3.3e-3,
            1e-9,
            "uses the given voltage",
        );
    }

    #[test]
    fn a_schematic_without_ground_is_reported() {
        let mut b = Bench::new();
        b.place("device:resistor", 0, 0, "1k");
        assert!(!b.build().1.has_ground);
    }

    #[test]
    fn every_part_with_a_simulation_model_builds() {
        let mut b = Bench::new();
        for (index, (symbol, value)) in [
            ("device:resistor", "1k"),
            ("device:potentiometer", "10k"),
            ("device:capacitor", "1u"),
            ("device:capacitor_polarized", "10u"),
            ("device:inductor", "1m"),
            ("device:diode", ""),
            ("device:diode_zener", "5V1"),
            ("device:diode_schottky", ""),
            ("device:led", ""),
            ("device:npn", ""),
            ("device:pnp", ""),
            ("device:nmos", ""),
            ("device:pmos", ""),
            ("analog:opamp", ""),
            ("sim:vsource", "SIN(0 1 1k)"),
            ("sim:isource", "1m"),
        ]
        .iter()
        .enumerate()
        {
            b.place(symbol, index as i64 * 30, 0, value);
        }
        b.place("supply:gnd", 0, 40, "");
        let (circuit, report) = b.build();
        assert!(report.unsupported.is_empty(), "{report:?}");
        assert!(report.bad_values.is_empty(), "{report:?}");
        assert_eq!(report.models.len(), 16);
        assert!(circuit.devices.len() >= 16);
    }
}

/// As in SPICE: DC analyses use the stated DC value, a transient starts from the
/// waveform. Checked against ngspice as `dc_beside_wave` in `verify_sim.py`.
#[test]
fn a_stated_dc_value_applies_to_the_operating_point_but_not_to_a_transient() {
    let mut c = Circuit::new();
    let a = c.node("a");
    let wave = parse_source("DC 10 PULSE(0 10 1 1u 1u 1 2)").unwrap();
    c.add(
        "V1",
        Element::Voltage {
            plus: a,
            minus: GROUND,
            source: wave,
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
    let op = simulate(&c, &Analysis::OperatingPoint).unwrap();
    let voltage = |r: &SimResult| r.traces.iter().find(|t| t.name == "V(a)").unwrap().values[0];
    assert_eq!(voltage(&op), 10.0);
    let tran = simulate(
        &c,
        &Analysis::Transient {
            stop: 1e-3,
            step: 1e-4,
        },
    )
    .unwrap();
    assert_eq!(voltage(&tran), 0.0);
}

#[test]
fn without_a_stated_dc_value_the_waveform_at_time_zero_applies() {
    let mut c = Circuit::new();
    let a = c.node("a");
    let wave = parse_source("PULSE(3 10 1 1u 1u 1 2)").unwrap();
    c.add(
        "V1",
        Element::Voltage {
            plus: a,
            minus: GROUND,
            source: wave,
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
    let op = simulate(&c, &Analysis::OperatingPoint).unwrap();
    assert_eq!(
        op.traces.iter().find(|t| t.name == "V(a)").unwrap().values[0],
        3.0
    );
}
