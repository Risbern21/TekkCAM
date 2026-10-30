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
