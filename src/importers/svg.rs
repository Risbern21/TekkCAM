// svg.rs — SVG parser (Phase 2)
//
// Supports <path> (all commands incl. arcs/curves), <line>, <rect>, <circle>,
// <ellipse>, <polyline>, <polygon>. Output is outline geometry in mm with Y up.
// `transform` attributes are not applied (a note is emitted).
use std::collections::HashMap;

use super::ParseOutput;
use crate::workspace::object::{Geometry, P};
use std::f64::consts::TAU;

type SubPath = (Vec<P>, bool);

fn parse_attrs(tag: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let b: Vec<char> = tag.chars().collect();
    let mut i = 0;
    // skip tag name
    while i < b.len() && !b[i].is_whitespace() {
        i += 1;
    }
    while i < b.len() {
        while i < b.len() && (b[i].is_whitespace() || b[i] == '/') {
            i += 1;
        }
        let ks = i;
        while i < b.len() && b[i] != '=' && !b[i].is_whitespace() && b[i] != '/' {
            i += 1;
        }
        let key: String = b[ks..i].iter().collect();
        while i < b.len() && b[i].is_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == '=' {
            i += 1;
            while i < b.len() && b[i].is_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == '"' || b[i] == '\'') {
                let q = b[i];
                i += 1;
                let vs = i;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                let val: String = b[vs..i].iter().collect();
                i += 1;
                if !key.is_empty() {
                    map.insert(key, val);
                }
            }
        } else if i == ks {
            i += 1;
        }
    }
    map
}

fn num(a: &HashMap<String, String>, k: &str) -> f64 {
    a.get(k)
        .map(|s| {
            s.trim_end_matches(|c: char| c.is_alphabetic() || c == '%')
                .trim()
        })
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0)
}

fn unit_factor(s: &str) -> f64 {
    let s = s.trim();
    if s.ends_with("mm") {
        1.0
    } else if s.ends_with("cm") {
        10.0
    } else if s.ends_with("in") {
        25.4
    } else if s.ends_with("pt") {
        25.4 / 72.0
    } else {
        25.4 / 96.0
    }
}

struct PathScanner {
    c: Vec<char>,
    pos: usize,
}

impl PathScanner {
    fn skip(&mut self) {
        while self.pos < self.c.len()
            && (self.c[self.pos].is_whitespace() || self.c[self.pos] == ',')
        {
            self.pos += 1;
        }
    }
    fn eof(&mut self) -> bool {
        self.skip();
        self.pos >= self.c.len()
    }
    fn peek_cmd(&mut self) -> Option<char> {
        self.skip();
        self.c
            .get(self.pos)
            .copied()
            .filter(|c| c.is_ascii_alphabetic())
    }
    fn has_num(&mut self) -> bool {
        self.skip();
        self.c.get(self.pos).map_or(false, |c| {
            c.is_ascii_digit() || *c == '-' || *c == '+' || *c == '.'
        })
    }
    fn num(&mut self) -> f64 {
        self.skip();
        let st = self.pos;
        if matches!(self.c.get(self.pos), Some('-') | Some('+')) {
            self.pos += 1;
        }
        let mut seen_dot = false;
        while let Some(&ch) = self.c.get(self.pos) {
            if ch.is_ascii_digit() {
                self.pos += 1;
            } else if ch == '.' && !seen_dot {
                seen_dot = true;
                self.pos += 1;
            } else if (ch == 'e' || ch == 'E')
                && self
                    .c
                    .get(self.pos + 1)
                    .map_or(false, |n| n.is_ascii_digit() || *n == '-' || *n == '+')
            {
                self.pos += 2;
            } else {
                break;
            }
        }
        let s: String = self.c[st..self.pos].iter().collect();
        s.parse().unwrap_or(0.0)
    }
    fn flag(&mut self) -> bool {
        self.skip();
        let v = self.c.get(self.pos).copied() == Some('1');
        self.pos += 1;
        v
    }
}

fn cubic(p0: P, p1: P, p2: P, p3: P, out: &mut Vec<P>) {
    let n = 20;
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let u = 1.0 - t;
        out.push([
            u * u * u * p0[0]
                + 3.0 * u * u * t * p1[0]
                + 3.0 * u * t * t * p2[0]
                + t * t * t * p3[0],
            u * u * u * p0[1]
                + 3.0 * u * u * t * p1[1]
                + 3.0 * u * t * t * p2[1]
                + t * t * t * p3[1],
        ]);
    }
}

fn quad(p0: P, p1: P, p2: P, out: &mut Vec<P>) {
    let n = 16;
    for i in 1..=n {
        let t = i as f64 / n as f64;
        let u = 1.0 - t;
        out.push([
            u * u * p0[0] + 2.0 * u * t * p1[0] + t * t * p2[0],
            u * u * p0[1] + 2.0 * u * t * p1[1] + t * t * p2[1],
        ]);
    }
}

