// gerber.rs — Gerber RS-274X parser (Phase 2)
//
// Supported: FS/MO, AD (C/R/O/P + macros), AM (circle, vector line, center line,
// outline, polygon primitives with $n variables + arithmetic), D01/D02/D03,
// G01/G02/G03, G74/G75, G36/G37 regions, LPD/LPC (clear polarity is skipped).
// Not supported (noted in output): SR, AB, LM/LR/LS, thermal/moire primitives.
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use super::ParseOutput;
use crate::workspace::object::{arc_points, circle_poly, Geometry, P};

#[derive(Clone)]
struct Aperture {
    polys: Vec<Vec<P>>,
    stroke_width: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum Interp {
    Linear,
    Cw,
    Ccw,
}

struct Parser {
    geom: Geometry,
    notes: Vec<String>,
    int_digits: usize,
    dec_digits: usize,
    trailing_omitted: bool,
    unit_scale: f64,
    apertures: HashMap<i32, Aperture>,
    macros: HashMap<String, Vec<String>>,
    cur_ap: i32,
    cur: P,
    interp: Interp,
    multi_quadrant: bool,
    region: bool,
    contour: Vec<P>,
    dark: bool,
    last_d: i32,
    skipped_clear: bool,
    unsupported: Vec<String>,
}

enum Item {
    Ext(Vec<String>),
    Word(String),
}

fn tokenize(src: &str) -> Vec<Item> {
    let mut items = Vec::new();
    let mut chars = src.chars();
    let mut cur = String::new();
    while let Some(c) = chars.next() {
        match c {
            '%' => {
                let mut ext = String::new();
                for c2 in chars.by_ref() {
                    if c2 == '%' {
                        break;
                    }
                    if c2 != '\n' && c2 != '\r' {
                        ext.push(c2);
                    }
                }
                let blocks = ext
                    .split('*')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                items.push(Item::Ext(blocks));
            }
            '*' => {
                let w = cur.trim().to_string();
                cur.clear();
                if !w.is_empty() {
                    items.push(Item::Word(w));
                }
            }
            '\r' | '\n' => {}
            _ => cur.push(c),
        }
    }
    items
}

/// Split "G01X100Y200D01" → [('G',"01"),('X',"100"),('Y',"200"),('D',"01")]
fn split_word(s: &str) -> Vec<(char, String)> {
    let mut out: Vec<(char, String)> = Vec::new();
    for c in s.chars() {
        if c.is_ascii_alphabetic() {
            out.push((c.to_ascii_uppercase(), String::new()));
        } else if !c.is_whitespace() {
            if let Some(last) = out.last_mut() {
                last.1.push(c);
            }
        }
    }
    out
}

fn rot(p: P, deg: f64) -> P {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

fn obround_poly(w: f64, h: f64) -> Vec<P> {
    let horizontal = w >= h;
    let (long, short) = if horizontal { (w, h) } else { (h, w) };
    let r = short * 0.5;
    let off = long * 0.5 - r;
    let mut pts = arc_points([off, 0.0], r, -PI / 2.0, PI);
    pts.extend(arc_points([-off, 0.0], r, PI / 2.0, PI));
    if !horizontal {
        for p in pts.iter_mut() {
            *p = [p[1], p[0]];
        }
    }
    pts
}

// ── tiny expression evaluator for aperture macros ($1, +, -, x, /, parens) ──
struct Ev<'a> {
    s: &'a [u8],
    pos: usize,
    vars: &'a HashMap<u32, f64>,
}

impl<'a> Ev<'a> {
    fn ws(&mut self) {
        while self.pos < self.s.len() && self.s[self.pos] == b' ' {
            self.pos += 1;
        }
    }
    fn expr(&mut self) -> f64 {
        let mut v = self.term();
        loop {
            self.ws();
            match self.s.get(self.pos) {
                Some(b'+') => {
                    self.pos += 1;
                    v += self.term();
                }
                Some(b'-') => {
                    self.pos += 1;
                    v -= self.term();
                }
                _ => return v,
            }
        }
    }
    fn term(&mut self) -> f64 {
        let mut v = self.factor();
        loop {
            self.ws();
            match self.s.get(self.pos) {
                Some(b'x') | Some(b'X') | Some(b'*') => {
                    self.pos += 1;
                    v *= self.factor();
                }
                Some(b'/') => {
                    self.pos += 1;
                    let d = self.factor();
                    if d != 0.0 {
                        v /= d;
                    }
                }
                _ => return v,
            }
        }
    }
    fn factor(&mut self) -> f64 {
        self.ws();
        match self.s.get(self.pos) {
            Some(b'-') => {
                self.pos += 1;
                -self.factor()
            }
            Some(b'+') => {
                self.pos += 1;
                self.factor()
            }
            Some(b'(') => {
                self.pos += 1;
                let v = self.expr();
                self.ws();
                if self.s.get(self.pos) == Some(&b')') {
                    self.pos += 1;
                }
                v
            }
            Some(b'$') => {
                self.pos += 1;
                let st = self.pos;
                while self.pos < self.s.len() && self.s[self.pos].is_ascii_digit() {
                    self.pos += 1;
                }
                let n: u32 = std::str::from_utf8(&self.s[st..self.pos]).unwrap_or("0").parse().unwrap_or(0);
                *self.vars.get(&n).unwrap_or(&0.0)
            }
            _ => {
                let st = self.pos;
                while self.pos < self.s.len() && (self.s[self.pos].is_ascii_digit() || self.s[self.pos] == b'.') {
                    self.pos += 1;
                }
                std::str::from_utf8(&self.s[st..self.pos]).unwrap_or("0").parse().unwrap_or(0.0)
            }
        }
    }
}

fn eval(s: &str, vars: &HashMap<u32, f64>) -> f64 {
    let mut e = Ev { s: s.as_bytes(), pos: 0, vars };
    e.expr()
}

fn arc_sweep(s: P, e: P, c: P, ccw: bool, multi: bool) -> f64 {
    let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
    let a1 = (e[1] - c[1]).atan2(e[0] - c[0]);
    let mut sweep = a1 - a0;
    if ccw {
        while sweep <= 1e-9 {
            sweep += TAU;
        }
    } else {
        while sweep >= -1e-9 {
            sweep -= TAU;
        }
    }
    let _ = multi;
    sweep
}

impl Parser {
    fn new() -> Self {
        Self {
            geom: Geometry::default(),
            notes: Vec::new(),
            int_digits: 2,
            dec_digits: 4,
            trailing_omitted: false,
            unit_scale: 1.0,
            apertures: HashMap::new(),
            macros: HashMap::new(),
            cur_ap: 0,
            cur: [0.0, 0.0],
            interp: Interp::Linear,
            multi_quadrant: true,
            region: false,
            contour: Vec::new(),
            dark: true,
            last_d: 2,
            skipped_clear: false,
            unsupported: Vec::new(),
        }
    }

