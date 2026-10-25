// excellon.rs — Excellon drill file parser (Phase 2)
//
// Supports METRIC/INCH headers, TZ/LZ zero suppression, decimal coordinates,
// T##C#.## tool definitions, drill hits, and G85 routed slots.
use std::collections::HashMap;

use super::ParseOutput;
use crate::workspace::object::{DrillHit, Geometry};

fn num_after(s: &str, key: char) -> Option<&str> {
    let pos = s.find(key)?;
    let rest = &s[pos + 1..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(rest.len());
    if end == 0 { None } else { Some(&rest[..end]) }
}

struct Fmt {
    metric: bool,
    int_digits: usize,
    dec_digits: usize,
    /// true = leading zeros suppressed (TZ), false = trailing suppressed (LZ)
    leading_suppressed: bool,
}

impl Fmt {
    fn parse(&self, s: &str) -> f64 {
        let v = if s.contains('.') {
            s.parse::<f64>().unwrap_or(0.0)
        } else {
            let (neg, digits) = match s.chars().next() {
                Some('-') => (true, &s[1..]),
                Some('+') => (false, &s[1..]),
                _ => (false, s),
            };
            let mut d = digits.to_string();
            if !self.leading_suppressed {
                while d.len() < self.int_digits + self.dec_digits {
                    d.push('0');
                }
            }
            let v = d.parse::<f64>().unwrap_or(0.0) / 10f64.powi(self.dec_digits as i32);
            if neg { -v } else { v }
        };
        if self.metric { v } else { v * 25.4 }
    }
}

pub fn parse(source: &str) -> anyhow::Result<ParseOutput> {
    let mut geom = Geometry::default();
    let mut notes = Vec::new();
    let mut fmt = Fmt { metric: true, int_digits: 3, dec_digits: 3, leading_suppressed: false };
    let mut format_from_file = false;
    let mut tools: HashMap<u32, f64> = HashMap::new();
    let mut cur_tool: Option<u32> = None;
    let mut pos = [0.0_f64, 0.0_f64];
    let mut warned_tool = false;

    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(c) = line.strip_prefix(';') {
            if let Some(i) = c.find("FILE_FORMAT=") {
                let spec = &c[i + 12..];
                if let Some((a, b)) = spec.split_once(':') {
                    if let (Ok(a), Ok(b)) = (a.trim().parse(), b.trim().parse()) {
                        fmt.int_digits = a;
                        fmt.dec_digits = b;
                        format_from_file = true;
                    }
                }
            }
            continue;
        }
        let up = line.to_uppercase();

        if up.starts_with("METRIC") || up.starts_with("INCH") {
            fmt.metric = up.starts_with("METRIC");
            if up.contains(",TZ") {
                fmt.leading_suppressed = true;
            } else if up.contains(",LZ") {
                fmt.leading_suppressed = false;
            }
            if !format_from_file {
                fmt.int_digits = if fmt.metric { 3 } else { 2 };
                fmt.dec_digits = if fmt.metric { 3 } else { 4 };
            }
            continue;
        }
        if up.starts_with("M71") {
            fmt.metric = true;
            continue;
        }
        if up.starts_with("M72") {
            fmt.metric = false;
            continue;
        }
        if up.starts_with("M30") {
            break;
        }

        // Tool definition / selection
        if up.starts_with('T') {
            let digits: String = up[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(t) = digits.parse::<u32>() {
                if let Some(d) = num_after(&up[1 + digits.len()..], 'C') {
                    let dia = d.parse::<f64>().unwrap_or(0.0);
                    tools.insert(t, if fmt.metric { dia } else { dia * 25.4 });
                }
                if t > 0 {
                    cur_tool = Some(t);
                }
            }
            continue;
        }

        // Coordinates
        if up.starts_with('X') || up.starts_with('Y') {
            let (first, second) = match up.split_once("G85") {
                Some((a, b)) => (a.to_string(), Some(b.to_string())),
                None => (up.clone(), None),
            };
            if let Some(v) = num_after(&first, 'X') {
                pos[0] = fmt.parse(v);
            }
            if let Some(v) = num_after(&first, 'Y') {
                pos[1] = fmt.parse(v);
            }
            let dia = match cur_tool.and_then(|t| tools.get(&t)).copied() {
                Some(d) => d,
                None => {
                    if !warned_tool {
                        notes.push("Drill hit with undefined tool — assuming 0.8 mm".into());
                        warned_tool = true;
                    }
                    0.8
                }
            };
            if let Some(second) = second {
                let mut end = pos;
                if let Some(v) = num_after(&second, 'X') {
                    end[0] = fmt.parse(v);
                }
                if let Some(v) = num_after(&second, 'Y') {
                    end[1] = fmt.parse(v);
                }
                geom.add_stroke(&[pos, end], dia);
                pos = end;
            } else {
                geom.add_circle(pos, dia * 0.5);
                geom.drills.push(DrillHit { x: pos[0], y: pos[1], diameter: dia });
            }
        }
    }

    if geom.is_empty() {
        anyhow::bail!("Excellon file contained no drill hits");
    }
    notes.push(format!("{} drill hits, {} tool(s)", geom.drills.len(), tools.len()));
    Ok(ParseOutput { geometry: geom, notes })
}
