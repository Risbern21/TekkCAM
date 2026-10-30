// dxf.rs — ASCII DXF parser (Phase 2)
//
// Supports LINE, CIRCLE, ARC, LWPOLYLINE (with bulges), POLYLINE/VERTEX,
// ELLIPSE, and SPLINE (control polygon approximation). Honors $INSUNITS
// (inches → mm). Output is outline geometry (line list).
use super::ParseOutput;
use crate::workspace::object::{arc_points, Geometry, P};
use std::f64::consts::TAU;

struct Entity {
    kind: String,
    data: Vec<(i32, String)>,
}

impl Entity {
    fn f(&self, code: i32) -> Option<f64> {
        self.data
            .iter()
            .find(|(c, _)| *c == code)
            .and_then(|(_, v)| v.parse().ok())
    }
    fn fd(&self, code: i32, default: f64) -> f64 {
        self.f(code).unwrap_or(default)
    }
}

/// Build a point list from (x, y, bulge) vertices, expanding bulge arcs.
fn with_bulges(verts: &[(f64, f64, f64)], closed: bool) -> Vec<P> {
    let n = verts.len();
    let mut out: Vec<P> = Vec::new();
    if n == 0 {
        return out;
    }
    let seg_count = if closed { n } else { n - 1 };
    for i in 0..seg_count {
        let (a, b) = (verts[i], verts[(i + 1) % n]);
        let (s, e) = ([a.0, a.1], [b.0, b.1]);
        let bulge = a.2;
        if bulge.abs() < 1e-9 {
            out.push(s);
            continue;
        }
        let (dx, dy) = (e[0] - s[0], e[1] - s[1]);
        let chord = (dx * dx + dy * dy).sqrt();
        if chord < 1e-12 {
            out.push(s);
            continue;
        }
        let mid = [(s[0] + e[0]) * 0.5, (s[1] + e[1]) * 0.5];
        let off = (chord * 0.5) * ((1.0 - bulge * bulge) / (2.0 * bulge));
        let (nx, ny) = (-dy / chord, dx / chord);
        let c = [mid[0] + nx * off, mid[1] + ny * off];
        let r = ((s[0] - c[0]).powi(2) + (s[1] - c[1]).powi(2)).sqrt();
        let a0 = (s[1] - c[1]).atan2(s[0] - c[0]);
        let sweep = 4.0 * bulge.atan();
        let mut pts = arc_points(c, r, a0, sweep);
        pts.pop(); // next segment pushes its own start
        out.extend(pts);
    }
    if closed {
        // closing segment's end is the first point (handled by closed flag)
    } else {
        out.push([verts[n - 1].0, verts[n - 1].1]);
    }
    out
}