/// SVG endpoint arc → points (spec F.6.5)
fn arc_to(
    from: P,
    to: P,
    mut rx: f64,
    mut ry: f64,
    phi_deg: f64,
    fa: bool,
    fs: bool,
    out: &mut Vec<P>,
) {
    if (from[0] - to[0]).abs() < 1e-12 && (from[1] - to[1]).abs() < 1e-12 {
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    if rx < 1e-12 || ry < 1e-12 {
        out.push(to);
        return;
    }
    let phi = phi_deg.to_radians();
    let (sp, cp) = phi.sin_cos();
    let dx2 = (from[0] - to[0]) * 0.5;
    let dy2 = (from[1] - to[1]) * 0.5;
    let x1p = cp * dx2 + sp * dy2;
    let y1p = -sp * dx2 + cp * dy2;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut co = if den.abs() < 1e-18 {
        0.0
    } else {
        (num / den).max(0.0).sqrt()
    };
    if fa == fs {
        co = -co;
    }
    let cxp = co * rx * y1p / ry;
    let cyp = -co * ry * x1p / rx;
    let cx = cp * cxp - sp * cyp + (from[0] + to[0]) * 0.5;
    let cy = sp * cxp + cp * cyp + (from[1] + to[1]) * 0.5;
    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        a
    };
    let th1 = ang(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dth = ang(
        (x1p - cxp) / rx,
        (y1p - cyp) / ry,
        (-x1p - cxp) / rx,
        (-y1p - cyp) / ry,
    );
    if !fs && dth > 0.0 {
        dth -= TAU;
    } else if fs && dth < 0.0 {
        dth += TAU;
    }
    let n = ((dth.abs() / 0.1).ceil() as usize).max(2);
    for i in 1..=n {
        let t = th1 + dth * i as f64 / n as f64;
        let (x, y) = (rx * t.cos(), ry * t.sin());
        out.push([cp * x - sp * y + cx, sp * x + cp * y + cy]);
    }
}

fn parse_path(d: &str) -> Vec<SubPath> {
    let mut sc = PathScanner {
        c: d.chars().collect(),
        pos: 0,
    };
    let mut subs: Vec<SubPath> = Vec::new();
    let mut pts: Vec<P> = Vec::new();
    let mut cur: P = [0.0, 0.0];
    let mut start: P = [0.0, 0.0];
    let mut cmd = ' ';
    let mut last_c2: Option<P> = None; // last cubic control for S
    let mut last_q: Option<P> = None; // last quad control for T

    macro_rules! finish {
        ($closed:expr) => {
            if pts.len() > 1 {
                subs.push((std::mem::take(&mut pts), $closed));
            } else {
                pts.clear();
            }
        };
    }

    while !sc.eof() {
        if let Some(c) = sc.peek_cmd() {
            cmd = c;
            sc.pos += 1;
            if cmd == 'z' || cmd == 'Z' {
                finish!(true);
                cur = start;
                last_c2 = None;
                last_q = None;
                continue;
            }
        } else if !sc.has_num() {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cur[0], cur[1]) } else { (0.0, 0.0) };
        let upper = cmd.to_ascii_uppercase();
        let mut new_c2 = None;
        let mut new_q = None;
        match upper {
            'M' => {
                let p = [ox + sc.num(), oy + sc.num()];
                finish!(false);
                pts.push(p);
                cur = p;
                start = p;
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' => {
                let p = [ox + sc.num(), oy + sc.num()];
                if pts.is_empty() {
                    pts.push(cur);
                }
                pts.push(p);
                cur = p;
            }
            'H' => {
                let p = [if rel { cur[0] + sc.num() } else { sc.num() }, cur[1]];
                if pts.is_empty() {
                    pts.push(cur);
                }
                pts.push(p);
                cur = p;
            }
            'V' => {
                let p = [cur[0], if rel { cur[1] + sc.num() } else { sc.num() }];
                if pts.is_empty() {
                    pts.push(cur);
                }
                pts.push(p);
                cur = p;
            }
            'C' => {
                let p1 = [ox + sc.num(), oy + sc.num()];
                let p2 = [ox + sc.num(), oy + sc.num()];
                let p3 = [ox + sc.num(), oy + sc.num()];
                if pts.is_empty() {
                    pts.push(cur);
                }
                cubic(cur, p1, p2, p3, &mut pts);
                new_c2 = Some(p2);
                cur = p3;
            }
            'S' => {
                let p2 = [ox + sc.num(), oy + sc.num()];
                let p3 = [ox + sc.num(), oy + sc.num()];
                let p1 = last_c2.map_or(cur, |c| [2.0 * cur[0] - c[0], 2.0 * cur[1] - c[1]]);
                if pts.is_empty() {
                    pts.push(cur);
                }
                cubic(cur, p1, p2, p3, &mut pts);
                new_c2 = Some(p2);
                cur = p3;
            }
            'Q' => {
                let p1 = [ox + sc.num(), oy + sc.num()];
                let p2 = [ox + sc.num(), oy + sc.num()];
                if pts.is_empty() {
                    pts.push(cur);
                }
                quad(cur, p1, p2, &mut pts);
                new_q = Some(p1);
                cur = p2;
            }
            'T' => {
                let p2 = [ox + sc.num(), oy + sc.num()];
                let p1 = last_q.map_or(cur, |c| [2.0 * cur[0] - c[0], 2.0 * cur[1] - c[1]]);
                if pts.is_empty() {
                    pts.push(cur);
                }
                quad(cur, p1, p2, &mut pts);
                new_q = Some(p1);
                cur = p2;
            }
            'A' => {
                let (rx, ry, rotn) = (sc.num(), sc.num(), sc.num());
                let (fa, fs) = (sc.flag(), sc.flag());
                let p = [ox + sc.num(), oy + sc.num()];
                if pts.is_empty() {
                    pts.push(cur);
                }
                arc_to(cur, p, rx, ry, rotn, fa, fs, &mut pts);
                cur = p;
            }
            _ => break,
        }
        last_c2 = new_c2;
        last_q = new_q;
    }
    finish!(false);
    subs
}

