//! Zilla's animation (`ZilaModel` in the mod): a dinosaur's heavy, rolling stride with its head
//! held steady, short arms swaying, a cascading tail, and its moves: the bite (it rears back
//! with its jaws open, lunges down, snaps shut and tosses its head to the side it flings its
//! catch), the roar (head thrown up, jaws wide, trembling), the tail swipe and the stomp.

use crate::anim::{AnimState, Move};
use crate::model::{Model, PartPose};
use crate::zila;

/// The part indices the animation drives, looked up once.
pub struct ZilaRig {
    hips: usize,
    body: usize,
    neck: usize,
    head: usize,
    jaw: usize,
    right_arm: usize,
    left_arm: usize,
    right_forearm: usize,
    left_forearm: usize,
    right_leg: usize,
    left_leg: usize,
    right_shin: usize,
    left_shin: usize,
    right_foot: usize,
    left_foot: usize,
    tail: [usize; 8],
}

impl ZilaRig {
    pub fn new(model: &Model) -> Result<Self, String> {
        let find = |path: &str| model.find(path).ok_or_else(|| format!("Zilla's model has no part {path}"));
        let mut tail = [0; 8];
        let mut path = String::from("root/hips");
        for (i, slot) in tail.iter_mut().enumerate() {
            path = format!("{path}/tail{}", i + 1);
            *slot = find(&path)?;
        }
        Ok(Self {
            hips: find("root/hips")?,
            body: find("root/hips/body")?,
            neck: find("root/hips/body/neck")?,
            head: find("root/hips/body/neck/head")?,
            jaw: find("root/hips/body/neck/head/jaw")?,
            right_arm: find("root/hips/body/right_arm")?,
            left_arm: find("root/hips/body/left_arm")?,
            right_forearm: find("root/hips/body/right_arm/right_forearm")?,
            left_forearm: find("root/hips/body/left_arm/left_forearm")?,
            right_leg: find("root/right_leg")?,
            left_leg: find("root/left_leg")?,
            right_shin: find("root/right_leg/right_shin")?,
            left_shin: find("root/left_leg/left_shin")?,
            right_foot: find("root/right_leg/right_shin/right_foot")?,
            left_foot: find("root/left_leg/left_shin/left_foot")?,
            tail,
        })
    }

    /// Poses the model for one frame.
    pub fn pose(&self, model: &Model, s: &AnimState) -> Vec<PartPose> {
        let mut p = model.rest_pose();
        // Its ground speed is small next to its size: amplified, or the stride barely shows.
        let walk_speed = (s.walk_speed * 2.5).min(1.0);
        let stride = s.walk_position * 0.9;
        let age = s.age_ticks;

        // Head look, split between neck and head.
        let yaw = s.head_yaw.clamp(-45.0, 45.0).to_radians();
        let pitch = s.head_pitch.clamp(-30.0, 30.0).to_radians();
        p[self.neck].y_rot += yaw * 0.45;
        p[self.neck].x_rot += pitch * 0.35;
        p[self.head].y_rot += yaw * 0.55;
        p[self.head].x_rot += pitch * 0.5;

        // Breathing, and the jaw working a little.
        p[self.body].x_rot += (age * 0.08).sin() * 0.015;
        p[self.jaw].x_rot += (age * 0.05).sin().max(0.0) * 0.06;

        // A heavy, rolling stride: the thigh swings, the shin folds as the foot lifts, the foot stays level.
        let swing = stride.cos() * 0.5 * walk_speed;
        p[self.right_leg].x_rot += swing;
        p[self.left_leg].x_rot -= swing;
        p[self.right_shin].x_rot += stride.sin().max(0.0) * 0.6 * walk_speed;
        p[self.left_shin].x_rot += (-stride.sin()).max(0.0) * 0.6 * walk_speed;
        p[self.right_foot].x_rot -= swing * 0.6;
        p[self.left_foot].x_rot += swing * 0.6;
        p[self.hips].y -= stride.sin().abs() * 1.2 * walk_speed;
        p[self.hips].z_rot += stride.cos() * 0.05 * walk_speed;
        p[self.body].x_rot += (stride * 2.0).sin() * 0.03 * walk_speed;
        // The head stays steady, as an animal's does.
        p[self.neck].x_rot -= (stride * 2.0).sin() * 0.03 * walk_speed;

        // Short arms: a little sway.
        let arm_idle = (age * 0.07).sin() * 0.05;
        p[self.right_arm].x_rot += -swing * 0.25 + arm_idle;
        p[self.left_arm].x_rot += swing * 0.25 - arm_idle;

        // The tail: a cascading side-to-side sway, stronger towards the tip.
        let sway = (age * 0.07).sin() * 0.04 + stride.cos() * 0.08 * walk_speed;
        for (i, &segment) in self.tail.iter().enumerate() {
            p[segment].y_rot += sway * (0.5 + i as f32 * 0.3);
            p[segment].x_rot += (age * 0.05 - i as f32 * 0.5).sin() * 0.012;
        }

        match s.movement {
            Move::Bite => self.bite(&mut p, s.move_time, s.move_direction),
            Move::Roar => self.roar(&mut p, s.move_time),
            Move::TailSwipe => self.tail_swipe(&mut p, s.move_time, s.move_direction),
            Move::Stomp => self.stomp(&mut p, s.move_time, s.move_direction),
            _ => {}
        }
        p
    }