pub fn parse(source: &str) -> anyhow::Result<ParseOutput> {
    let lines: Vec<&str> = source.lines().collect();
    let mut pairs: Vec<(i32, String)> = Vec::new();
    let mut i = 0;
    while i + 1 < lines.len() {
        if let Ok(code) = lines[i].trim().parse::<i32>() {
            pairs.push((code, lines[i + 1].trim().to_string()));
        }
        i += 2;
    }
    if pairs.is_empty() {
        anyhow::bail!("Not an ASCII DXF file (binary DXF is not supported)");
    }

    // Units
    let mut scale = 1.0;
    for w in pairs.windows(2) {
        if w[0].0 == 9 && w[0].1 == "$INSUNITS" && w[1].0 == 70 {
            match w[1].1.as_str() {
                "1" => scale = 25.4,
                "2" => scale = 304.8,
                "5" => scale = 10.0,
                "6" => scale = 1000.0,
                _ => {}
            }
        }
    }

    // Collect ENTITIES section
    let mut entities: Vec<Entity> = Vec::new();
    let mut in_entities = false;
    let mut k = 0;
    while k < pairs.len() {
        let (code, val) = &pairs[k];
        if *code == 0 && val == "SECTION" {
            in_entities = pairs
                .get(k + 1)
                .map_or(false, |p| p.0 == 2 && p.1 == "ENTITIES");
            k += 2;
            continue;
        }
        if *code == 0 && val == "ENDSEC" {
            in_entities = false;
        }
        if in_entities && *code == 0 {
            let mut e = Entity {
                kind: val.clone(),
                data: Vec::new(),
            };
            k += 1;
            while k < pairs.len() && pairs[k].0 != 0 {
                e.data.push(pairs[k].clone());
                k += 1;
            }
            entities.push(e);
            continue;
        }
        k += 1;
    }

    let mut geom = Geometry::default();
    let mut skipped: std::collections::BTreeMap<String, usize> = Default::default();
    let mut poly_verts: Option<(Vec<(f64, f64, f64)>, bool)> = None;

    let s = scale;
    let flush = |geom: &mut Geometry, pv: &mut Option<(Vec<(f64, f64, f64)>, bool)>| {
        if let Some((v, closed)) = pv.take() {
            let pts = with_bulges(&v, closed);
            let pts: Vec<P> = pts.iter().map(|p| [p[0] * s, p[1] * s]).collect();
            geom.add_polyline(&pts, closed);
        }
    };

    for e in &entities {
        match e.kind.as_str() {
            "LINE" => {
                let (a, b) = (
                    [e.fd(10, 0.0) * s, e.fd(20, 0.0) * s],
                    [e.fd(11, 0.0) * s, e.fd(21, 0.0) * s],
                );
                geom.add_line(a, b);
            }
            "CIRCLE" => {
                let c = [e.fd(10, 0.0) * s, e.fd(20, 0.0) * s];
                let pts = arc_points(c, e.fd(40, 0.0) * s, 0.0, TAU);
                geom.add_polyline(&pts, false);
            }
            "ARC" => {
                let c = [e.fd(10, 0.0) * s, e.fd(20, 0.0) * s];
                let a0 = e.fd(50, 0.0);
                let mut a1 = e.fd(51, 360.0);
                if a1 <= a0 {
                    a1 += 360.0;
                }
                let pts = arc_points(
                    c,
                    e.fd(40, 0.0) * s,
                    a0.to_radians(),
                    (a1 - a0).to_radians(),
                );
                geom.add_polyline(&pts, false);
            }
            "LWPOLYLINE" => {
                let mut verts: Vec<(f64, f64, f64)> = Vec::new();
                let mut closed = false;
                for (c, v) in &e.data {
                    match c {
                        10 => verts.push((v.parse().unwrap_or(0.0), 0.0, 0.0)),
                        20 => {
                            if let Some(l) = verts.last_mut() {
                                l.1 = v.parse().unwrap_or(0.0);
                            }
                        }
                        42 => {
                            if let Some(l) = verts.last_mut() {
                                l.2 = v.parse().unwrap_or(0.0);
                            }
                        }
                        70 => closed = v.parse::<i32>().unwrap_or(0) & 1 == 1,
                        _ => {}
                    }
                }
                let mut pv = Some((verts, closed));
                flush(&mut geom, &mut pv);
            }
            "POLYLINE" => {
                flush(&mut geom, &mut poly_verts);
                let closed = e.fd(70, 0.0) as i32 & 1 == 1;
                poly_verts = Some((Vec::new(), closed));
            }
            "VERTEX" => {
                if let Some((v, _)) = poly_verts.as_mut() {
                    v.push((e.fd(10, 0.0), e.fd(20, 0.0), e.fd(42, 0.0)));
                }
            }
            "SEQEND" => flush(&mut geom, &mut poly_verts),
            "ELLIPSE" => {
                let c = [e.fd(10, 0.0), e.fd(20, 0.0)];
                let major = [e.fd(11, 1.0), e.fd(21, 0.0)];
                let ratio = e.fd(40, 1.0);
                let (t0, mut t1) = (e.fd(41, 0.0), e.fd(42, TAU));
                if t1 <= t0 {
                    t1 += TAU;
                }
                let minor = [-major[1] * ratio, major[0] * ratio];
                let n = 96;
                let pts: Vec<P> = (0..=n)
                    .map(|i| {
                        let t = t0 + (t1 - t0) * i as f64 / n as f64;
                        [
                            (c[0] + major[0] * t.cos() + minor[0] * t.sin()) * s,
                            (c[1] + major[1] * t.cos() + minor[1] * t.sin()) * s,
                        ]
                    })
                    .collect();
                geom.add_polyline(&pts, false);
            }
            "SPLINE" => {
                let mut pts: Vec<P> = Vec::new();
                for (c, v) in &e.data {
                    match c {
                        10 => pts.push([v.parse::<f64>().unwrap_or(0.0) * s, 0.0]),
                        20 => {
                            if let Some(l) = pts.last_mut() {
                                l[1] = v.parse::<f64>().unwrap_or(0.0) * s;
                            }
                        }
                        _ => {}
                    }
                }
                geom.add_polyline(&pts, false);
                *skipped
                    .entry("SPLINE (drawn as control polygon)".into())
                    .or_default() += 1;
            }
            other => {
                *skipped.entry(other.to_string()).or_default() += 1;
            }
        }
    }
    flush(&mut geom, &mut poly_verts);

    if geom.is_empty() {
        anyhow::bail!("DXF contained no supported entities");
    }
    let notes = skipped
        .into_iter()
        .map(|(k, n)| format!("Skipped/approximated {n}× {k}"))
        .collect();
    Ok(ParseOutput {
        geometry: geom,
        notes,
    })
}
