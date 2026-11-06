//! Contour chaining, offsetting and ramp-entry helpers for 2D profile operations.
//!
//! Fusion 360 exposes a `chainingTolerance` that links loose edges into
//! contours, plus `doRamp`/`rampAngle` entry moves. These are independent
//! implementations of those ideas (see docs/FUSION_CAM_EVIDENCE.md).

use crate::cam::toolpath::Point;
use std::collections::HashMap;

/// Default distance below which two segment endpoints are treated as joined.
pub const DEFAULT_CHAIN_TOLERANCE_MM: f64 = 1e-3;

#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    /// Vertices in travel order. For a closed chain the first vertex is not
    /// repeated at the end.
    pub points: Vec<Point>,
    pub closed: bool,
}

type Cell = (i64, i64);

fn cell_of(p: Point, size: f64) -> Cell {
    ((p[0] / size).floor() as i64, (p[1] / size).floor() as i64)
}

fn dist(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Link a flat segment list (two vertices per segment) into polylines.
///
/// The first unused segment of each chain fixes its direction; later segments
/// are flipped as needed so the whole chain has a consistent travel direction.
/// Degenerate segments (shorter than `tolerance_mm`) are dropped.
pub fn chain_segments(lines: &[[f32; 2]], tolerance_mm: f64) -> Vec<Chain> {
    let tol = if tolerance_mm.is_finite() && tolerance_mm > 0.0 {
        tolerance_mm
    } else {
        DEFAULT_CHAIN_TOLERANCE_MM
    };
    let segments: Vec<(Point, Point)> = lines
        .chunks_exact(2)
        .map(|s| {
            (
                [s[0][0] as f64, s[0][1] as f64],
                [s[1][0] as f64, s[1][1] as f64],
            )
        })
        .filter(|(a, b)| dist(*a, *b) >= tol)
        .collect();

    // Spatial hash of endpoints: cell -> (segment index, which end).
    let mut grid: HashMap<Cell, Vec<(usize, bool)>> = HashMap::new();
    for (index, (a, b)) in segments.iter().enumerate() {
        grid.entry(cell_of(*a, tol))
            .or_default()
            .push((index, false));
        grid.entry(cell_of(*b, tol))
            .or_default()
            .push((index, true));
    }
    let mut used = vec![false; segments.len()];

    // Find an unused segment with an endpoint within tolerance of `p`.
    // Returns the far endpoint of that segment.
    let find = |p: Point, used: &[bool]| -> Option<(usize, Point)> {
        let (cx, cy) = cell_of(p, tol);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(entries) = grid.get(&(cx + dx, cy + dy)) else {
                    continue;
                };
                for &(index, is_end) in entries {
                    if used[index] {
                        continue;
                    }
                    let (a, b) = segments[index];
                    let (near, far) = if is_end { (b, a) } else { (a, b) };
                    if dist(near, p) <= tol {
                        return Some((index, far));
                    }
                }
            }
        }
        None
    };

    let mut chains = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (a, b) = segments[start];
        let mut points = vec![a, b];
        // Extend forward from the tail.
        while let Some((index, far)) = find(*points.last().unwrap(), &used) {
            used[index] = true;
            points.push(far);
        }
        // Extend backward from the head (insert in front, keeping direction).
        let mut front = Vec::new();
        let mut head = points[0];
        while let Some((index, far)) = find(head, &used) {
            used[index] = true;
            front.push(far);
            head = far;
        }
        if !front.is_empty() {
            front.reverse();
            front.extend(points);
            points = front;
        }
        let closed = points.len() >= 4 && dist(points[0], *points.last().unwrap()) <= tol;
        if closed {
            points.pop();
        }
        chains.push(Chain { points, closed });
    }
    chains
}

fn unit_left_normal(a: Point, b: Point) -> Point {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    [-dy / len, dx / len]
}

