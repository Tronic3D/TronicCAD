//! The analyses: operating point, DC sweep, AC sweep and transient.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::circuit::*;

/// kT/q at 27 °C (300.15 K), as in SPICE.
const VT: f64 = 0.025_865_1;
const GMIN: f64 = 1e-12;
const RELTOL: f64 = 1e-6;
const VNTOL: f64 = 1e-7;
const MAX_ITERATIONS: usize = 200;
const MAX_UNKNOWNS: usize = 2000;
const MAX_POINTS: usize = 20_000;
const MAX_STEPS: u64 = 2_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize)]
pub enum SimError {
    #[error("the circuit has no connection to ground")]
    NoGround,
    #[error("the circuit has no devices")]
    Empty,
    #[error("the circuit has too many nodes")]
    TooLarge,
    #[error("no convergence")]
    NoConvergence,
    #[error("the circuit equations are singular")]
    Singular,
    #[error("the analysis settings are invalid")]
    InvalidSettings,
    #[error("unknown source {0}")]
    UnknownSource(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Analysis {
    OperatingPoint,
    DcSweep {
        source: String,
        start: f64,
        stop: f64,
        step: f64,
    },
    Ac {
        start: f64,
        stop: f64,
        points_per_decade: u32,
    },
    Transient {
        stop: f64,
        step: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trace {
    pub name: String,
    pub unit: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SimResult {
    /// What the x values mean: `point`, `V`, `Hz` or `s`.
    pub x_unit: String,
    pub x: Vec<f64>,
    pub traces: Vec<Trace>,
}

/// A linearised piece of a nonlinear device: current flowing into `row`.
struct Term {
    pos: usize,
    neg: usize,
    g: f64,
    v0: f64,
}

struct CurrentStamp {
    row: usize,
    i0: f64,
    terms: Vec<Term>,
}

#[derive(Clone, Copy)]
enum Mode {
    /// Sources scaled, for source stepping.
    Dc { scale: f64 },
    Tran {
        h: f64,
        trapezoidal: bool,
        time: f64,
    },
}

struct Layout {
    nodes: usize,
    branch: Vec<usize>,
    size: usize,
}

fn layout(circuit: &Circuit) -> Result<Layout, SimError> {
    let nodes = circuit.nodes.len() - 1;
    let mut next = nodes;
    let mut branch = vec![usize::MAX; circuit.devices.len()];
    for (index, device) in circuit.devices.iter().enumerate() {
        if matches!(
            device.element,
            Element::Voltage { .. } | Element::Inductor { .. } | Element::Opamp { .. }
        ) {
            branch[index] = next;
            next += 1;
        }
    }
    if next > MAX_UNKNOWNS {
        return Err(SimError::TooLarge);
    }
    Ok(Layout {
        nodes,
        branch,
        size: next,
    })
}

fn volt(x: &[f64], node: usize) -> f64 {
    if node == GROUND {
        0.0
    } else {
        x[node - 1]
    }
}

/// Dense matrix with right-hand side.
struct System {
    size: usize,
    a: Vec<f64>,
    b: Vec<f64>,
}

impl System {
    fn new(size: usize) -> Self {
        Self {
            size,
            a: vec![0.0; size * size],
            b: vec![0.0; size],
        }
    }

    fn clear(&mut self) {
        self.a.fill(0.0);
        self.b.fill(0.0);
    }

    fn add(&mut self, row: usize, column: usize, value: f64) {
        self.a[row * self.size + column] += value;
    }

    /// A conductance between two nodes.
    fn conductance(&mut self, p: usize, n: usize, g: f64) {
        if p != GROUND {
            self.add(p - 1, p - 1, g);
        }
        if n != GROUND {
            self.add(n - 1, n - 1, g);
        }
        if p != GROUND && n != GROUND {
            self.add(p - 1, n - 1, -g);
            self.add(n - 1, p - 1, -g);
        }
    }

    /// A constant current from p through the device to n.
    fn current(&mut self, p: usize, n: usize, i: f64) {
        if p != GROUND {
            self.b[p - 1] -= i;
        }
        if n != GROUND {
            self.b[n - 1] += i;
        }
    }

    fn stamp(&mut self, stamp: &CurrentStamp) {
        if stamp.row == GROUND {
            return;
        }
        let row = stamp.row - 1;
        let mut rhs = -stamp.i0;
        for term in &stamp.terms {
            rhs += term.g * term.v0;
            if term.pos != GROUND {
                self.add(row, term.pos - 1, term.g);
            }
            if term.neg != GROUND {
                self.add(row, term.neg - 1, -term.g);
            }
        }
        self.b[row] += rhs;
    }

    /// LU with partial pivoting. The result overwrites `b`.
    fn solve(&mut self) -> Result<(), SimError> {
        let n = self.size;
        for k in 0..n {
            let mut pivot = k;
            let mut largest = self.a[k * n + k].abs();
            for row in k + 1..n {
                let value = self.a[row * n + k].abs();
                if value > largest {
                    largest = value;
                    pivot = row;
                }
            }
            if largest < 1e-300 {
                return Err(SimError::Singular);
            }
            if pivot != k {
                for column in k..n {
                    self.a.swap(k * n + column, pivot * n + column);
                }
                self.b.swap(k, pivot);
            }
            let diagonal = self.a[k * n + k];
            for row in k + 1..n {
                let factor = self.a[row * n + k] / diagonal;
                if factor != 0.0 {
                    for column in k + 1..n {
                        self.a[row * n + column] -= factor * self.a[k * n + column];
                    }
                    self.b[row] -= factor * self.b[k];
                }
            }
        }
        for k in (0..n).rev() {
            let mut sum = self.b[k];
            for column in k + 1..n {
                sum -= self.a[k * n + column] * self.b[column];
            }
            self.b[k] = sum / self.a[k * n + k];
        }
        if self.b.iter().any(|v| !v.is_finite()) {
            return Err(SimError::Singular);
        }
        Ok(())
    }
}

/// SPICE's limit on junction voltage steps, which keeps exponentials in range.
fn limit_junction(new: f64, old: f64, nvt: f64, is: f64) -> f64 {
    let critical = nvt * (nvt / (std::f64::consts::SQRT_2 * is)).ln();
    if new > critical && (new - old).abs() > 2.0 * nvt {
        if old > 0.0 {
            let arg = 1.0 + (new - old) / nvt;
            if arg > 0.0 {
                old + nvt * arg.ln()
            } else {
                critical
            }
        } else {
            nvt * (new / nvt).ln()
        }
    } else {
        new
    }
}

/// Current at which the breakdown voltage is defined, as in SPICE (IBV).
const BREAKDOWN_CURRENT: f64 = 1e-3;

/// Where the reverse exponential sits, so that the stated breakdown voltage
/// carries the breakdown current.
fn breakdown_knee(model: &DiodeModel) -> f64 {
    model.bv - model.n * VT * (BREAKDOWN_CURRENT / model.is).ln()
}

fn diode_current(model: &DiodeModel, v: f64) -> (f64, f64) {
    let nvt = model.n * VT;
    let e = (v / nvt).min(500.0).exp();
    let mut i = model.is * (e - 1.0);
    let mut g = model.is * e / nvt;
    if model.bv.is_finite() {
        let reverse = ((-v - breakdown_knee(model)) / nvt).min(500.0).exp();
        i -= model.is * reverse;
        g += model.is * reverse / nvt;
    }
    (i + GMIN * v, g + GMIN)
}

/// The linearised currents of a nonlinear device at the present solution.
fn nonlinear_stamps(
    element: &Element,
    x: &[f64],
    state: &mut [f64; 2],
    limit: bool,
) -> Vec<CurrentStamp> {
    match *element {
        Element::Diode {
            anode,
            cathode,
            model,
        } => {
            let raw = volt(x, anode) - volt(x, cathode);
            let nvt = model.n * VT;
            let v = if limit {
                let forward = limit_junction(raw, state[0], nvt, model.is);
                let knee = breakdown_knee(&model);
                if model.bv.is_finite() && forward < -knee {
                    // Mirror the limit around the breakdown voltage.
                    -knee - limit_junction(-knee - forward, -knee - state[0], nvt, model.is)
                } else {
                    forward
                }
            } else {
                raw
            };
            state[0] = v;
            let (i, g) = diode_current(&model, v);
            let term = |g: f64| {
                vec![Term {
                    pos: anode,
                    neg: cathode,
                    g,
                    v0: v,
                }]
            };
            vec![
                CurrentStamp {
                    row: anode,
                    i0: i,
                    terms: term(g),
                },
                CurrentStamp {
                    row: cathode,
                    i0: -i,
                    terms: term(-g),
                },
            ]
        }
        Element::Bjt {
            collector,
            base,
            emitter,
            pnp,
            model,
        } => {
            let s = if pnp { -1.0 } else { 1.0 };
            let (raw_be, raw_bc) = (
                s * (volt(x, base) - volt(x, emitter)),
                s * (volt(x, base) - volt(x, collector)),
            );
            let (vbe, vbc) = if limit {
                (
                    limit_junction(raw_be, state[0], VT, model.is),
                    limit_junction(raw_bc, state[1], VT, model.is),
                )
            } else {
                (raw_be, raw_bc)
            };
            state[0] = vbe;
            state[1] = vbc;
            let e1 = (vbe / VT).min(500.0).exp();
            let e2 = (vbc / VT).min(500.0).exp();
            let transport = model.is * (e1 - e2);
            let ibe = model.is / model.bf * (e1 - 1.0) + GMIN * vbe;
            let ibc = model.is / model.br * (e2 - 1.0) + GMIN * vbc;
            let gpi = model.is / (model.bf * VT) * e1 + GMIN;
            let gmu = model.is / (model.br * VT) * e2 + GMIN;
            let gf = model.is / VT * e1;
            let gr = model.is / VT * e2;
            // Currents into collector and base as functions of vbe and vbc.
            let ic = s * (transport - ibc);
            let ib = s * (ibe + ibc);
            let (be_p, be_n) = if pnp {
                (emitter, base)
            } else {
                (base, emitter)
            };
            let (bc_p, bc_n) = if pnp {
                (collector, base)
            } else {
                (base, collector)
            };
            let terms = |dbe: f64, dbc: f64| {
                vec![
                    Term {
                        pos: be_p,
                        neg: be_n,
                        g: dbe,
                        v0: vbe,
                    },
                    Term {
                        pos: bc_p,
                        neg: bc_n,
                        g: dbc,
                        v0: vbc,
                    },
                ]
            };
            let (dic_be, dic_bc) = (s * gf, s * (-gr - gmu));
            let (dib_be, dib_bc) = (s * gpi, s * gmu);
            vec![
                CurrentStamp {
                    row: collector,
                    i0: ic,
                    terms: terms(dic_be, dic_bc),
                },
                CurrentStamp {
                    row: base,
                    i0: ib,
                    terms: terms(dib_be, dib_bc),
                },
                CurrentStamp {
                    row: emitter,
                    i0: -(ic + ib),
                    terms: terms(-(dic_be + dib_be), -(dic_bc + dib_bc)),
                },
            ]
        }
        Element::Mos {
            drain,
            gate,
            source,
            p_channel,
            model,
        } => {
            let s = if p_channel { -1.0 } else { 1.0 };
            let raw_gs = s * (volt(x, gate) - volt(x, source));
            let raw_ds = s * (volt(x, drain) - volt(x, source));
            let (vgs, vds) = if limit {
                (
                    state[0] + (raw_gs - state[0]).clamp(-2.0, 2.0),
                    state[1] + (raw_ds - state[1]).clamp(-5.0, 5.0),
                )
            } else {
                (raw_gs, raw_ds)
            };
            state[0] = vgs;
            state[1] = vds;
            let forward = vds >= 0.0;
            let (control, across) = if forward {
                (vgs, vds)
            } else {
                (vgs - vds, -vds)
            };
            let overdrive = control - model.vto;
            let (id, gm, gds) = if overdrive <= 0.0 {
                (0.0, 0.0, 0.0)
            } else if across < overdrive {
                let clm = 1.0 + model.lambda * across;
                (
                    model.kp * (overdrive - across / 2.0) * across * clm,
                    model.kp * across * clm,
                    model.kp * (overdrive - across) * clm
                        + model.kp * (overdrive - across / 2.0) * across * model.lambda,
                )
            } else {
                let clm = 1.0 + model.lambda * across;
                (
                    model.kp / 2.0 * overdrive * overdrive * clm,
                    model.kp * overdrive * clm,
                    model.kp / 2.0 * overdrive * overdrive * model.lambda,
                )
            };
            // Current into the drain, in terms of vgs and vds.
            let (i, dgs, dds) = if forward {
                (id, gm, gds)
            } else {
                (-id, -gm, gm + gds)
            };
            let i = s * (i + GMIN * vds);
            let dds = dds + GMIN;
            let (gs_p, gs_n) = if p_channel {
                (source, gate)
            } else {
                (gate, source)
            };
            let (ds_p, ds_n) = if p_channel {
                (source, drain)
            } else {
                (drain, source)
            };
            let terms = |dgs: f64, dds: f64| {
                vec![
                    Term {
                        pos: gs_p,
                        neg: gs_n,
                        g: dgs,
                        v0: vgs,
                    },
                    Term {
                        pos: ds_p,
                        neg: ds_n,
                        g: dds,
                        v0: vds,
                    },
                ]
            };
            vec![
                CurrentStamp {
                    row: drain,
                    i0: i,
                    terms: terms(s * dgs, s * dds),
                },
                CurrentStamp {
                    row: source,
                    i0: -i,
                    terms: terms(-s * dgs, -s * dds),
                },
            ]
        }
        _ => Vec::new(),
    }
}

/// Energy-storage history for the transient analysis.
#[derive(Clone)]
struct History {
    voltage: Vec<f64>,
    current: Vec<f64>,
}

#[allow(clippy::too_many_arguments)]
fn assemble(
    system: &mut System,
    circuit: &Circuit,
    layout: &Layout,
    x: &[f64],
    states: &mut [[f64; 2]],
    mode: Mode,
    history: Option<&History>,
    gmin: f64,
    limit: bool,
) {
    system.clear();
    for node in 1..=layout.nodes {
        system.conductance(node, GROUND, gmin);
    }
    for (index, device) in circuit.devices.iter().enumerate() {
        let branch = layout.branch[index];
        match &device.element {
            Element::Resistor { a, b, ohms } => system.conductance(*a, *b, 1.0 / ohms),
            Element::Capacitor { a, b, farads } => {
                if let (Mode::Tran { h, trapezoidal, .. }, Some(past)) = (mode, history) {
                    let (g, i_eq) = if trapezoidal {
                        let g = 2.0 * farads / h;
                        (g, -g * past.voltage[index] - past.current[index])
                    } else {
                        let g = farads / h;
                        (g, -g * past.voltage[index])
                    };
                    system.conductance(*a, *b, g);
                    system.current(*a, *b, i_eq);
                }
            }
            Element::Inductor { a, b, henries } => {
                if *a != GROUND {
                    system.add(a - 1, branch, 1.0);
                    system.add(branch, a - 1, 1.0);
                }
                if *b != GROUND {
                    system.add(b - 1, branch, -1.0);
                    system.add(branch, b - 1, -1.0);
                }
                if let (Mode::Tran { h, trapezoidal, .. }, Some(past)) = (mode, history) {
                    let (r, v_eq) = if trapezoidal {
                        let r = 2.0 * henries / h;
                        (r, -r * past.current[index] - past.voltage[index])
                    } else {
                        let r = henries / h;
                        (r, -r * past.current[index])
                    };
                    system.add(branch, branch, -r);
                    system.b[branch] += v_eq;
                }
            }
            Element::Voltage {
                plus,
                minus,
                source,
            } => {
                if *plus != GROUND {
                    system.add(plus - 1, branch, 1.0);
                    system.add(branch, plus - 1, 1.0);
                }
                if *minus != GROUND {
                    system.add(minus - 1, branch, -1.0);
                    system.add(branch, minus - 1, -1.0);
                }
                system.b[branch] += match mode {
                    Mode::Dc { scale } => source.at_dc() * scale,
                    Mode::Tran { time, .. } => source.at(time),
                };
            }
            Element::Current {
                plus,
                minus,
                source,
            } => {
                let value = match mode {
                    Mode::Dc { scale } => source.at_dc() * scale,
                    Mode::Tran { time, .. } => source.at(time),
                };
                system.current(*plus, *minus, value);
            }
            Element::Opamp {
                output,
                plus,
                minus,
                gain,
            } => {
                if *output != GROUND {
                    system.add(output - 1, branch, 1.0);
                    system.add(branch, output - 1, 1.0);
                }
                if *plus != GROUND {
                    system.add(branch, plus - 1, -gain);
                }
                if *minus != GROUND {
                    system.add(branch, minus - 1, *gain);
                }
            }
            element => {
                for stamp in nonlinear_stamps(element, x, &mut states[index], limit) {
                    system.stamp(&stamp);
                }
            }
        }
    }
}

/// Whether the voltages used for linearising agree with the solution.
fn devices_at_solution(circuit: &Circuit, x: &[f64], states: &[[f64; 2]]) -> bool {
    let close =
        |raw: f64, used: f64| (raw - used).abs() <= RELTOL * raw.abs().max(used.abs()) + VNTOL;
    circuit
        .devices
        .iter()
        .zip(states)
        .all(|(device, state)| match device.element {
            Element::Diode { anode, cathode, .. } => {
                close(volt(x, anode) - volt(x, cathode), state[0])
            }
            Element::Bjt {
                collector,
                base,
                emitter,
                pnp,
                ..
            } => {
                let s = if pnp { -1.0 } else { 1.0 };
                close(s * (volt(x, base) - volt(x, emitter)), state[0])
                    && close(s * (volt(x, base) - volt(x, collector)), state[1])
            }
            Element::Mos {
                drain,
                gate,
                source,
                p_channel,
                ..
            } => {
                let s = if p_channel { -1.0 } else { 1.0 };
                close(s * (volt(x, gate) - volt(x, source)), state[0])
                    && close(s * (volt(x, drain) - volt(x, source)), state[1])
            }
            _ => true,
        })
}

/// Solves the (possibly nonlinear) system from a starting point.
#[allow(clippy::too_many_arguments)]
fn newton(
    system: &mut System,
    circuit: &Circuit,
    layout: &Layout,
    x: &mut [f64],
    states: &mut [[f64; 2]],
    mode: Mode,
    history: Option<&History>,
    gmin: f64,
) -> Result<(), SimError> {
    let nonlinear = circuit.devices.iter().any(|d| {
        matches!(
            d.element,
            Element::Diode { .. } | Element::Bjt { .. } | Element::Mos { .. }
        )
    });
    for _ in 0..MAX_ITERATIONS {
        assemble(
            system, circuit, layout, x, states, mode, history, gmin, true,
        );
        system.solve()?;
        let converged = x
            .iter()
            .zip(&system.b)
            .all(|(old, new)| (new - old).abs() <= RELTOL * new.abs().max(old.abs()) + VNTOL);
        x.copy_from_slice(&system.b);
        // A solution only counts if the devices were evaluated at it, and not
        // at a voltage still held back by the step limit.
        if !nonlinear || (converged && devices_at_solution(circuit, x, states)) {
            return Ok(());
        }
    }
    Err(SimError::NoConvergence)
}

/// The operating point, with gmin and source stepping as fallbacks.
fn operating_point(
    system: &mut System,
    circuit: &Circuit,
    layout: &Layout,
    start: Option<&[f64]>,
) -> Result<(Vec<f64>, Vec<[f64; 2]>), SimError> {
    let mut x = start.map_or_else(|| vec![0.0; layout.size], <[f64]>::to_vec);
    let mut states = vec![[0.0; 2]; circuit.devices.len()];
    let dc = Mode::Dc { scale: 1.0 };
    if newton(system, circuit, layout, &mut x, &mut states, dc, None, GMIN).is_ok() {
        return Ok((x, states));
    }
    // Gmin stepping: start with a strongly damped circuit.
    let mut x = vec![0.0; layout.size];
    let mut states = vec![[0.0; 2]; circuit.devices.len()];
    let mut ok = true;
    let mut gmin = 1e-2;
    while gmin >= GMIN {
        if newton(system, circuit, layout, &mut x, &mut states, dc, None, gmin).is_err() {
            ok = false;
            break;
        }
        gmin /= 10.0;
    }
    if ok && newton(system, circuit, layout, &mut x, &mut states, dc, None, GMIN).is_ok() {
        return Ok((x, states));
    }
    // Source stepping: bring the supplies up slowly.
    let mut x = vec![0.0; layout.size];
    let mut states = vec![[0.0; 2]; circuit.devices.len()];
    for step in 1..=50 {
        let scale = f64::from(step) / 50.0;
        newton(
            system,
            circuit,
            layout,
            &mut x,
            &mut states,
            Mode::Dc { scale },
            None,
            GMIN,
        )?;
    }
    Ok((x, states))
}

fn device_current(circuit: &Circuit, layout: &Layout, x: &[f64], index: usize) -> Option<f64> {
    let branch = layout.branch[index];
    match &circuit.devices[index].element {
        Element::Resistor { a, b, ohms } => Some((volt(x, *a) - volt(x, *b)) / ohms),
        Element::Inductor { .. } | Element::Voltage { .. } => Some(x[branch]),
        Element::Diode {
            anode,
            cathode,
            model,
        } => Some(diode_current(model, volt(x, *anode) - volt(x, *cathode)).0),
        _ => None,
    }
}

struct Recorder {
    traces: Vec<Trace>,
    current_devices: Vec<usize>,
}

impl Recorder {
    fn new(circuit: &Circuit) -> Self {
        let mut traces: Vec<Trace> = circuit.nodes[1..]
            .iter()
            .map(|name| Trace {
                name: format!("V({name})"),
                unit: "V".to_owned(),
                values: Vec::new(),
            })
            .collect();
        let mut current_devices = Vec::new();
        for (index, device) in circuit.devices.iter().enumerate() {
            if matches!(
                device.element,
                Element::Resistor { .. }
                    | Element::Inductor { .. }
                    | Element::Voltage { .. }
                    | Element::Diode { .. }
            ) {
                current_devices.push(index);
                traces.push(Trace {
                    name: format!("I({})", device.name),
                    unit: "A".to_owned(),
                    values: Vec::new(),
                });
            }
        }
        Self {
            traces,
            current_devices,
        }
    }

    fn record(&mut self, circuit: &Circuit, layout: &Layout, x: &[f64]) {
        for (trace, value) in self.traces.iter_mut().zip(&x[..layout.nodes]) {
            trace.values.push(*value);
        }
        for (slot, index) in self.current_devices.iter().enumerate() {
            let value = device_current(circuit, layout, x, *index).unwrap_or(0.0);
            self.traces[layout.nodes + slot].values.push(value);
        }
    }
}

fn check(circuit: &Circuit) -> Result<(), SimError> {
    if circuit.devices.is_empty() {
        return Err(SimError::Empty);
    }
    let touches_ground = circuit.devices.iter().any(|device| match device.element {
        Element::Resistor { a, b, .. }
        | Element::Capacitor { a, b, .. }
        | Element::Inductor { a, b, .. } => a == GROUND || b == GROUND,
        Element::Voltage { plus, minus, .. } | Element::Current { plus, minus, .. } => {
            plus == GROUND || minus == GROUND
        }
        Element::Diode { anode, cathode, .. } => anode == GROUND || cathode == GROUND,
        Element::Bjt {
            collector,
            base,
            emitter,
            ..
        } => collector == GROUND || base == GROUND || emitter == GROUND,
        Element::Mos {
            drain,
            gate,
            source,
            ..
        } => drain == GROUND || gate == GROUND || source == GROUND,
        Element::Opamp { .. } => true,
    });
    if !touches_ground {
        return Err(SimError::NoGround);
    }
    Ok(())
}

/// Runs an analysis.
pub fn simulate(circuit: &Circuit, analysis: &Analysis) -> Result<SimResult, SimError> {
    check(circuit)?;
    let layout = layout(circuit)?;
    let mut system = System::new(layout.size);
    let result = match analysis {
        Analysis::OperatingPoint => {
            let (x, _) = operating_point(&mut system, circuit, &layout, None)?;
            let mut recorder = Recorder::new(circuit);
            recorder.record(circuit, &layout, &x);
            SimResult {
                x_unit: "point".to_owned(),
                x: vec![0.0],
                traces: recorder.traces,
            }
        }
        Analysis::DcSweep {
            source,
            start,
            stop,
            step,
        } => dc_sweep(&mut system, circuit, &layout, source, *start, *stop, *step)?,
        Analysis::Ac {
            start,
            stop,
            points_per_decade,
        } => ac_sweep(
            &mut system,
            circuit,
            &layout,
            *start,
            *stop,
            *points_per_decade,
        )?,
        Analysis::Transient { stop, step } => {
            transient(&mut system, circuit, &layout, *stop, *step)?
        }
    };
    Ok(result)
}

fn dc_sweep(
    system: &mut System,
    circuit: &Circuit,
    layout: &Layout,
    source_name: &str,
    start: f64,
    stop: f64,
    step: f64,
) -> Result<SimResult, SimError> {
    if !(step != 0.0 && step.is_finite() && (stop - start) / step >= 0.0) {
        return Err(SimError::InvalidSettings);
    }
    let count = ((stop - start) / step).round() as usize + 1;
    if count > MAX_POINTS {
        return Err(SimError::InvalidSettings);
    }
    let index = circuit
        .devices
        .iter()
        .position(|device| {
            device.name == source_name
                && matches!(
                    device.element,
                    Element::Voltage { .. } | Element::Current { .. }
                )
        })
        .ok_or_else(|| SimError::UnknownSource(source_name.to_owned()))?;
    let mut sweep = circuit.clone();
    let mut recorder = Recorder::new(circuit);
    let mut x_axis = Vec::with_capacity(count);
    let mut last: Option<Vec<f64>> = None;
    for point in 0..count {
        let value = start + step * point as f64;
        match &mut sweep.devices[index].element {
            Element::Voltage { source, .. } | Element::Current { source, .. } => {
                *source = Source::dc(value);
            }
            _ => {}
        }
        let (x, _) = operating_point(system, &sweep, layout, last.as_deref())?;
        recorder.record(&sweep, layout, &x);
        x_axis.push(value);
        last = Some(x);
    }
    Ok(SimResult {
        x_unit: "V".to_owned(),
        x: x_axis,
        traces: recorder.traces,
    })
}

#[derive(Clone, Copy)]
struct Complex {
    re: f64,
    im: f64,
}

impl Complex {
    const ZERO: Self = Self { re: 0.0, im: 0.0 };

    fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }

    fn mul(self, o: Self) -> Self {
        Self {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }

    fn sub(self, o: Self) -> Self {
        Self {
            re: self.re - o.re,
            im: self.im - o.im,
        }
    }

    fn div(self, o: Self) -> Self {
        let d = o.re * o.re + o.im * o.im;
        Self {
            re: (self.re * o.re + self.im * o.im) / d,
            im: (self.im * o.re - self.re * o.im) / d,
        }
    }
}

fn complex_solve(n: usize, a: &mut [Complex], b: &mut [Complex]) -> Result<(), SimError> {
    for k in 0..n {
        let mut pivot = k;
        let mut largest = a[k * n + k].abs();
        for row in k + 1..n {
            let value = a[row * n + k].abs();
            if value > largest {
                largest = value;
                pivot = row;
            }
        }
        if largest < 1e-300 {
            return Err(SimError::Singular);
        }
        if pivot != k {
            for column in k..n {
                a.swap(k * n + column, pivot * n + column);
            }
            b.swap(k, pivot);
        }
        for row in k + 1..n {
            let factor = a[row * n + k].div(a[k * n + k]);
            for column in k + 1..n {
                a[row * n + column] = a[row * n + column].sub(factor.mul(a[k * n + column]));
            }
            b[row] = b[row].sub(factor.mul(b[k]));
        }
    }
    for k in (0..n).rev() {
        let mut sum = b[k];
        for column in k + 1..n {
            sum = sum.sub(a[k * n + column].mul(b[column]));
        }
        b[k] = sum.div(a[k * n + k]);
    }
    Ok(())
}

fn ac_sweep(
    system: &mut System,
    circuit: &Circuit,
    layout: &Layout,
    start: f64,
    stop: f64,
    points_per_decade: u32,
) -> Result<SimResult, SimError> {
    if !(start > 0.0 && stop > start && points_per_decade > 0) {
        return Err(SimError::InvalidSettings);
    }
    let decades = (stop / start).log10();
    let count = (decades * f64::from(points_per_decade)).ceil() as usize + 1;
    if count > MAX_POINTS {
        return Err(SimError::InvalidSettings);
    }
    let (op, mut states) = operating_point(system, circuit, layout, None)?;

    // Real part: everything the operating point linearises to.
    let dc = Mode::Dc { scale: 0.0 };
    assemble(
        system,
        circuit,
        layout,
        &op,
        &mut states,
        dc,
        None,
        GMIN,
        false,
    );
    let real = system.a.clone();

    let n = layout.size;
    let mut names: Vec<Trace> = Vec::new();
    for name in &circuit.nodes[1..] {
        names.push(Trace {
            name: format!("V({name})"),
            unit: "V".to_owned(),
            values: Vec::new(),
        });
        names.push(Trace {
            name: format!("P({name})"),
            unit: "deg".to_owned(),
            values: Vec::new(),
        });
    }
    let mut x_axis = Vec::with_capacity(count);
    for point in 0..count {
        let frequency = start * 10f64.powf(point as f64 / f64::from(points_per_decade));
        let omega = std::f64::consts::TAU * frequency;
        let mut a: Vec<Complex> = real.iter().map(|re| Complex { re: *re, im: 0.0 }).collect();
        let mut b = vec![Complex::ZERO; n];
        for (index, device) in circuit.devices.iter().enumerate() {
            let branch = layout.branch[index];
            match &device.element {
                Element::Capacitor { a: p, b: q, farads } => {
                    let y = omega * farads;
                    if *p != GROUND {
                        a[(p - 1) * n + (p - 1)].im += y;
                    }
                    if *q != GROUND {
                        a[(q - 1) * n + (q - 1)].im += y;
                    }
                    if *p != GROUND && *q != GROUND {
                        a[(p - 1) * n + (q - 1)].im -= y;
                        a[(q - 1) * n + (p - 1)].im -= y;
                    }
                }
                Element::Inductor { henries, .. } => {
                    a[branch * n + branch].im -= omega * henries;
                }
                Element::Voltage { source, .. } => {
                    b[branch].re += source.ac;
                }
                Element::Current {
                    plus,
                    minus,
                    source,
                } => {
                    if *plus != GROUND {
                        b[plus - 1].re -= source.ac;
                    }
                    if *minus != GROUND {
                        b[minus - 1].re += source.ac;
                    }
                }
                _ => {}
            }
        }
        complex_solve(n, &mut a, &mut b)?;
        for node in 0..layout.nodes {
            names[2 * node].values.push(b[node].abs());
            names[2 * node + 1]
                .values
                .push(b[node].im.atan2(b[node].re).to_degrees());
        }
        x_axis.push(frequency);
    }
    Ok(SimResult {
        x_unit: "Hz".to_owned(),
        x: x_axis,
        traces: names,
    })
}

/// How finely a damped step is divided.
const DAMPED_DIVISIONS: u32 = 8;

/// Whether a pulse source changes slope between two times.
fn has_corner(circuit: &Circuit, from: f64, to: f64) -> bool {
    circuit.devices.iter().any(|device| {
        let (Element::Voltage { source, .. } | Element::Current { source, .. }) = &device.element
        else {
            return false;
        };
        let Some(Waveform::Pulse {
            delay,
            rise,
            fall,
            width,
            period,
            ..
        }) = source.wave
        else {
            return false;
        };
        let start = if period < f64::MAX && from > delay {
            delay + ((from - delay) / period).floor() * period
        } else {
            delay
        };
        [start, start + period.min(1e30)].iter().any(|base| {
            [
                0.0,
                rise,
                rise + width.min(1e30),
                rise + width.min(1e30) + fall,
            ]
            .iter()
            .any(|offset| {
                let corner = base + offset;
                corner > from && corner <= to
            })
        })
    })
}

fn transient(
    system: &mut System,
    circuit: &Circuit,
    layout: &Layout,
    stop: f64,
    step: f64,
) -> Result<SimResult, SimError> {
    if !(stop > 0.0 && step > 0.0 && stop / step <= MAX_STEPS as f64) {
        return Err(SimError::InvalidSettings);
    }
    let mut at_start = circuit.clone();
    for device in &mut at_start.devices {
        if let Element::Voltage { source, .. } | Element::Current { source, .. } =
            &mut device.element
        {
            *source = source.at_transient_start();
        }
    }
    let (mut x, mut states) = operating_point(system, &at_start, layout, None)?;
    let count = circuit.devices.len();
    let mut past = History {
        voltage: vec![0.0; count],
        current: vec![0.0; count],
    };
    for (index, device) in circuit.devices.iter().enumerate() {
        match device.element {
            Element::Capacitor { a, b, .. } => past.voltage[index] = volt(&x, a) - volt(&x, b),
            Element::Inductor { .. } => past.current[index] = x[layout.branch[index]],
            _ => {}
        }
    }
    let stride = ((stop / step) as usize / MAX_POINTS).max(1);
    let mut recorder = Recorder::new(circuit);
    recorder.record(circuit, layout, &x);
    let mut x_axis = vec![0.0];
    let mut time = 0.0;
    let mut accepted = 0usize;
    // Backward Euler on the first step and around source edges: the trapezoidal
    // rule rings when a time constant is shorter than the step. Backward Euler is
    // only first-order accurate, so these steps are divided to keep edges in time.
    let mut damped_steps = DAMPED_DIVISIONS;
    while time < stop - step * 1e-9 {
        if has_corner(circuit, time, time + step) {
            damped_steps = damped_steps.max(2 * DAMPED_DIVISIONS);
        }
        let trapezoidal = damped_steps == 0;
        let wanted = if trapezoidal {
            step
        } else {
            step / f64::from(DAMPED_DIVISIONS)
        };
        let mut h = wanted.min(stop - time);
        let mut attempt = x.clone();
        let mut attempt_states = states.clone();
        let mut halvings = 0;
        loop {
            let mode = Mode::Tran {
                h,
                trapezoidal,
                time: time + h,
            };
            match newton(
                system,
                circuit,
                layout,
                &mut attempt,
                &mut attempt_states,
                mode,
                Some(&past),
                GMIN,
            ) {
                Ok(()) => break,
                Err(error) => {
                    halvings += 1;
                    if halvings > 12 {
                        return Err(error);
                    }
                    h /= 2.0;
                    attempt.copy_from_slice(&x);
                    attempt_states.clone_from(&states);
                }
            }
        }
        // Update what capacitors and inductors remember.
        for (index, device) in circuit.devices.iter().enumerate() {
            match device.element {
                Element::Capacitor { a, b, farads } => {
                    let v = volt(&attempt, a) - volt(&attempt, b);
                    past.current[index] = if !trapezoidal {
                        farads / h * (v - past.voltage[index])
                    } else {
                        2.0 * farads / h * (v - past.voltage[index]) - past.current[index]
                    };
                    past.voltage[index] = v;
                }
                Element::Inductor { a, b, .. } => {
                    past.current[index] = attempt[layout.branch[index]];
                    past.voltage[index] = volt(&attempt, a) - volt(&attempt, b);
                }
                _ => {}
            }
        }
        x = attempt;
        states = attempt_states;
        time += h;
        damped_steps = damped_steps.saturating_sub(1);
        accepted += 1;
        if accepted.is_multiple_of(stride) || time >= stop - step * 1e-9 {
            recorder.record(circuit, layout, &x);
            x_axis.push(time);
        }
        if accepted as u64 > MAX_STEPS {
            return Err(SimError::InvalidSettings);
        }
    }
    Ok(SimResult {
        x_unit: "s".to_owned(),
        x: x_axis,
        traces: recorder.traces,
    })
}
