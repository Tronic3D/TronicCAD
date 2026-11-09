//! Reads component values and source descriptions as written on schematics.

use crate::circuit::{Source, Waveform};

fn suffix_factor(suffix: &str) -> Option<f64> {
    let lower = suffix.to_ascii_lowercase();
    if lower.starts_with("meg") {
        return Some(1e6);
    }
    match lower.chars().next() {
        None => Some(1.0),
        Some('t') => Some(1e12),
        Some('g') => Some(1e9),
        Some('k') => Some(1e3),
        // Lower case m is milli as in SPICE, and M means mega on schematics.
        Some('m') => Some(if suffix.starts_with('M') { 1e6 } else { 1e-3 }),
        Some('u' | 'µ' | 'μ') => Some(1e-6),
        Some('n') => Some(1e-9),
        Some('p') => Some(1e-12),
        Some('f') => Some(1e-15),
        Some('r' | 'v' | 'a' | 'h' | 'w') => Some(1.0),
        Some('ω' | 'Ω') => Some(1.0),
        Some(_) => None,
    }
}

/// Parses `10k`, `4k7`, `100n`, `1meg`, `3V3`, `2.2u`, `1e-3`, `470R`, `5 V`.
pub fn parse_value(text: &str) -> Option<f64> {
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if text.is_empty() {
        return None;
    }
    let bytes: Vec<char> = text.chars().collect();
    let mut end = 0;
    while end < bytes.len() {
        let c = bytes[end];
        let exponent = (c == 'e' || c == 'E')
            && end > 0
            && bytes
                .get(end + 1)
                .is_some_and(|n| n.is_ascii_digit() || *n == '-' || *n == '+');
        let sign_of_exponent =
            (c == '-' || c == '+') && end > 0 && matches!(bytes[end - 1], 'e' | 'E');
        let leading_sign = (c == '-' || c == '+') && end == 0;
        if c.is_ascii_digit() || c == '.' || exponent || sign_of_exponent || leading_sign {
            end += 1;
        } else {
            break;
        }
    }
    let number: String = bytes[..end].iter().collect();
    let base: f64 = number.parse().ok()?;
    if !base.is_finite() {
        return None;
    }
    let rest: String = bytes[end..].iter().collect();
    if rest.is_empty() {
        return Some(base);
    }
    let factor = suffix_factor(&rest)?;
    // A digit after the unit letter is the decimal part: 4k7, 3V3, 2R2.
    let tail: String = rest.chars().skip_while(|c| !c.is_ascii_digit()).collect();
    let letters = rest.chars().take_while(|c| !c.is_ascii_digit()).count();
    // Part numbers like 1N4148 look similar but have more digits.
    if tail.len() > 3 {
        return None;
    }
    if !tail.is_empty() && letters == 1 && tail.chars().all(|c| c.is_ascii_digit()) {
        let fraction: f64 = format!("0.{tail}").parse().ok()?;
        return Some(
            if base < 0.0 {
                base - fraction
            } else {
                base + fraction
            } * factor,
        );
    }
    Some(base * factor)
}

fn arguments(text: &str) -> Option<(String, Vec<f64>)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let name = text[..open].trim().to_ascii_uppercase();
    let values = text[open + 1..close]
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .map(parse_value)
        .collect::<Option<Vec<f64>>>()?;
    Some((name, values))
}