    fn coord(&self, s: &str) -> f64 {
        if s.contains('.') {
            return s.parse::<f64>().unwrap_or(0.0) * self.unit_scale;
        }
        let (neg, digits) = match s.chars().next() {
            Some('-') => (true, &s[1..]),
            Some('+') => (false, &s[1..]),
            _ => (false, s),
        };
        let total = self.int_digits + self.dec_digits;
        let mut d = digits.to_string();
        if self.trailing_omitted {
            while d.len() < total {
                d.push('0');
            }
        }
        let v = d.parse::<f64>().unwrap_or(0.0) / 10f64.powi(self.dec_digits as i32);
        (if neg { -v } else { v }) * self.unit_scale
    }

    fn handle_ext(&mut self, blocks: &[String]) {
        let Some(first) = blocks.first() else { return };
        if first.starts_with("FS") {
            self.trailing_omitted = first.contains('T');
            if let Some(xpos) = first.find('X') {
                let b = first.as_bytes();
                if xpos + 2 < b.len() {
                    self.int_digits = (b[xpos + 1] as char).to_digit(10).unwrap_or(2) as usize;
                    self.dec_digits = (b[xpos + 2] as char).to_digit(10).unwrap_or(4) as usize;
                }
            }
        } else if first.starts_with("MO") {
            self.unit_scale = if first.contains("IN") { 25.4 } else { 1.0 };
        } else if first.starts_with("AD") {
            self.define_aperture(first);
        } else if first.starts_with("AM") {
            let name = first[2..].to_string();
            self.macros.insert(name, blocks[1..].to_vec());
        } else if first.starts_with("LP") {
            self.dark = first.contains('D');
            if !self.dark {
                self.skipped_clear = true;
            }
        } else if first.starts_with("SR") && first.len() > 2 {
            // "SR" with parameters = start step/repeat
            if first.contains('X') || first.contains('Y') {
                self.unsupported.push("Step & Repeat (SR)".into());
            }
        } else if first.starts_with("AB") {
            self.unsupported.push("Aperture blocks (AB)".into());
        } else if first.starts_with("LM") || first.starts_with("LR") || first.starts_with("LS") {
            self.unsupported.push("Aperture transforms (LM/LR/LS)".into());
        }
        // TF/TA/TD/IP/OF etc. are ignored silently.
    }

