use crate::cam::toolpath::ToolMove;

/// Time-based playback state for the generated moves.
#[derive(Debug, Clone, Default)]
pub struct Simulator {
    pub move_index: usize,
    pub fraction: f64,
    pub position: Option<[f64; 2]>,
    pub z: f64,
    pub running: bool,
}

impl Simulator {
    pub fn reset(&mut self) {
        *self = Self {
            position: Some([0.0, 0.0]),
            running: true,
            ..Self::default()
        };
    }
    pub fn stop(&mut self) {
        self.running = false;
    }
    pub fn advance(&mut self, moves: &[ToolMove], delta_seconds: f32) {
        if !self.running {
            return;
        }
        let mut time = delta_seconds.max(0.0) as f64;
        while time > 0.0 && self.move_index < moves.len() {
            let movement = &moves[self.move_index];
            let (end, z, rate) = match movement {
                ToolMove::Rapid { to, z } => (*to, *z, 3000.0),
                ToolMove::Feed { to, z, feed_mm_min } => (*to, *z, *feed_mm_min),
                ToolMove::Arc {
                    to, z, feed_mm_min, ..
                } => (*to, *z, *feed_mm_min),
                ToolMove::Drill {
                    at,
                    depth,
                    plunge_mm_min,
                    ..
                } => (*at, *depth, *plunge_mm_min),
            };
            let start = self.position.unwrap_or(end);
            let distance = (end[0] - start[0]).hypot(end[1] - start[1]) + (z - self.z).abs();
            let duration = (distance / rate.max(1.0) * 60.0).max(0.025);
            let remaining = duration * (1.0 - self.fraction);
            if time >= remaining {
                time -= remaining;
                self.position = Some(end);
                self.z = z;
                self.move_index += 1;
                self.fraction = 0.0;
            } else {
                self.fraction += time / duration;
                self.position = Some(interpolate_xy(movement, start, end, self.fraction));
                self.z += (z - self.z) * (time / duration);
                time = 0.0;
            }
        }
        if self.move_index >= moves.len() {
            self.running = false;
        }
    }
}

fn interpolate_xy(movement: &ToolMove, start: [f64; 2], end: [f64; 2], t: f64) -> [f64; 2] {
    if let ToolMove::Arc {
        center_offset,
        clockwise,
        ..
    } = movement
    {
        let center = [start[0] + center_offset[0], start[1] + center_offset[1]];
        let start_angle = (start[1] - center[1]).atan2(start[0] - center[0]);
        let end_angle = (end[1] - center[1]).atan2(end[0] - center[0]);
        let tau = std::f64::consts::TAU;
        let mut sweep = end_angle - start_angle;
        if *clockwise {
            while sweep >= 0.0 {
                sweep -= tau;
            }
        } else {
            while sweep <= 0.0 {
                sweep += tau;
            }
        }
        // Equal start/end points denote a complete circle in G-code.
        if (start[0] - end[0]).hypot(start[1] - end[1]) < 1e-9 {
            sweep = if *clockwise { -tau } else { tau };
        }
        let radius = (start[0] - center[0]).hypot(start[1] - center[1]);
        let angle = start_angle + sweep * t;
        [
            center[0] + radius * angle.cos(),
            center[1] + radius * angle.sin(),
        ]
    } else {
        [
            start[0] + (end[0] - start[0]) * t,
            start[1] + (end[1] - start[1]) * t,
        ]
    }
}