/// Offset a chain to the left of its travel direction by `distance` (a
/// negative distance offsets to the right). Interior corners are mitred; a
/// corner whose miter would exceed twice the offset is bevelled instead.
///
/// The returned polyline repeats the first vertex at the end for closed chains
/// so it can be cut as one continuous loop.
pub fn offset_chain(chain: &Chain, distance: f64) -> Vec<Point> {
    let pts = &chain.points;
    let n = pts.len();
    if n < 2 {
        return pts.clone();
    }
    if distance == 0.0 {
        let mut out = pts.clone();
        if chain.closed {
            out.push(pts[0]);
        }
        return out;
    }
    let normal_of = |i: usize| unit_left_normal(pts[i], pts[(i + 1) % n]);
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..n {
        let has_prev = chain.closed || i > 0;
        let has_next = chain.closed || i + 1 < n;
        let p = pts[i];
        match (has_prev, has_next) {
            (true, true) => {
                let n0 = normal_of((i + n - 1) % n);
                let n1 = normal_of(i);
                let denom = 1.0 + n0[0] * n1[0] + n0[1] * n1[1];
                if denom < 1e-6 {
                    // Reversal: bevel.
                    out.push([p[0] + n0[0] * distance, p[1] + n0[1] * distance]);
                    out.push([p[0] + n1[0] * distance, p[1] + n1[1] * distance]);
                    continue;
                }
                let m = [(n0[0] + n1[0]) / denom, (n0[1] + n1[1]) / denom];
                let miter_len = m[0].hypot(m[1]);
                if miter_len > 2.0 {
                    out.push([p[0] + n0[0] * distance, p[1] + n0[1] * distance]);
                    out.push([p[0] + n1[0] * distance, p[1] + n1[1] * distance]);
                } else {
                    out.push([p[0] + m[0] * distance, p[1] + m[1] * distance]);
                }
            }
            (false, true) => {
                let nn = normal_of(i);
                out.push([p[0] + nn[0] * distance, p[1] + nn[1] * distance]);
            }
            (true, false) => {
                let nn = normal_of(i - 1);
                out.push([p[0] + nn[0] * distance, p[1] + nn[1] * distance]);
            }
            (false, false) => unreachable!(),
        }
    }
    if chain.closed {
        out.push(out[0]);
    }
    out
}

pub fn polyline_length(points: &[Point]) -> f64 {
    points.windows(2).map(|w| dist(w[0], w[1])).sum()
}

/// Return the polyline cut off at arc length `length`, ending with an
/// interpolated vertex, together with the index of the first original vertex
/// beyond it. `length` must be within the polyline.
fn split_at_length(points: &[Point], length: f64) -> (Vec<Point>, usize) {
    let mut out = vec![points[0]];
    let mut travelled = 0.0;
    for i in 1..points.len() {
        let seg = dist(points[i - 1], points[i]);
        if travelled + seg >= length - 1e-12 {
            let t = if seg > 0.0 {
                ((length - travelled) / seg).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let a = points[i - 1];
            let b = points[i];
            let split = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            if dist(*out.last().unwrap(), split) > 1e-9 {
                out.push(split);
            }
            return (out, i);
        }
        travelled += seg;
        out.push(points[i]);
    }
    (out, points.len())
}

/// A single move of a ramped pass: XY target and Z, plus whether it is a ramp
/// move (descending) or a full-depth cut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PassMove {
    pub to: Point,
    pub z: f64,
    pub ramp: bool,
}