    fn define_aperture(&mut self, blk: &str) {
        let rest = &blk[2..];
        let Some(rest) = rest.strip_prefix('D') else { return };
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(code) = digits.parse::<i32>() else { return };
        let rest = &rest[digits.len()..];
        let (name, params_str) = match rest.split_once(',') {
            Some((n, p)) => (n, p),
            None => (rest, ""),
        };
        let params: Vec<f64> = params_str
            .split('X')
            .filter_map(|p| p.trim().parse::<f64>().ok())
            .collect();
        let us = self.unit_scale;
        let p = |i: usize| params.get(i).copied().unwrap_or(0.0);
        let ap = match name {
            "C" => Aperture {
                polys: vec![circle_poly([0.0, 0.0], p(0) * us * 0.5)],
                stroke_width: p(0) * us,
            },
            "R" => {
                let (w, h) = (p(0) * us * 0.5, p(1) * us * 0.5);
                Aperture {
                    polys: vec![vec![[-w, -h], [w, -h], [w, h], [-w, h]]],
                    stroke_width: p(0).min(p(1)) * us,
                }
            }
            "O" => Aperture {
                polys: vec![obround_poly(p(0) * us, p(1) * us)],
                stroke_width: p(0).min(p(1)) * us,
            },
            "P" => {
                let n = (p(1) as usize).max(3);
                let r = p(0) * us * 0.5;
                let rot_deg = p(2);
                let poly = (0..n)
                    .map(|k| {
                        let a = rot_deg.to_radians() + TAU * k as f64 / n as f64;
                        [r * a.cos(), r * a.sin()]
                    })
                    .collect();
                Aperture { polys: vec![poly], stroke_width: p(0) * us }
            }
            other => {
                if let Some(m) = self.macros.get(other).cloned() {
                    let polys = self.macro_polys(&m, &params);
                    Aperture { polys, stroke_width: 0.0 }
                } else {
                    self.notes.push(format!("Unknown aperture/macro '{other}' (D{code})"));
                    return;
                }
            }
        };
        self.apertures.insert(code, ap);
    }

