//! A circuit as devices between numbered nodes. Node 0 is ground.

use serde::Serialize;
use std::fmt::Write;

pub const GROUND: usize = 0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Waveform {
    Sine {
        offset: f64,
        amplitude: f64,
        frequency: f64,
        delay: f64,
    },
    Pulse {
        v1: f64,
        v2: f64,
        delay: f64,
        rise: f64,
        fall: f64,
        width: f64,
        period: f64,
    },
}

impl Waveform {
    pub fn at(&self, time: f64) -> f64 {
        match *self {
            Self::Sine {
                offset,
                amplitude,
                frequency,
                delay,
            } => {
                if time < delay {
                    offset
                } else {
                    offset + amplitude * (std::f64::consts::TAU * frequency * (time - delay)).sin()
                }
            }
            Self::Pulse {
                v1,
                v2,
                delay,
                rise,
                fall,
                width,
                period,
            } => {
                if time < delay {
                    return v1;
                }
                let t = if period < f64::MAX {
                    (time - delay) % period
                } else {
                    time - delay
                };
                if t < rise {
                    v1 + (v2 - v1) * t / rise
                } else if t < rise + width {
                    v2
                } else if t < rise + width + fall {
                    v2 + (v1 - v2) * (t - rise - width) / fall
                } else {
                    v1
                }
            }
        }
    }
}

/// DC value, AC magnitude and an optional time-domain shape.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Source {
    pub dc: f64,
    /// Whether `dc` was stated. If not, a waveform's value at time zero applies.
    pub dc_given: bool,
    pub ac: f64,
    pub wave: Option<Waveform>,
}

impl Source {
    pub fn dc(value: f64) -> Self {
        Self {
            dc: value,
            dc_given: true,
            ..Self::default()
        }
    }

    pub fn at(&self, time: f64) -> f64 {
        self.wave.map_or(self.dc, |wave| wave.at(time))
    }

    /// The value used for an operating point of its own, a DC sweep and the bias
    /// of an AC analysis: the stated DC value, as in SPICE.
    pub fn at_dc(&self) -> f64 {
        match self.wave {
            Some(wave) if !self.dc_given => wave.at(0.0),
            _ => self.dc,
        }
    }