/// Build the moves for one depth pass along `points` (already offset; closed
/// loops repeat the first vertex at the end).
///
/// With `ramp_angle_deg <= 0`, or when the path is too short to ramp at that
/// angle, the pass plunges vertically: the caller's start move is expected to
/// be a plunge to `z_to`. Otherwise the pass descends from `z_from` to `z_to`
/// along the first `(z_from - z_to) / tan(angle)` of path length and then
/// re-cuts that section at full depth so no ramped material is left behind.
/// The returned moves exclude the starting vertex.
pub fn ramped_pass(
    points: &[Point],
    closed: bool,
    z_from: f64,
    z_to: f64,
    ramp_angle_deg: f64,
) -> Option<Vec<PassMove>> {
    if points.len() < 2 || !(ramp_angle_deg > 0.0) || z_from <= z_to {
        return None;
    }
    let ramp_len = (z_from - z_to) / ramp_angle_deg.to_radians().tan();
    let total = polyline_length(points);
    if !ramp_len.is_finite() || ramp_len > total {
        return None;
    }
    let (ramp_pts, next_index) = split_at_length(points, ramp_len);
    let mut moves = Vec::new();
    // Descend along the ramp section: z is linear in arc length.
    let mut travelled = 0.0;
    for window in ramp_pts.windows(2) {
        travelled += dist(window[0], window[1]);
        let z = z_from + (z_to - z_from) * (travelled / ramp_len).min(1.0);
        moves.push(PassMove {
            to: window[1],
            z,
            ramp: true,
        });
    }
    if let Some(last) = moves.last_mut() {
        last.z = z_to;
    }
    let ramp_end = *ramp_pts.last().unwrap();
    if closed {
        // Finish the loop at depth, then re-cut the ramp section.
        for p in &points[next_index.min(points.len())..] {
            moves.push(PassMove {
                to: *p,
                z: z_to,
                ramp: false,
            });
        }
        for p in ramp_pts.iter().skip(1) {
            moves.push(PassMove {
                to: *p,
                z: z_to,
                ramp: false,
            });
        }
    } else {
        // Return along the ramp section at depth, then cut the remainder.
        for p in ramp_pts.iter().rev().skip(1) {
            moves.push(PassMove {
                to: *p,
                z: z_to,
                ramp: false,
            });
        }
        moves.push(PassMove {
            to: ramp_end,
            z: z_to,
            ramp: false,
        });
        for p in &points[next_index.min(points.len())..] {
            moves.push(PassMove {
                to: *p,
                z: z_to,
                ramp: false,
            });
        }
    }
    Some(moves)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(a: [f32; 2], b: [f32; 2]) -> [[f32; 2]; 2] {
        [a, b]
    }

    #[test]
    fn chains_shuffled_square_into_one_closed_loop() {
        let mut lines = Vec::new();
        // Deliberately out of order and with flipped directions.
        lines.extend(seg([10.0, 0.0], [10.0, 10.0]));
        lines.extend(seg([0.0, 0.0], [10.0, 0.0]));
        lines.extend(seg([0.0, 10.0], [10.0, 10.0]));
        lines.extend(seg([0.0, 10.0], [0.0, 0.0]));
        let chains = chain_segments(&lines, 1e-3);
        assert_eq!(chains.len(), 1);
        assert!(chains[0].closed);
        assert_eq!(chains[0].points.len(), 4);
    }

    #[test]
    fn open_polyline_and_separate_segment_stay_separate() {
        let mut lines = Vec::new();
        lines.extend(seg([0.0, 0.0], [1.0, 0.0]));
        lines.extend(seg([1.0, 0.0005], [2.0, 0.0])); // within tolerance
        lines.extend(seg([50.0, 50.0], [51.0, 50.0]));
        let chains = chain_segments(&lines, 1e-3);
        assert_eq!(chains.len(), 2);
        let long = chains.iter().find(|c| c.points.len() == 3).unwrap();
        assert!(!long.closed);
    }

    #[test]
    fn drops_degenerate_segments() {
        let lines = [[1.0, 1.0], [1.0, 1.0]];
        assert!(chain_segments(&lines, 1e-3).is_empty());
    }

    #[test]
    fn left_offset_of_ccw_square_grows_it_inward_and_right_offset_outward() {
        let chain = Chain {
            points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            closed: true,
        };
        let inward = offset_chain(&chain, 1.0);
        assert!(inward
            .iter()
            .all(|p| p[0] >= 1.0 - 1e-9 && p[0] <= 9.0 + 1e-9));
        let outward = offset_chain(&chain, -1.0);
        assert!(outward.iter().any(|p| (p[0] + 1.0).abs() < 1e-9));
        assert!(outward.iter().any(|p| (p[1] - 11.0).abs() < 1e-9));
        // Mitred: exactly four corners plus the closing vertex.
        assert_eq!(outward.len(), 5);
        assert_eq!(outward[0], outward[4]);
    }

    #[test]
    fn sharp_reversal_is_bevelled_not_spiked() {
        let chain = Chain {
            points: vec![[0.0, 0.0], [10.0, 0.0], [0.0, 0.01]],
            closed: false,
        };
        let out = offset_chain(&chain, 1.0);
        assert!(out.iter().all(|p| p[0] < 12.0));
    }

    #[test]
    fn ramp_descends_over_requested_length_then_recuts_open_chain() {
        let pts = vec![[0.0, 0.0], [20.0, 0.0]];
        // 45 degrees: dropping 2 mm takes 2 mm of path.
        let moves = ramped_pass(&pts, false, 0.0, -2.0, 45.0).unwrap();
        assert!((moves[0].to[0] - 2.0).abs() < 1e-9);
        assert!((moves[0].z + 2.0).abs() < 1e-9);
        assert!(moves[0].ramp);
        assert!(moves[1..]
            .iter()
            .all(|m| (m.z + 2.0).abs() < 1e-9 && !m.ramp));
        assert_eq!(moves.last().unwrap().to, [20.0, 0.0]);
        // z never below target.
        assert!(moves.iter().all(|m| m.z >= -2.0 - 1e-9));
    }

    #[test]
    fn ramp_on_closed_loop_recuts_start_section_at_depth() {
        let pts = vec![
            [0.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
            [0.0, 0.0],
        ];
        let moves = ramped_pass(&pts, true, 0.0, -1.0, 45.0).unwrap();
        let last = moves.last().unwrap();
        assert_eq!(last.z, -1.0);
        assert!((last.to[0] - 1.0).abs() < 1e-9 && last.to[1].abs() < 1e-9);
    }

    #[test]
    fn ramp_falls_back_when_path_is_too_short() {
        let pts = vec![[0.0, 0.0], [1.0, 0.0]];
        assert!(ramped_pass(&pts, false, 0.0, -5.0, 10.0).is_none());
        assert!(ramped_pass(&pts, false, 0.0, -0.1, 0.0).is_none());
    }
}