    fn macro_polys(&mut self, lines: &[String], args: &[f64]) -> Vec<Vec<P>> {
        let mut vars: HashMap<u32, f64> = HashMap::new();
        for (i, a) in args.iter().enumerate() {
            vars.insert(i as u32 + 1, *a);
        }
        let us = self.unit_scale;
        let mut out: Vec<Vec<P>> = Vec::new();
        for line in lines {
            let line = line.trim();
            if line.starts_with('$') {
                if let Some((lhs, rhs)) = line.split_once('=') {
                    if let Ok(n) = lhs[1..].trim().parse::<u32>() {
                        let v = eval(rhs, &vars);
                        vars.insert(n, v);
                    }
                }
                continue;
            }
            let mut parts = line.split(',');
            let code = parts.next().unwrap_or("").trim();
            if code.starts_with('0') && code.len() > 1 {
                continue; // comment ("0 text")
            }
            let Ok(code) = code.parse::<i32>() else { continue };
            let v: Vec<f64> = parts.map(|s| eval(s.trim(), &vars)).collect();
            let g = |i: usize| v.get(i).copied().unwrap_or(0.0);
            if matches!(code, 1 | 20 | 21 | 4 | 5) && g(0) == 0.0 {
                self.skipped_clear = true; // negative exposure → cannot subtract
                continue;
            }
            let mut poly: Vec<P>;
            let rotation;
            match code {
                1 => {
                    poly = circle_poly([g(2), g(3)], g(1) * 0.5);
                    rotation = g(4);
                }
                20 => {
                    let (a, b) = ([g(2), g(3)], [g(4), g(5)]);
                    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                    let len = (dx * dx + dy * dy).sqrt().max(1e-12);
                    let (nx, ny) = (-dy / len * g(1) * 0.5, dx / len * g(1) * 0.5);
                    poly = vec![
                        [a[0] + nx, a[1] + ny],
                        [b[0] + nx, b[1] + ny],
                        [b[0] - nx, b[1] - ny],
                        [a[0] - nx, a[1] - ny],
                    ];
                    rotation = g(6);
                }
                21 => {
                    let (hw, hh, cx, cy) = (g(1) * 0.5, g(2) * 0.5, g(3), g(4));
                    poly = vec![[cx - hw, cy - hh], [cx + hw, cy - hh], [cx + hw, cy + hh], [cx - hw, cy + hh]];
                    rotation = g(5);
                }
                4 => {
                    let n = g(1) as usize;
                    poly = (0..=n).map(|i| [g(2 + 2 * i), g(3 + 2 * i)]).collect();
                    rotation = g(4 + 2 * n);
                }
                5 => {
                    let n = (g(1) as usize).max(3);
                    let (cx, cy, r) = (g(2), g(3), g(4) * 0.5);
                    poly = (0..n)
                        .map(|k| {
                            let a = TAU * k as f64 / n as f64;
                            [cx + r * a.cos(), cy + r * a.sin()]
                        })
                        .collect();
                    rotation = g(5);
                }
                6 | 7 => {
                    self.unsupported.push("Moire/Thermal macro primitives".into());
                    continue;
                }
                _ => continue,
            }
            for p in poly.iter_mut() {
                let r = rot(*p, rotation);
                *p = [r[0] * us, r[1] * us];
            }
            out.push(poly);
        }
        out
    }

    fn handle_word(&mut self, w: &str) {
        if w.starts_with("G04") {
            return; // comment
        }
        let mut x = None;
        let mut y = None;
        let mut i = None;
        let mut j = None;
        let mut d: Option<i32> = None;
        for (letter, val) in split_word(w) {
            match letter {
                'G' => match val.parse::<i32>().unwrap_or(-1) {
                    1 => self.interp = Interp::Linear,
                    2 => self.interp = Interp::Cw,
                    3 => self.interp = Interp::Ccw,
                    36 => {
                        self.region = true;
                        self.contour.clear();
                    }
                    37 => {
                        self.close_contour();
                        self.region = false;
                    }
                    74 => self.multi_quadrant = false,
                    75 => self.multi_quadrant = true,
                    _ => {}
                },
                'X' => x = Some(self.coord(&val)),
                'Y' => y = Some(self.coord(&val)),
                'I' => i = Some(self.coord(&val)),
                'J' => j = Some(self.coord(&val)),
                'D' => d = val.parse::<i32>().ok(),
                _ => {}
            }
        }

        if let Some(code) = d {
            if code >= 10 {
                self.cur_ap = code;
                if x.is_none() && y.is_none() {
                    return;
                }
            }
        }
        if x.is_none() && y.is_none() && i.is_none() && j.is_none() {
            return;
        }
        let op = match d {
            Some(c) if c < 10 => {
                self.last_d = c;
                c
            }
            _ => self.last_d,
        };
        let np = [x.unwrap_or(self.cur[0]), y.unwrap_or(self.cur[1])];
        match op {
            1 => self.draw(np, i.unwrap_or(0.0), j.unwrap_or(0.0)),
            2 => {
                if self.region {
                    self.close_contour();
                }
                self.cur = np;
            }
            3 => {
                self.flash(np);
            }
            _ => {}
        }
    }