    /// The same source as seen by the operating point a transient starts from:
    /// there the waveform decides, whatever the DC value says.
    pub fn at_transient_start(&self) -> Self {
        Self {
            dc: self.at(0.0),
            dc_given: true,
            ac: self.ac,
            wave: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DiodeModel {
    pub is: f64,
    pub n: f64,
    /// Reverse breakdown voltage; infinite means none.
    pub bv: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BjtModel {
    pub is: f64,
    pub bf: f64,
    pub br: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MosModel {
    /// Transconductance parameter times W/L, in A/V².
    pub kp: f64,
    /// Threshold voltage magnitude.
    pub vto: f64,
    pub lambda: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Element {
    Resistor {
        a: usize,
        b: usize,
        ohms: f64,
    },
    Capacitor {
        a: usize,
        b: usize,
        farads: f64,
    },
    Inductor {
        a: usize,
        b: usize,
        henries: f64,
    },
    Voltage {
        plus: usize,
        minus: usize,
        source: Source,
    },
    Current {
        plus: usize,
        minus: usize,
        source: Source,
    },
    Diode {
        anode: usize,
        cathode: usize,
        model: DiodeModel,
    },
    Bjt {
        collector: usize,
        base: usize,
        emitter: usize,
        pnp: bool,
        model: BjtModel,
    },
    Mos {
        drain: usize,
        gate: usize,
        source: usize,
        p_channel: bool,
        model: MosModel,
    },
    /// Ideal voltage-controlled output, no supply limits.
    Opamp {
        output: usize,
        plus: usize,
        minus: usize,
        gain: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Device {
    pub name: String,
    pub element: Element,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Circuit {
    /// Node names; index 0 is ground.
    pub nodes: Vec<String>,
    pub devices: Vec<Device>,
}

fn source_text(source: &Source) -> String {
    let mut out = format!("DC {}", source.at_dc());
    if source.ac != 0.0 {
        let _ = write!(out, " AC {}", source.ac);
    }
    match source.wave {
        Some(Waveform::Sine {
            offset,
            amplitude,
            frequency,
            delay,
        }) => {
            let _ = write!(out, " SIN({offset} {amplitude} {frequency} {delay})");
        }
        Some(Waveform::Pulse {
            v1,
            v2,
            delay,
            rise,
            fall,
            width,
            period,
        }) => {
            let width = width.min(1e30);
            let period = period.min(1e30);
            let _ = write!(
                out,
                " PULSE({v1} {v2} {delay} {rise} {fall} {width} {period})"
            );
        }
        None => {}
    }
    out
}

impl Circuit {
    pub fn new() -> Self {
        Self {
            nodes: vec!["0".to_owned()],
            devices: Vec::new(),
        }
    }

    /// The index of a node, added if new.
    pub fn node(&mut self, name: &str) -> usize {
        if name == "0" {
            return GROUND;
        }
        match self.nodes.iter().position(|node| node == name) {
            Some(index) => index,
            None => {
                self.nodes.push(name.to_owned());
                self.nodes.len() - 1
            }
        }
    }

    pub fn add(&mut self, name: &str, element: Element) {
        self.devices.push(Device {
            name: name.to_owned(),
            element,
        });
    }

    /// Writes the circuit as a SPICE deck, with every model parameter spelled out.
    pub fn to_spice(&self, title: &str) -> String {
        // SPICE names must be plain words, and the first letter gives the type.
        let node = |index: usize| {
            if index == GROUND {
                "0".to_owned()
            } else {
                format!("n{index}")
            }
        };
        let mut out = format!("* {title}\n");
        for (index, name) in self.nodes.iter().enumerate().skip(1) {
            let _ = writeln!(out, "* n{index} = {name}");
        }
        for (number, device) in self.devices.iter().enumerate() {
            let id = number + 1;
            let _ = match &device.element {
                Element::Resistor { a, b, ohms } => {
                    writeln!(out, "R{id} {} {} {ohms}", node(*a), node(*b))
                }
                Element::Capacitor { a, b, farads } => {
                    writeln!(out, "C{id} {} {} {farads}", node(*a), node(*b))
                }
                Element::Inductor { a, b, henries } => {
                    writeln!(out, "L{id} {} {} {henries}", node(*a), node(*b))
                }
                Element::Voltage {
                    plus,
                    minus,
                    source,
                } => {
                    writeln!(
                        out,
                        "V{id} {} {} {}",
                        node(*plus),
                        node(*minus),
                        source_text(source)
                    )
                }
                Element::Current {
                    plus,
                    minus,
                    source,
                } => {
                    writeln!(
                        out,
                        "I{id} {} {} {}",
                        node(*plus),
                        node(*minus),
                        source_text(source)
                    )
                }
                Element::Diode {
                    anode,
                    cathode,
                    model,
                } => {
                    let breakdown = if model.bv.is_finite() {
                        format!(" BV={}", model.bv)
                    } else {
                        String::new()
                    };
                    writeln!(
                        out,
                        "D{id} {} {} DM{id}\n.model DM{id} D(IS={} N={}{breakdown})",
                        node(*anode),
                        node(*cathode),
                        model.is,
                        model.n
                    )
                }
                Element::Bjt {
                    collector,
                    base,
                    emitter,
                    pnp,
                    model,
                } => writeln!(
                    out,
                    "Q{id} {} {} {} QM{id}\n.model QM{id} {}(IS={} BF={} BR={})",
                    node(*collector),
                    node(*base),
                    node(*emitter),
                    if *pnp { "PNP" } else { "NPN" },
                    model.is,
                    model.bf,
                    model.br
                ),
                Element::Mos {
                    drain,
                    gate,
                    source,
                    p_channel,
                    model,
                } => {
                    let vto = if *p_channel { -model.vto } else { model.vto };
                    writeln!(
                        out,
                        "M{id} {} {} {} {} MM{id} L=1 W=1\n.model MM{id} {}(LEVEL=1 KP={} VTO={vto} LAMBDA={})",
                        node(*drain),
                        node(*gate),
                        node(*source),
                        node(*source),
                        if *p_channel { "PMOS" } else { "NMOS" },
                        model.kp,
                        model.lambda
                    )
                }
                Element::Opamp {
                    output,
                    plus,
                    minus,
                    gain,
                } => writeln!(
                    out,
                    "E{id} {} 0 {} {} {gain}",
                    node(*output),
                    node(*plus),
                    node(*minus)
                ),
            };
        }
        out
    }
}
