//! Path (BX5): waypoint polylines — open chains and closed loops. Loops wrap
//! and count laps (LapCompleted is the loop-TD seam: lap buffs, treasure
//! digs, anything lap-triggered hangs off it). Open chains finish once and
//! emit Arrived (consumed by the Lives leak policy).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Polyline path. Points live on the logic plane as `[x, z]` pairs
/// (serde-friendly); junctions are a reserved schema slot for maze maps.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct PathDef {
    pub points: Vec<[f32; 2]>,
    pub closed: bool,
    #[serde(default)]
    pub junctions: Vec<Junction>,
}

/// Reserved for maze/multi-loop maps (schema stability only, unused in v1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Junction {
    pub at_dist: f32,
    pub to_dist: f32,
}

fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

impl PathDef {
    pub fn open(points: Vec<[f32; 2]>) -> Self {
        Self { points, closed: false, junctions: Vec::new() }
    }

    pub fn closed_loop(points: Vec<[f32; 2]>) -> Self {
        Self { points, closed: true, junctions: Vec::new() }
    }

    pub fn length(&self) -> f32 {
        let n = self.points.len();
        if n < 2 {
            return 0.0;
        }
        let segs = if self.closed { n } else { n - 1 };
        let mut len = 0.0;
        for i in 0..segs {
            len += dist2(self.points[i], self.points[(i + 1) % n]);
        }
        len
    }

    /// Position along the path at distance `d`. Open chains clamp at the
    /// end; closed loops wrap (rem_euclid, so negatives are safe too).
    pub fn sample(&self, d: f32) -> Vec2 {
        let n = self.points.len();
        if n == 0 {
            return Vec2::ZERO;
        }
        let first = Vec2::new(self.points[0][0], self.points[0][1]);
        if n == 1 {
            return first;
        }
        let len = self.length();
        if len <= 0.0 {
            return first;
        }
        let d = if self.closed { d.rem_euclid(len) } else { d.clamp(0.0, len) };
        let segs = if self.closed { n } else { n - 1 };
        let mut acc = 0.0;
        for i in 0..segs {
            let a = self.points[i];
            let b = self.points[(i + 1) % n];
            let l = dist2(a, b);
            if d <= acc + l {
                let t = if l > 0.0 { (d - acc) / l } else { 0.0 };
                return Vec2::new(a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
            }
            acc += l;
        }
        Vec2::new(self.points[n - 1][0], self.points[n - 1][1])
    }
}

#[derive(Component, Debug)]
pub struct PathFollower {
    pub path: Entity,
    pub progress: f32,
    pub laps: u32,
    pub finished: bool,
}

/// Effective move speed (from the unit def; lap buffs multiply into this
/// until the StatMods pipeline lands with the skill-framework extraction).
#[derive(Component, Debug)]
pub struct MoveSpeed(pub f32);

#[derive(Message, Debug)]
pub struct LapCompleted {
    pub unit: Entity,
    pub lap: u32,
}

#[derive(Message, Debug)]
pub struct Arrived {
    pub unit: Entity,
}

/// Director: advance followers by speed * dt, wrap loops (counting laps),
/// finish open chains once, and write the derived Transform (kinematic —
/// no physics).
pub fn advance_followers(
    time: Res<Time>,
    paths: Query<&PathDef>,
    mut followers: Query<(Entity, &mut PathFollower, &MoveSpeed, &mut Transform)>,
    mut laps: MessageWriter<LapCompleted>,
    mut arrived: MessageWriter<Arrived>,
) {
    let dt = time.delta_secs();
    for (e, mut f, speed, mut tf) in &mut followers {
        if f.finished {
            continue;
        }
        let Ok(path) = paths.get(f.path) else { continue };
        f.progress += speed.0 * dt;
        let len = path.length();
        if path.closed {
            if f.progress >= len && len > 0.0 {
                f.progress -= len;
                f.laps += 1;
                laps.write(LapCompleted { unit: e, lap: f.laps });
            }
        } else if f.progress >= len {
            f.progress = len;
            f.finished = true;
            arrived.write(Arrived { unit: e });
        }
        let p = path.sample(f.progress);
        tf.translation.x = p.x;
        tf.translation.z = p.y;
    }
}
