//! Godzilla's animation: a heavy rolling gait, a swaying tail, his jaw opening for the atomic
//! breath while his dorsal plates rise and light up from the tail tip to the neck, and the
//! tail swipe and stomp. What every kaiju's animation reads ([`AnimState`]) is here too;
//! Zilla's rig is in `zila_anim`.

use crate::godzilla;
use crate::model::{Model, PartPose};

/// How long (ticks) the plates take to light up from the tail tip to the neck.
pub const PLATE_LIGHT_TICKS: f32 = 38.0;
/// How far a plate rises when it lights up, in model units.
const PLATE_RISE: f32 = 2.5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Move {
    #[default]
    None,
    TailSwipe,
    Stomp,
    /// Zilla's: it rears back, lunges and snaps, and flings what it catches aside.
    Bite,
    /// Zilla's, at whatever it first sets eyes on.
    Roar,
}

/// Everything the animation reads, interpolated for the frame being drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnimState {
    pub walk_position: f32,
    pub walk_speed: f32,
    pub age_ticks: f32,
    /// Head look relative to the body, degrees.
    pub head_yaw: f32,
    pub head_pitch: f32,
    /// 0..1: how far his jaw is open for the breath.
    pub breath_amount: f32,
    /// Ticks since he started charging the breath (0 when he isn't).
    pub breath_time: f32,
    pub movement: Move,
    pub move_time: f32,
    /// +1 his left, -1 his right.
    pub move_direction: i32,
}

/// The part indices the animation drives, looked up once.
pub struct Rig {
    hips: usize,
    body: usize,
    neck: usize,
    head: usize,
    jaw: usize,
    left_arm: usize,
    right_arm: usize,
    left_forearm: usize,
    right_forearm: usize,
    left_leg: usize,
    right_leg: usize,
    left_shin: usize,
    right_shin: usize,
    left_foot: usize,
    right_foot: usize,
    tail: [usize; 7],
    plates: Vec<usize>,
    /// Per plate: rise direction x, y, z, the order it lights up in (0 neck .. 1 tail tip), height.
    plate_info: Vec<Vec<f32>>,
}