pub fn parse(source: &str) -> anyhow::Result<ParseOutput> {
    let mut shapes: Vec<SubPath> = Vec::new();
    let mut notes = Vec::new();
    let mut scale = 25.4 / 96.0;
    let (mut vbx, mut vby) = (0.0, 0.0);
    let mut saw_transform = false;
    let mut saw_svg = false;

    let mut rest = source;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[..gt];
        rest = &rest[gt + 1..];
        if tag.starts_with('/') || tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        let name: String = tag
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '/')
            .collect();
        let a = parse_attrs(tag);
        if a.contains_key("transform") {
            saw_transform = true;
        }
        match name.as_str() {
            "svg" if !saw_svg => {
                saw_svg = true;
                if let Some(vb) = a.get("viewBox") {
                    let v: Vec<f64> = vb
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    if v.len() == 4 && v[2] > 0.0 {
                        vbx = v[0];
                        vby = v[1];
                        if let Some(w) = a.get("width") {
                            scale = num(&a, "width") * unit_factor(w) / v[2];
                        } else {
                            scale = 25.4 / 96.0;
                        }
                    }
                }
            }
            "path" => {
                if let Some(d) = a.get("d") {
                    shapes.extend(parse_path(d));
                }
            }
            "line" => shapes.push((
                vec![
                    [num(&a, "x1"), num(&a, "y1")],
                    [num(&a, "x2"), num(&a, "y2")],
                ],
                false,
            )),
            "rect" => {
                let (x, y, w, h) = (
                    num(&a, "x"),
                    num(&a, "y"),
                    num(&a, "width"),
                    num(&a, "height"),
                );
                shapes.push((vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]], true));
            }
            "circle" => {
                let (cx, cy, r) = (num(&a, "cx"), num(&a, "cy"), num(&a, "r"));
                let mut p = Vec::new();
                arc_to([cx + r, cy], [cx - r, cy], r, r, 0.0, false, true, &mut p);
                arc_to([cx - r, cy], [cx + r, cy], r, r, 0.0, false, true, &mut p);
                p.insert(0, [cx + r, cy]);
                shapes.push((p, true));
            }
            "ellipse" => {
                let (cx, cy, rx, ry) = (num(&a, "cx"), num(&a, "cy"), num(&a, "rx"), num(&a, "ry"));
                let p: Vec<P> = (0..64)
                    .map(|i| {
                        let t = TAU * i as f64 / 64.0;
                        [cx + rx * t.cos(), cy + ry * t.sin()]
                    })
                    .collect();
                shapes.push((p, true));
            }
            "polyline" | "polygon" => {
                if let Some(pts) = a.get("points") {
                    let n: Vec<f64> = pts
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    let p: Vec<P> = n.chunks_exact(2).map(|c| [c[0], c[1]]).collect();
                    shapes.push((p, name == "polygon"));
                }
            }
            _ => {}
        }
    }

    let mut geom = Geometry::default();
    for (pts, closed) in shapes {
        let world: Vec<P> = pts
            .iter()
            .map(|p| [(p[0] - vbx) * scale, -(p[1] - vby) * scale])
            .collect();
        geom.add_polyline(&world, closed);
    }
    if geom.is_empty() {
        anyhow::bail!("SVG contained no supported shapes");
    }
    if saw_transform {
        notes.push("SVG 'transform' attributes are not applied".into());
    }
    notes.push("SVG shapes imported as outlines (fills ignored)".into());
    Ok(ParseOutput {
        geometry: geom,
        notes,
    })
}