/// Parses a source description: `5`, `DC 5`, `AC 1`, `SIN(0 1 1k)`,
/// `PULSE(0 5 0 1n 1n 1m 2m)`, or several of these together.
pub fn parse_source(text: &str) -> Option<Source> {
    let mut source = Source::default();
    let mut rest = text.trim();
    if rest.is_empty() {
        return None;
    }
    let mut understood = false;
    while !rest.is_empty() {
        let upper = rest.to_ascii_uppercase();
        if upper.starts_with("SIN") || upper.starts_with("PULSE") {
            let close = rest.find(')')?;
            let (name, v) = arguments(&rest[..=close])?;
            if name == "PULSE" {
                source.wave = Some(Waveform::Pulse {
                    v1: *v.first()?,
                    v2: *v.get(1)?,
                    delay: v.get(2).copied().unwrap_or(0.0),
                    rise: v.get(3).copied().unwrap_or(1e-9),
                    fall: v.get(4).copied().unwrap_or(1e-9),
                    width: v.get(5).copied().unwrap_or(f64::MAX),
                    period: v.get(6).copied().unwrap_or(f64::MAX),
                });
            } else {
                source.wave = Some(Waveform::Sine {
                    offset: *v.first()?,
                    amplitude: *v.get(1)?,
                    frequency: *v.get(2)?,
                    delay: v.get(3).copied().unwrap_or(0.0),
                });
            }
            rest = rest[close + 1..].trim_start();
            understood = true;
            continue;
        }
        let mut words = rest.splitn(2, char::is_whitespace);
        let word = words.next()?;
        let after = words.next().unwrap_or("").trim_start();
        match word.to_ascii_uppercase().as_str() {
            "DC" | "AC" => {
                let mut inner = after.splitn(2, char::is_whitespace);
                let value = parse_value(inner.next()?)?;
                if word.eq_ignore_ascii_case("DC") {
                    source.dc = value;
                    source.dc_given = true;
                } else {
                    source.ac = value;
                }
                rest = inner.next().unwrap_or("").trim_start();
            }
            _ => {
                source.dc = parse_value(word)?;
                source.dc_given = true;
                rest = after;
            }
        }
        understood = true;
    }
    understood.then_some(source)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() <= b.abs() * 1e-9, "{a} != {b}");
    }

    #[test]
    fn reads_engineering_notation() {
        for (text, expected) in [
            ("10k", 10e3),
            ("4k7", 4700.0),
            ("2R2", 2.2),
            ("100n", 100e-9),
            ("1u", 1e-6),
            ("2.2u", 2.2e-6),
            ("1µ", 1e-6),
            ("1meg", 1e6),
            ("1M", 1e6),
            ("1m", 1e-3),
            ("470", 470.0),
            ("470R", 470.0),
            ("10p", 10e-12),
            ("1e-3", 1e-3),
            ("5 V", 5.0),
            ("3V3", 3.3),
            ("5V1", 5.1),
            ("-12V", -12.0),
            ("10uF", 10e-6),
            ("4.7kΩ", 4700.0),
        ] {
            close(parse_value(text).unwrap(), expected);
        }
    }

    #[test]
    fn rejects_what_is_not_a_value() {
        for text in ["", "abc", "1N4148", "NE555", "1x2"] {
            assert_eq!(parse_value(text), None, "{text}");
        }
    }

    #[test]
    fn reads_sources() {
        assert_eq!(parse_source("5").unwrap().dc, 5.0);
        assert_eq!(parse_source("DC 12").unwrap().dc, 12.0);
        let both = parse_source("DC 2 AC 1").unwrap();
        assert_eq!((both.dc, both.ac), (2.0, 1.0));
        let sine = parse_source("SIN(0 1 1k)").unwrap();
        assert_eq!(
            sine.wave,
            Some(Waveform::Sine {
                offset: 0.0,
                amplitude: 1.0,
                frequency: 1000.0,
                delay: 0.0
            })
        );
        let pulse = parse_source("PULSE(0 5 0 1n 1n 1m 2m) AC 1").unwrap();
        assert_eq!(pulse.ac, 1.0);
        assert!(matches!(pulse.wave, Some(Waveform::Pulse { v2, .. }) if v2 == 5.0));
    }

    #[test]
    fn rejects_broken_sources() {
        for text in ["", "SIN(1)", "PULSE(", "DC", "hello"] {
            assert_eq!(parse_source(text), None, "{text}");
        }
    }
}
