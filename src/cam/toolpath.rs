use serde::{Deserialize, Serialize};

pub type Point = [f64; 2];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MoveKind {
    Rapid,
    Feed,
    Arc,
    Drill,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolMove {
    Rapid {
        to: Point,
        z: f64,
    },
    Feed {
        to: Point,
        z: f64,
        feed_mm_min: f64,
    },
    Arc {
        to: Point,
        center_offset: Point,
        z: f64,
        clockwise: bool,
        feed_mm_min: f64,
    },
    Drill {
        at: Point,
        depth: f64,
        retract: f64,
        plunge_mm_min: f64,
    },
}

impl ToolMove {
    pub fn kind(&self) -> MoveKind {
        match self {
            Self::Rapid { .. } => MoveKind::Rapid,
            Self::Feed { .. } => MoveKind::Feed,
            Self::Arc { .. } => MoveKind::Arc,
            Self::Drill { .. } => MoveKind::Drill,
        }
    }
    pub fn end(&self) -> Point {
        match self {
            Self::Rapid { to, .. } | Self::Feed { to, .. } | Self::Arc { to, .. } => *to,
            Self::Drill { at, .. } => *at,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Toolpath {
    pub moves: Vec<ToolMove>,
}

impl Toolpath {
    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }
    pub fn push_rapid(&mut self, to: Point, z: f64) {
        self.moves.push(ToolMove::Rapid { to, z });
    }
    pub fn push_feed(&mut self, to: Point, z: f64, feed_mm_min: f64) {
        self.moves.push(ToolMove::Feed { to, z, feed_mm_min });
    }

    /// Simplify each contiguous feed run with a 3D Ramer-Douglas-Peucker
    /// bound. Rapids and operation boundaries are preserved exactly.
    pub fn simplify_feed_runs(&mut self, tolerance_mm: f64) -> Result<(), String> {
        if !tolerance_mm.is_finite() || tolerance_mm < 0.0 {
            return Err("path simplification tolerance must be non-negative".into());
        }
        if tolerance_mm == 0.0 || self.moves.len() < 3 {
            return Ok(());
        }
        let mut simplified = Vec::with_capacity(self.moves.len());
        let mut index = 0;
        while index < self.moves.len() {
            if !matches!(self.moves[index], ToolMove::Feed { .. }) {
                simplified.push(self.moves[index].clone());
                index += 1;
                continue;
            }
            let start = index;
            while index < self.moves.len() && matches!(self.moves[index], ToolMove::Feed { .. }) {
                index += 1;
            }
            let run = &self.moves[start..index];
            let points: Vec<[f64; 3]> = run
                .iter()
                .map(|movement| match movement {
                    ToolMove::Feed { to, z, .. } => [to[0], to[1], *z],
                    _ => unreachable!(),
                })
                .collect();
            let mut keep = vec![false; run.len()];
            keep[0] = true;
            keep[run.len() - 1] = true;
            simplify_range(&points, 0, run.len() - 1, tolerance_mm, &mut keep);
            simplified.extend(
                run.iter()
                    .zip(keep)
                    .filter_map(|(movement, keep)| keep.then(|| movement.clone())),
            );
        }
        self.moves = simplified;
        Ok(())
    }
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut it = self.moves.iter().map(ToolMove::end);
        let first = it.next()?;
        let mut min = first;
        let mut max = first;
        for p in it {
            min[0] = min[0].min(p[0]);
            min[1] = min[1].min(p[1]);
            max[0] = max[0].max(p[0]);
            max[1] = max[1].max(p[1]);
        }
        Some((min, max))
    }
}

fn simplify_range(
    points: &[[f64; 3]],
    start: usize,
    end: usize,
    tolerance: f64,
    keep: &mut [bool],
) {
    if end <= start + 1 {
        return;
    }
    let a = points[start];
    let b = points[end];
    let mut farthest = (0.0, None);
    for (index, point) in points.iter().enumerate().take(end).skip(start + 1) {
        let distance = point_segment_distance(*point, a, b);
        if distance > farthest.0 {
            farthest = (distance, Some(index));
        }
    }
    if let Some(index) = farthest.1.filter(|_| farthest.0 > tolerance) {
        keep[index] = true;
        simplify_range(points, start, index, tolerance, keep);
        simplify_range(points, index, end, tolerance, keep);
    }
}

fn point_segment_distance(point: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ap = [point[0] - a[0], point[1] - a[1], point[2] - a[2]];
    let length_sq = ab.iter().map(|v| v * v).sum::<f64>();
    let t = if length_sq <= f64::EPSILON {
        0.0
    } else {
        (ap.iter().zip(ab).map(|(p, d)| p * d).sum::<f64>() / length_sq).clamp(0.0, 1.0)
    };
    (0..3)
        .map(|axis| (point[axis] - (a[axis] + t * ab[axis])).powi(2))
        .sum::<f64>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplification_respects_deviation_bound_and_preserves_rapid_boundaries() {
        let mut path = Toolpath::default();
        path.push_rapid([0.0, 0.0], 5.0);
        path.push_feed([0.0, 0.0], 1.0, 100.0);
        path.push_feed([1.0, 0.01], 1.0, 100.0);
        path.push_feed([2.0, 0.0], 1.0, 100.0);
        path.push_rapid([2.0, 0.0], 5.0);
        path.simplify_feed_runs(0.02).unwrap();
        assert_eq!(path.moves.len(), 4);
        assert!(matches!(path.moves[0], ToolMove::Rapid { .. }));
        assert!(matches!(path.moves[3], ToolMove::Rapid { .. }));
    }

    #[test]
    fn rejects_invalid_simplification_bound() {
        assert!(Toolpath::default().simplify_feed_runs(f64::NAN).is_err());
    }
}