impl Rig {
    pub fn new(model: &Model) -> Result<Self, String> {
        let find = |path: &str| model.find(path).ok_or_else(|| format!("Godzilla's model has no part {path}"));
        let mut tail = [0; 7];
        let mut path = String::from("root/hips");
        for (i, slot) in tail.iter_mut().enumerate() {
            path = format!("{path}/tail{}", i + 1);
            *slot = find(&path)?;
        }
        let plates = model.strings("PLATE_PATHS").iter().map(|p| find(p)).collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            hips: find("root/hips")?,
            body: find("root/hips/body")?,
            neck: find("root/hips/body/neck")?,
            head: find("root/hips/body/neck/head")?,
            jaw: find("root/hips/body/neck/head/jaw")?,
            left_arm: find("root/hips/body/left_arm")?,
            right_arm: find("root/hips/body/right_arm")?,
            left_forearm: find("root/hips/body/left_arm/left_forearm")?,
            right_forearm: find("root/hips/body/right_arm/right_forearm")?,
            left_leg: find("root/left_leg")?,
            right_leg: find("root/right_leg")?,
            left_shin: find("root/left_leg/left_shin")?,
            right_shin: find("root/right_leg/right_shin")?,
            left_foot: find("root/left_leg/left_shin/left_foot")?,
            right_foot: find("root/right_leg/right_shin/right_foot")?,
            tail,
            plates,
            plate_info: model.table("PLATE_INFO"),
        })
    }

    /// Poses the model for one frame. `glow_pass` hides every plate that hasn't lit up yet,
    /// for drawing the plates' blue glow on its own.
    pub fn pose(&self, model: &Model, s: &AnimState, glow_pass: bool) -> Vec<PartPose> {
        let mut p = model.rest_pose();
        let walk_speed = (s.walk_speed * 3.0).min(1.0);
        let age = s.age_ticks;
        let breath = s.breath_amount;
        let stride = s.walk_position * 0.8;

        // Head look, split between neck and head; the head tips back as the jaw opens.
        let yaw = s.head_yaw.clamp(-40.0, 40.0).to_radians();
        let pitch = s.head_pitch.clamp(-35.0, 35.0).to_radians();
        p[self.neck].y_rot += yaw * 0.45;
        p[self.neck].x_rot += pitch * 0.4;
        p[self.head].y_rot += yaw * 0.55;
        p[self.head].x_rot += pitch * 0.6 - breath * 0.15;
        p[self.jaw].x_rot += breath * 0.75 + (age * 0.03).sin().max(0.0) * 0.05;

        // Heavy, rolling gait: the thigh swings, the shin folds under as the foot lifts.
        let swing = stride.cos() * 0.42 * walk_speed;
        p[self.right_leg].x_rot += swing;
        p[self.left_leg].x_rot -= swing;
        p[self.right_shin].x_rot += stride.sin().max(0.0) * 0.55 * walk_speed;
        p[self.left_shin].x_rot += (-stride.sin()).max(0.0) * 0.55 * walk_speed;
        p[self.right_foot].x_rot -= swing * 0.5;
        p[self.left_foot].x_rot += swing * 0.5;
        p[self.hips].y -= stride.sin().abs() * 1.2 * walk_speed;
        p[self.hips].z_rot += stride.cos() * 0.04 * walk_speed;
        p[self.body].x_rot += (stride * 2.0).sin() * 0.02 * walk_speed + (age * 0.05).sin() * 0.01;

        // Short arms: held forward, counter-swinging, drifting.
        let arm_idle = (age * 0.06).sin() * 0.04;
        p[self.right_arm].x_rot += -swing * 0.3 + arm_idle;
        p[self.left_arm].x_rot += swing * 0.3 - arm_idle;
        p[self.right_forearm].x_rot -= breath * 0.25;
        p[self.left_forearm].x_rot -= breath * 0.25;

        // The huge dragging tail sways side to side, more towards the tip.
        let sway = (age * 0.06).sin() * 0.035 + stride.cos() * 0.09 * walk_speed;
        for (i, &t) in self.tail.iter().enumerate() {
            p[t].y_rot += sway * (0.6 + i as f32 * 0.35);
        }

        match s.movement {
            Move::TailSwipe => self.tail_swipe(&mut p, s.move_time, s.move_direction),
            Move::Stomp => self.stomp(&mut p, s.move_time, s.move_direction),
            Move::Bite | Move::Roar | Move::None => {}
        }

        // The breath's charge: plates light up and rise one after another, tail tip first.
        let progress = s.breath_time / PLATE_LIGHT_TICKS;
        for (i, &plate) in self.plates.iter().enumerate() {
            let info = self.plate_info.get(i).map(Vec::as_slice).unwrap_or(&[0.0, 0.0, 0.0, 0.0]);
            let lit = if s.breath_time <= 0.0 { 0.0 } else { ((progress - info[3] * 0.85) / 0.15).clamp(0.0, 1.0) };
            if lit > 0.0 {
                let rise = PLATE_RISE * lit * lit * (3.0 - 2.0 * lit) * (breath * 2.0).min(1.0);
                p[plate].x += info[0] * rise;
                p[plate].y += info[1] * rise;
                p[plate].z += info[2] * rise;
            }
            // The glow texture is clear everywhere but the plates, so the glow pass only has to
            // hide the plates that haven't lit up yet.
            if glow_pass {
                p[plate].visible = lit > 0.0;
            }
        }
        p
    }

    fn tail_swipe(&self, p: &mut [PartPose], t: f32, direction: i32) {
        let tail = godzilla::TAIL;
        let angle = tail.angle(f64::from(t), direction) as f32;
        // The rest of the tail lags behind the base, like a whip.
        let speed = (tail.angle(f64::from(t) + 1.0, direction) as f32) - angle;
        p[self.tail[0]].y_rot += angle;
        for (i, &seg) in self.tail.iter().enumerate().skip(1) {
            p[seg].y_rot -= speed * 0.03 * i as f32;
        }
        // Winding up he crouches, twists his shoulders against the tail and roars over his shoulder.
        let windup = tail.windup as f32;
        let wound = (t / windup).clamp(0.0, 1.0);
        let effort = if t < windup { wound } else { (1.0 - (t - windup) / tail.sweep as f32).clamp(0.0, 1.0) };
        p[self.hips].y += effort * 2.0;
        p[self.body].y_rot -= angle * 0.15;
        p[self.neck].y_rot -= direction as f32 * 0.3 * effort;
        p[self.jaw].x_rot += 0.35 * effort;
        p[self.right_leg].x_rot -= effort * 0.08;
        p[self.left_leg].x_rot -= effort * 0.08;
        p[self.right_shin].x_rot += effort * 0.12;
        p[self.left_shin].x_rot += effort * 0.12;
    }

    fn stomp(&self, p: &mut [PartPose], t: f32, direction: i32) {
        let stomp = godzilla::STOMP;
        let lift = stomp.foot_lift(f64::from(t)) as f32;
        let windup = stomp.windup as f32;
        // Just after the foot lands he sinks into the blow, and it shakes him.
        let landed = if t < windup { 0.0 } else { (1.0 - (t - windup) / 12.0).clamp(0.0, 1.0) };
        let (leg, shin, foot, standing_shin) = if direction > 0 {
            (self.left_leg, self.left_shin, self.left_foot, self.right_shin)
        } else {
            (self.right_leg, self.right_shin, self.right_foot, self.left_shin)
        };
        p[leg].x_rot -= lift * 1.15;
        p[shin].x_rot += lift * 0.55;
        p[foot].x_rot -= lift * 0.35;
        p[standing_shin].x_rot += lift * 0.1;
        p[self.hips].z_rot += direction as f32 * lift * 0.06;
        p[self.hips].y += landed * 2.5;
        p[self.body].x_rot += -lift * 0.12 + landed * 0.15;
        p[self.right_arm].x_rot -= lift * 0.35;
        p[self.left_arm].x_rot -= lift * 0.35;
        p[self.jaw].x_rot += lift * 0.3 + landed * 0.6;
        p[self.head].x_rot -= lift * 0.1;
        for (i, &seg) in self.tail.iter().enumerate() {
            p[seg].x_rot += landed * 0.03 * (t * 2.0 + i as f32).sin();
        }
    }
}

/// The scale that makes a model stand `height` blocks to the top of its head.
pub fn model_scale(model: &Model, height: f64) -> f32 {
    let model_height = model.number("MODEL_HEIGHT").unwrap_or(148.65);
    height as f32 * 16.0 / model_height
}
