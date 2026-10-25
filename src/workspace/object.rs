// object.rs — parsed geometry objects shared by every importer.
//
// All coordinates are stored in millimetres (world space, Y up).
// Filled shapes are tessellated into a triangle list; open/closed outlines
// are stored as a line list (pairs of vertices).
use serde::{Deserialize, Serialize};

/// A 2D point in f64 used while parsing (converted to f32 for storage / GPU).
pub type P = [f64; 2];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DrillHit {
    pub x: f64,
    pub y: f64,
    pub diameter: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Geometry {
    /// Triangle list (3 vertices per triangle)
    pub triangles: Vec<[f32; 2]>,
    /// Line list (2 vertices per segment)
    pub lines: Vec<[f32; 2]>,
    /// Drill hits (also tessellated into `triangles` for display)
    pub drills: Vec<DrillHit>,
}

fn f(p: P) -> [f32; 2] {
    [p[0] as f32, p[1] as f32]
}

fn cross(a: P, b: P, c: P) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn signed_area(v: &[P]) -> f64 {
    let n = v.len();
    let mut s = 0.0;
    for i in 0..n {
        let a = v[i];
        let b = v[(i + 1) % n];
        s += a[0] * b[1] - b[0] * a[1];
    }
    s * 0.5
}

fn point_in_tri(p: P, a: P, b: P, c: P) -> bool {
    let eps = 1e-12;
    let same = |u: P, v: P| (u[0] - v[0]).abs() < 1e-12 && (u[1] - v[1]).abs() < 1e-12;
    if same(p, a) || same(p, b) || same(p, c) {
        return false; // duplicated vertex (e.g. bridged contours)
    }
    // Inclusive: a vertex lying on an edge also blocks the ear.
    cross(a, b, p) >= -eps && cross(b, c, p) >= -eps && cross(c, a, p) >= -eps
}

/// Points on an arc (inclusive of both ends). `sweep` is signed (CCW positive), radians.
pub fn arc_points(center: P, r: f64, a0: f64, sweep: f64) -> Vec<P> {
    let tol = 0.005_f64; // mm chord error
    let step = if r > tol {
        (2.0 * (1.0 - tol / r).acos()).clamp(1.0_f64.to_radians(), 15.0_f64.to_radians())
    } else {
        15.0_f64.to_radians()
    };
    let n = ((sweep.abs() / step).ceil() as usize).max(2);
    (0..=n)
        .map(|i| {
            let a = a0 + sweep * (i as f64 / n as f64);
            [center[0] + r * a.cos(), center[1] + r * a.sin()]
        })
        .collect()
}

/// Closed circle polygon (no duplicated end point).
pub fn circle_poly(c: P, r: f64) -> Vec<P> {
    let mut pts = arc_points(c, r, 0.0, std::f64::consts::TAU);
    pts.pop();
    pts
}

impl Geometry {
    pub fn is_empty(&self) -> bool {
        self.triangles.is_empty() && self.lines.is_empty()
    }

    pub fn add_line(&mut self, a: P, b: P) {
        self.lines.push(f(a));
        self.lines.push(f(b));
    }

    pub fn add_polyline(&mut self, pts: &[P], closed: bool) {
        for w in pts.windows(2) {
            self.add_line(w[0], w[1]);
        }
        if closed && pts.len() > 2 {
            self.add_line(pts[pts.len() - 1], pts[0]);
        }
    }

    pub fn add_triangle(&mut self, a: P, b: P, c: P) {
        self.triangles.push(f(a));
        self.triangles.push(f(b));
        self.triangles.push(f(c));
    }

    pub fn add_circle(&mut self, c: P, r: f64) {
        let poly = circle_poly(c, r);
        for i in 1..poly.len().saturating_sub(1) {
            self.add_triangle(poly[0], poly[i], poly[i + 1]);
        }
    }

    /// Stroke a polyline with round joints/caps (Gerber circular aperture draw).
    pub fn add_stroke(&mut self, pts: &[P], width: f64) {
        if pts.is_empty() {
            return;
        }
        if width <= 0.0 {
            self.add_polyline(pts, false);
            return;
        }
        let r = width * 0.5;
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-12 {
                continue;
            }
            let (nx, ny) = (-dy / len * r, dx / len * r);
            let (p0, p1) = ([a[0] + nx, a[1] + ny], [a[0] - nx, a[1] - ny]);
            let (p2, p3) = ([b[0] - nx, b[1] - ny], [b[0] + nx, b[1] + ny]);
            self.add_triangle(p0, p1, p2);
            self.add_triangle(p0, p2, p3);
        }
        for p in pts {
            self.add_circle(*p, r);
        }
    }

    /// Fill a simple polygon using ear clipping (orientation independent).
    pub fn add_polygon(&mut self, pts: &[P]) {
        let mut v: Vec<P> = Vec::with_capacity(pts.len());
        for p in pts {
            if v.last().map_or(true, |l| (l[0] - p[0]).abs() > 1e-12 || (l[1] - p[1]).abs() > 1e-12) {
                v.push(*p);
            }
        }
        if v.len() > 1 {
            let (first, last) = (v[0], v[v.len() - 1]);
            if (first[0] - last[0]).abs() < 1e-12 && (first[1] - last[1]).abs() < 1e-12 {
                v.pop();
            }
        }
        if v.len() < 3 {
            return;
        }
        if signed_area(&v) < 0.0 {
            v.reverse();
        }

        let mut idx: Vec<usize> = (0..v.len()).collect();
        let mut start = 0usize;
        while idx.len() > 3 {
            let m = idx.len();
            let mut clipped = false;
            for k in 0..m {
                let i = (start + k) % m;
                let (ip, ic, inx) = (idx[(i + m - 1) % m], idx[i], idx[(i + 1) % m]);
                let (a, b, c) = (v[ip], v[ic], v[inx]);
                if cross(a, b, c) <= 1e-12 {
                    continue; // reflex or degenerate
                }
                let blocked = idx
                    .iter()
                    .any(|&j| j != ip && j != ic && j != inx && point_in_tri(v[j], a, b, c));
                if blocked {
                    continue;
                }
                self.add_triangle(a, b, c);
                idx.remove(i);
                start = if i == 0 { 0 } else { i - 1 };
                clipped = true;
                break;
            }
            if !clipped {
                // Degenerate / self-touching: drop the flattest vertex and continue.
                let m = idx.len();
                let mut best = 0;
                let mut best_val = f64::MAX;
                for i in 0..m {
                    let c = cross(v[idx[(i + m - 1) % m]], v[idx[i]], v[idx[(i + 1) % m]]).abs();
                    if c < best_val {
                        best_val = c;
                        best = i;
                    }
                }
                idx.remove(best);
                start = 0;
            }
        }
        if idx.len() == 3 {
            self.add_triangle(v[idx[0]], v[idx[1]], v[idx[2]]);
        }
    }

    /// Axis-aligned bounds (min, max) over all vertices.
    pub fn bounds(&self) -> Option<([f32; 2], [f32; 2])> {
        let mut it = self.triangles.iter().chain(self.lines.iter());
        let first = it.next()?;
        let (mut mn, mut mx) = (*first, *first);
        for p in it {
            mn[0] = mn[0].min(p[0]);
            mn[1] = mn[1].min(p[1]);
            mx[0] = mx[0].max(p[0]);
            mx[1] = mx[1].max(p[1]);
        }
        Some((mn, mx))
    }
}