    fn arc(&self, s: P, e: P, i: f64, j: f64) -> Vec<P> {
        let ccw = self.interp == Interp::Ccw;
        let dist = |a: P, b: P| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        let center = if self.multi_quadrant {
            [s[0] + i, s[1] + j]
        } else {
            if dist(s, e) < 1e-9 {
                return vec![s, e];
            }
            let mut best: Option<(f64, P)> = None;
            for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                let c = [s[0] + sx * i.abs(), s[1] + sy * j.abs()];
                let sw = arc_sweep(s, e, c, ccw, false);
                let mut score = (dist(c, s) - dist(c, e)).abs();
                if sw.abs() > PI / 2.0 + 1e-3 {
                    score += 1e6;
                }
                if best.map_or(true, |(b, _)| score < b) {
                    best = Some((score, c));
                }
            }
            best.unwrap().1
        };
        let r = dist(s, center);
        if r < 1e-9 {
            return vec![s, e];
        }
        let a0 = (s[1] - center[1]).atan2(s[0] - center[0]);
        let sweep = arc_sweep(s, e, center, ccw, self.multi_quadrant);
        let mut pts = arc_points(center, r, a0, sweep);
        // snap exact endpoints
        if let Some(first) = pts.first_mut() {
            *first = s;
        }
        if let Some(last) = pts.last_mut() {
            *last = e;
        }
        pts
    }

    fn draw(&mut self, np: P, i: f64, j: f64) {
        let pts: Vec<P> = if self.interp == Interp::Linear {
            vec![self.cur, np]
        } else {
            self.arc(self.cur, np, i, j)
        };
        if self.region {
            if self.contour.is_empty() {
                self.contour.push(self.cur);
            }
            self.contour.extend_from_slice(&pts[1..]);
        } else if self.dark {
            let w = self.apertures.get(&self.cur_ap).map_or(0.0, |a| a.stroke_width);
            self.geom.add_stroke(&pts, w);
        }
        self.cur = np;
    }

    fn flash(&mut self, np: P) {
        if !self.region && self.dark {
            if let Some(ap) = self.apertures.get(&self.cur_ap) {
                for poly in &ap.polys {
                    let moved: Vec<P> = poly.iter().map(|p| [p[0] + np[0], p[1] + np[1]]).collect();
                    self.geom.add_polygon(&moved);
                }
            }
        }
        self.cur = np;
    }

    fn close_contour(&mut self) {
        if self.contour.len() >= 3 && self.dark {
            let c = std::mem::take(&mut self.contour);
            self.geom.add_polygon(&c);
        }
        self.contour.clear();
    }
}

pub fn parse(source: &str) -> anyhow::Result<ParseOutput> {
    let mut p = Parser::new();
    for item in tokenize(source) {
        match item {
            Item::Ext(blocks) => p.handle_ext(&blocks),
            Item::Word(w) => {
                if w.starts_with("M02") || w.starts_with("M00") {
                    break;
                }
                p.handle_word(&w);
            }
        }
    }
    p.close_contour();

    if p.geom.is_empty() {
        anyhow::bail!("Gerber file produced no geometry (is it a valid RS-274X file?)");
    }
    let mut notes = std::mem::take(&mut p.notes);
    if p.skipped_clear {
        notes.push("Clear-polarity / negative-exposure objects were skipped".into());
    }
    p.unsupported.sort();
    p.unsupported.dedup();
    for u in &p.unsupported {
        notes.push(format!("Unsupported Gerber feature ignored: {u}"));
    }
    Ok(ParseOutput { geometry: p.geom, notes })
}