    fn bite(&self, p: &mut [PartPose], t: f32, direction: i32) {
        let bite = zila::BITE;
        let lunge = bite.lunge(f64::from(t)) as f32;
        let back = (-lunge).max(0.0);
        let forward = lunge.max(0.0);
        // Rearing back: the head up and back, jaws opening; then the whole front lunges down at the ground.
        p[self.body].x_rot += -back * 0.12 + forward * 0.65;
        p[self.neck].x_rot += -back * 0.35 + forward * 0.8;
        p[self.head].x_rot += -back * 0.15 - forward * 0.45;
        p[self.jaw].x_rot += bite.jaw(f64::from(t)) as f32 * 0.8;
        p[self.hips].y += forward * 4.0;
        p[self.right_leg].x_rot -= forward * 0.3;
        p[self.left_leg].x_rot -= forward * 0.3;
        p[self.right_shin].x_rot += forward * 0.5;
        p[self.left_shin].x_rot += forward * 0.5;
        p[self.right_foot].x_rot -= forward * 0.2;
        p[self.left_foot].x_rot -= forward * 0.2;
        // The tail rises to balance the head going down.
        p[self.tail[0]].x_rot += forward * 0.2 - back * 0.05;
        // Tossing its head to the side it flings its catch (a head turned +y looks to its right).
        let after_snap = t - (bite.windup + bite.snap) as f32;
        if after_snap > 0.0 {
            let toss = ((after_snap / bite.recover as f32).clamp(0.0, 1.0) * std::f32::consts::PI).sin();
            p[self.neck].y_rot -= direction as f32 * 0.45 * toss;
            p[self.head].y_rot -= direction as f32 * 0.3 * toss;
        }
    }

    fn roar(&self, p: &mut [PartPose], t: f32) {
        let total = zila::ROAR_TICKS as f32;
        let rise = smooth(t / 8.0) * (1.0 - smooth((t - (total - 10.0)) / 10.0));
        let tremble = (t * 1.7).sin() * 0.04 * rise;
        p[self.body].x_rot -= rise * 0.12;
        p[self.neck].x_rot -= rise * 0.45;
        p[self.head].x_rot -= rise * 0.35;
        p[self.head].y_rot += tremble;
        p[self.neck].z_rot += tremble * 0.5;
        p[self.jaw].x_rot += rise * 0.9;
        p[self.right_arm].x_rot -= rise * 0.5;
        p[self.left_arm].x_rot -= rise * 0.5;
        p[self.right_arm].z_rot += rise * 0.25;
        p[self.left_arm].z_rot -= rise * 0.25;
        p[self.right_forearm].x_rot -= rise * 0.2;
        p[self.left_forearm].x_rot -= rise * 0.2;
        p[self.tail[0]].x_rot += rise * 0.1;
    }

    fn tail_swipe(&self, p: &mut [PartPose], t: f32, direction: i32) {
        let tail = zila::TAIL;
        let angle = tail.angle(f64::from(t), direction) as f32;
        // How fast it is swinging: the rest of the tail lags behind the base, like a whip.
        let speed = tail.angle(f64::from(t) + 1.0, direction) as f32 - angle;
        p[self.tail[0]].y_rot += angle;
        for (i, &segment) in self.tail.iter().enumerate().skip(1) {
            p[segment].y_rot -= speed * 0.03 * i as f32;
        }
        // Winding up it crouches, twists its shoulders against the tail and snarls over its
        // shoulder at its target.
        let windup = tail.windup as f32;
        let effort = if t < windup { (t / windup).clamp(0.0, 1.0) } else { (1.0 - (t - windup) / tail.sweep as f32).clamp(0.0, 1.0) };
        p[self.hips].y += effort * 1.5;
        p[self.body].y_rot -= angle * 0.15;
        p[self.neck].y_rot -= direction as f32 * 0.35 * effort;
        p[self.jaw].x_rot += 0.4 * effort;
        p[self.right_leg].x_rot -= effort * 0.08;
        p[self.left_leg].x_rot -= effort * 0.08;
        p[self.right_shin].x_rot += effort * 0.12;
        p[self.left_shin].x_rot += effort * 0.12;
    }

    fn stomp(&self, p: &mut [PartPose], t: f32, direction: i32) {
        let stomp = zila::STOMP;
        let lift = stomp.foot_lift(f64::from(t)) as f32;
        let windup = stomp.windup as f32;
        // Just after the foot lands it sinks into the blow, and it shakes it.
        let landed = if t < windup { 0.0 } else { (1.0 - (t - windup) / 10.0).clamp(0.0, 1.0) };
        let (leg, shin, foot, standing_shin) = if direction > 0 {
            (self.left_leg, self.left_shin, self.left_foot, self.right_shin)
        } else {
            (self.right_leg, self.right_shin, self.right_foot, self.left_shin)
        };
        p[leg].x_rot -= lift * 1.1;
        p[shin].x_rot += lift * 0.5;
        p[foot].x_rot -= lift * 0.3;
        p[standing_shin].x_rot += lift * 0.12;
        p[self.hips].z_rot += direction as f32 * lift * 0.07;
        p[self.hips].y += landed * 1.5;
        p[self.body].x_rot += -lift * 0.15 + landed * 0.15;
        p[self.right_arm].x_rot -= lift * 0.35;
        p[self.left_arm].x_rot -= lift * 0.35;
        p[self.jaw].x_rot += lift * 0.35 + landed * 0.6;
        p[self.head].x_rot -= lift * 0.12;
        for (i, &segment) in self.tail.iter().enumerate() {
            p[segment].x_rot += landed * 0.04 * (t * 2.0 + i as f32).sin();
        }
    }
}

fn smooth(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
