//! A giant's follow-along hitboxes: with its own column, they cover the whole body, so a shot
//! at the head, a plate, a claw or the tip of the tail lands on it.

use crate::combat::Tail;

/// A box in the giant's own frame: `length` along the way it faces, `width` across, `height`
/// tall, centred `forward` ahead of its centre and `side` to its left, from `up` above its feet.
/// A tail box swings round with the tail.
#[derive(Clone, Copy, Debug)]
pub struct BodyBox {
    pub length: f64,
    pub width: f64,
    pub height: f64,
    pub forward: f64,
    pub up: f64,
    pub side: f64,
    pub tail: bool,
}

const fn body(length: f64, width: f64, height: f64, forward: f64, up: f64, side: f64) -> BodyBox {
    BodyBox { length, width, height, forward, up, side, tail: false }
}

const fn tail(length: f64, width: f64, height: f64, forward: f64, up: f64, side: f64) -> BodyBox {
    BodyBox { length, width, height, forward, up, side, tail: true }
}

/// Godzilla's boxes, fitted to his model standing, swaying and striding.
pub const GODZILLA: [BodyBox; 21] = [
    body(14.5, 12.4, 16.1, 9.75, 34.9, 0.0),
    body(19.1, 20.6, 28.5, 0.55, 13.8, 0.0),
    body(16.2, 24.6, 14.2, -6.7, 33.1, 0.0),
    body(16.3, 19.2, 16.0, -9.35, 20.9, 0.0),
    body(20.8, 21.8, 12.3, -2.1, 42.0, 0.0),
    body(7.0, 7.8, 7.5, 4.4, 31.7, 8.7),
    body(7.0, 7.8, 7.5, 4.4, 31.7, -8.7),
    body(8.7, 7.3, 8.3, 11.85, 24.7, 10.25),
    body(8.7, 7.3, 8.3, 11.85, 24.7, -10.25),
    body(8.0, 6.9, 8.1, 5.3, 26.7, 10.95),
    body(8.0, 6.9, 8.1, 5.3, 26.7, -10.95),
    body(16.1, 24.8, 14.7, -7.55, -2.9, 0.0),
    body(18.6, 9.2, 14.2, 7.1, -4.3, 5.7),
    body(18.6, 9.2, 14.2, 7.1, -4.3, -5.7),
    body(10.8, 26.0, 14.2, -6.0, 10.7, 0.0),
    body(11.6, 26.0, 17.8, 2.0, 7.4, 0.0),
    tail(18.3, 15.2, 20.3, -13.35, 11.9, 0.3),
    tail(18.0, 16.1, 20.0, -23.6, 3.0, 2.25),
    tail(20.0, 20.9, 14.2, -32.0, -1.2, 8.05),
    tail(24.1, 21.9, 8.3, -45.85, -0.9, 18.55),
    tail(19.8, 23.3, 6.9, -48.9, -1.0, -0.05),
];

/// Zilla's boxes: with its 6x9 column they cover the whole body, down to the tips of its
/// spikes, claws and tail, standing or striding.
pub const ZILA: [BodyBox; 17] = [
    body(7.8, 4.2, 6.4, 8.8, 8.8, 0.0),
    body(6.3, 5.4, 4.4, 5.35, 5.2, 0.0),
    body(7.6, 6.2, 4.1, 4.2, 8.7, 0.0),
    body(6.6, 6.4, 5.8, 4.9, 10.5, 0.0),
    body(7.6, 6.2, 6.1, 0.4, 8.4, 0.0),
    body(3.4, 1.7, 2.6, 7.6, 5.6, 2.45),
    body(3.4, 1.7, 2.6, 7.6, 5.6, -2.45),
    body(2.2, 2.1, 3.4, 6.3, 6.7, 1.95),
    body(2.2, 2.1, 3.4, 6.3, 6.7, -1.95),
    body(6.6, 2.6, 5.1, 3.4, -0.7, 2.0),
    body(6.6, 2.6, 5.1, 3.4, -0.7, -2.0),
    body(6.3, 7.2, 5.7, 0.55, 3.0, 0.0),
    body(4.9, 3.0, 6.3, -2.25, -0.6, 2.1),
    body(4.9, 3.0, 6.3, -2.25, -0.6, -2.1),
    tail(7.1, 4.6, 6.7, -5.85, 3.8, 0.2),
    tail(6.3, 6.2, 6.3, -10.35, 0.4, 1.3),
    tail(6.8, 11.2, 4.0, -14.6, -0.6, 2.1),
];

/// An axis-aligned box in world blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb {
    /// Where a segment from `a` to `b` first enters the box, as a fraction 0..1 of the way.
    pub fn clip(&self, a: [f64; 3], b: [f64; 3]) -> Option<f64> {
        let mut enter: f64 = 0.0;
        let mut leave: f64 = 1.0;
        for i in 0..3 {
            let d = b[i] - a[i];
            if d.abs() < 1e-12 {
                if a[i] < self.min[i] || a[i] > self.max[i] {
                    return None;
                }
                continue;
            }
            let (t0, t1) = ((self.min[i] - a[i]) / d, (self.max[i] - a[i]) / d);
            enter = enter.max(t0.min(t1));
            leave = leave.min(t0.max(t1));
            if enter > leave {
                return None;
            }
        }
        Some(enter)
    }

    pub fn contains(&self, p: [f64; 3]) -> bool {
        (0..3).all(|i| p[i] >= self.min[i] && p[i] <= self.max[i])
    }
}

/// The world box of each body box for a giant standing at `feet`, facing `yaw_degrees`
/// (Minecraft yaw), its tail swung `tail_angle` radians towards its left. Each box is the
/// smallest axis-aligned one round the turned box, as the mod's hitbox entities are.
pub fn world_boxes(specs: &[BodyBox], feet: [f64; 3], yaw_degrees: f32, tail_spec: &Tail, tail_angle: f64) -> Vec<Aabb> {
    let yaw = f64::from(yaw_degrees).to_radians();
    let (fwd_x, fwd_z) = (-yaw.sin(), yaw.cos());
    specs
        .iter()
        .map(|spec| {
            let (mut forward, mut side, mut box_yaw) = (spec.forward, spec.side, yaw);
            if spec.tail && tail_angle != 0.0 {
                (forward, side) = tail_spec.swing_point(forward, side, tail_angle);
                box_yaw += tail_angle;
            }
            let centre = [feet[0] + fwd_x * forward + fwd_z * side, feet[1] + spec.up, feet[2] + fwd_z * forward - fwd_x * side];
            let (sin, cos) = (box_yaw.sin().abs(), box_yaw.cos().abs());
            let half_x = (sin * spec.length + cos * spec.width) / 2.0;
            let half_z = (cos * spec.length + sin * spec.width) / 2.0;
            Aabb {
                min: [centre[0] - half_x, centre[1], centre[2] - half_z],
                max: [centre[0] + half_x, centre[1] + spec.height, centre[2] + half_z],
            }
        })
        .collect()
}
