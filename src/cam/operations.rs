use crate::{
    cam::{
        tool::Tool,
        toolpath::{Point, ToolMove, Toolpath},
    },
    workspace::object::Geometry,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationKind {
    Isolation,
    Pocket,
    Drill,
    Contour,
    Placeholder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub name: String,
    pub kind: OperationKind,
    pub tool: Tool,
    pub depth_per_pass_mm: f64,
    pub total_depth_mm: f64,
    pub margin_mm: f64,
    pub generated: bool,
    #[serde(default)]
    pub toolpath: Toolpath,
}

impl Operation {
    pub fn placeholder() -> Self {
        Self {
            name: "New Operation".into(),
            kind: OperationKind::Placeholder,
            tool: Tool::default(),
            depth_per_pass_mm: 0.2,
            total_depth_mm: 1.6,
            margin_mm: 0.0,
            generated: false,
            toolpath: Toolpath::default(),
        }
    }
    pub fn isolation() -> Self {
        Self {
            name: "Isolation Routing".into(),
            kind: OperationKind::Isolation,
            ..Self::placeholder()
        }
    }
    pub fn pocket() -> Self {
        Self {
            name: "Pocket Milling".into(),
            kind: OperationKind::Pocket,
            ..Self::placeholder()
        }
    }
    pub fn drill() -> Self {
        Self {
            name: "Drilling".into(),
            kind: OperationKind::Drill,
            tool: Tool {
                diameter_mm: 0.8,
                ..Default::default()
            },
            ..Self::placeholder()
        }
    }
    pub fn contour() -> Self {
        Self {
            name: "Contour Cut".into(),
            kind: OperationKind::Contour,
            ..Self::placeholder()
        }
    }

    pub fn generate_from_geometry(&mut self, geometry: &Geometry) -> Result<usize, String> {
        if self.tool.diameter_mm <= 0.0 || self.total_depth_mm <= 0.0 {
            return Err("tool diameter and total depth must be positive".into());
        }
        let depth = -self.total_depth_mm;
        let mut path = Toolpath::default();
        match self.kind {
            OperationKind::Drill => generate_drills(&mut path, geometry, depth, &self.tool),
            OperationKind::Pocket => generate_pocket(
                &mut path,
                geometry,
                depth,
                &self.tool,
                self.depth_per_pass_mm,
            ),
            OperationKind::Isolation | OperationKind::Contour | OperationKind::Placeholder => {
                generate_lines(
                    &mut path,
                    geometry,
                    depth,
                    &self.tool,
                    self.margin_mm,
                    matches!(self.kind, OperationKind::Isolation),
                    self.depth_per_pass_mm,
                )
            }
        }
        let count = path.moves.len();
        self.toolpath = path;
        self.generated = count > 0;
        Ok(count)
    }
    pub fn generate(&mut self) {
        self.generated = false;
        self.toolpath = Toolpath::default();
    }
}

fn pass_depths(total: f64, step: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut z = -step.abs();
    while z > total {
        out.push(z);
        z -= step.abs();
    }
    out.push(total);
    out
}
fn generate_drills(path: &mut Toolpath, g: &Geometry, depth: f64, tool: &Tool) {
    let mut hits = g.drills.clone();
    let mut pos = [0.0, 0.0];
    while !hits.is_empty() {
        let (i, _) = hits
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                dist(pos, [a.x, a.y])
                    .partial_cmp(&dist(pos, [b.x, b.y]))
                    .unwrap()
            })
            .unwrap();
        let h = hits.remove(i);
        let p = [h.x, h.y];
        path.push_rapid(p, 0.0);
        path.moves.push(ToolMove::Drill {
            at: p,
            depth,
            retract: 0.0,
            plunge_mm_min: tool.plunge_rate_mm_min,
        });
        pos = p;
    }
}
fn generate_lines(
    path: &mut Toolpath,
    g: &Geometry,
    depth: f64,
    tool: &Tool,
    margin: f64,
    isolation: bool,
    pass: f64,
) {
    let off = if isolation {
        tool.diameter_mm * 0.5 + margin
    } else {
        margin
    };
    for seg in g.lines.chunks_exact(2) {
        let a = [seg[0][0] as f64, seg[0][1] as f64];
        let b = [seg[1][0] as f64, seg[1][1] as f64];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            continue;
        }
        let n = [-dy / len * off, dx / len * off];
        let a = [a[0] + n[0], a[1] + n[1]];
        let b = [b[0] + n[0], b[1] + n[1]];
        for z in pass_depths(depth, pass).into_iter().take(256) {
            path.push_rapid(a, 0.0);
            path.push_feed(a, z, tool.plunge_rate_mm_min);
            path.push_feed(b, z, tool.feed_rate_mm_min);
        }
    }
}
fn generate_pocket(path: &mut Toolpath, g: &Geometry, depth: f64, tool: &Tool, pass: f64) {
    let Some((mn, mx)) = g.bounds() else { return };
    let r = tool.diameter_mm * 0.5;
    let step = tool.diameter_mm.max(0.05) * 0.8;
    for z in pass_depths(depth, pass) {
        let mut y = mn[1] as f64 + r;
        let mut left = true;
        while y <= mx[1] as f64 - r + 1e-9 {
            let a = [mn[0] as f64 + r, y];
            let b = [mx[0] as f64 - r, y];
            let (s, e) = if left { (a, b) } else { (b, a) };
            path.push_rapid(s, 0.0);
            path.push_feed(s, z, tool.plunge_rate_mm_min);
            path.push_feed(e, z, tool.feed_rate_mm_min);
            y += step;
            left = !left;
        }
    }
}
fn dist(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drill_order_is_nearest_first() {
        let mut g = Geometry::default();
        g.drills = vec![
            crate::workspace::object::DrillHit {
                x: 10.0,
                y: 0.0,
                diameter: 1.0,
            },
            crate::workspace::object::DrillHit {
                x: 1.0,
                y: 0.0,
                diameter: 1.0,
            },
        ];
        let mut o = Operation::drill();
        o.generate_from_geometry(&g).unwrap();
        assert_eq!(o.toolpath.moves.len(), 4);
        assert_eq!(o.toolpath.moves[0].end(), [1.0, 0.0]);
    }
}
