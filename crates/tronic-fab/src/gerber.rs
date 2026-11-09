//! A Gerber RS-274X writer. Coordinates are written in nanometres, which is
//! format 4.6 in millimetres. Gerber's Y axis points up, ours down.

use std::collections::BTreeMap;
use std::fmt::Write;
use tronic_pcb::PadShape;
use tronic_schematic::Point;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Positive,
    Negative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Aperture {
    Circle(i64),
    Rect(i64, i64),
    Oblong(i64, i64),
}

pub struct Gerber {
    header: String,
    body: String,
    apertures: BTreeMap<Aperture, u32>,
    current: Option<u32>,
    dark: bool,
    position: Option<Point>,
}

fn mm(nm: i64) -> String {
    format!("{:.6}", nm as f64 / 1_000_000.0)
}

impl Gerber {
    pub fn new(function: &str, polarity: Polarity) -> Self {
        let mut header = String::new();
        let _ = writeln!(
            header,
            "%TF.GenerationSoftware,TronicCAD,TronicCAD,{}*%",
            env!("CARGO_PKG_VERSION")
        );
        let _ = writeln!(header, "%TF.FileFunction,{function}*%");
        let _ = writeln!(
            header,
            "%TF.FilePolarity,{}*%",
            match polarity {
                Polarity::Positive => "Positive",
                Polarity::Negative => "Negative",
            }
        );
        header.push_str("%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\n");
        Self {
            header,
            body: String::new(),
            apertures: BTreeMap::new(),
            current: None,
            dark: true,
            position: None,
        }
    }

    fn select(&mut self, aperture: Aperture) {
        let next = 10 + self.apertures.len() as u32;
        let code = *self.apertures.entry(aperture).or_insert(next);
        if self.current != Some(code) {
            let _ = writeln!(self.body, "D{code}*");
            self.current = Some(code);
        }
    }

    fn coordinate(p: Point) -> String {
        format!("X{}Y{}", p.x, -p.y)
    }

    fn set_dark(&mut self, dark: bool) {
        if self.dark != dark {
            self.body
                .push_str(if dark { "%LPD*%\n" } else { "%LPC*%\n" });
            self.dark = dark;
        }
    }

    /// A line of the given width with round ends.
    pub fn stroke(&mut self, a: Point, b: Point, width: i64) {
        self.set_dark(true);
        self.select(Aperture::Circle(width));
        if self.position != Some(a) {
            let _ = writeln!(self.body, "{}D02*", Self::coordinate(a));
        }
        let _ = writeln!(self.body, "{}D01*", Self::coordinate(b));
        self.position = Some(b);
    }

    pub fn flash(&mut self, at: Point, shape: PadShape, size: Point) {
        self.set_dark(true);
        self.select(match shape {
            PadShape::Round => Aperture::Circle(size.x),
            PadShape::Rect => Aperture::Rect(size.x, size.y),
            PadShape::Oblong => Aperture::Oblong(size.x, size.y),
        });
        let _ = writeln!(self.body, "{}D03*", Self::coordinate(at));
        self.position = Some(at);
    }

    fn region(&mut self, contour: &[Point]) {
        let Some(first) = contour.first() else {
            return;
        };
        self.body.push_str("G36*\n");
        let _ = writeln!(self.body, "{}D02*", Self::coordinate(*first));
        for point in &contour[1..] {
            let _ = writeln!(self.body, "{}D01*", Self::coordinate(*point));
        }
        let _ = writeln!(self.body, "{}D01*", Self::coordinate(*first));
        self.body.push_str("G37*\n");
        self.position = None;
    }

    /// A filled area: first contour dark, the following ones cleared.
    pub fn region_with_holes(&mut self, contours: &[Vec<Point>]) {
        for (index, contour) in contours.iter().enumerate() {
            self.set_dark(index == 0);
            self.region(contour);
        }
        self.set_dark(true);
    }

    pub fn finish(self) -> String {
        let mut out = self.header;
        for (aperture, code) in &self.apertures {
            let _ = match aperture {
                Aperture::Circle(d) => writeln!(out, "%ADD{code}C,{}*%", mm(*d)),
                Aperture::Rect(w, h) => writeln!(out, "%ADD{code}R,{}X{}*%", mm(*w), mm(*h)),
                Aperture::Oblong(w, h) => writeln!(out, "%ADD{code}O,{}X{}*%", mm(*w), mm(*h)),
            };
        }
        out.push_str("G01*\n");
        out.push_str(&self.body);
        out.push_str("M02*\n");
        out
    }
}
