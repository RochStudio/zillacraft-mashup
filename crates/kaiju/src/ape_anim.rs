//! The apes' animation (`KongModel` and `KongAnimations` in the mod), shared by Kong's and King
//! Kong's models, whose parts have the same names: heavy breathing, a slow rolling gait with
//! long arm swings, the head following its look, and a keyframed animation for each move, timed
//! to the move's ticks (see `ape`).

use crate::anim::{AnimState, Move};
use crate::ape;
use crate::model::{Model, PartPose};
use crate::species::Species;

/// A giant takes long, slow strides: its walk cycles this much slower than a normal mob's,
/// divided by its scale.
const STRIDE_ANIMATION_SCALE: f32 = 0.22;

#[derive(Clone, Copy)]
enum Bone {
    Hips,
    Body,
    Head,
    Jaw,
    RightArm,
    RightForearm,
    LeftArm,
    LeftForearm,
    RightLeg,
    LeftLeg,
}

use Bone::*;

const BONES: [(Bone, &str); 10] = [
    (Hips, "hips"),
    (Body, "body"),
    (Head, "head"),
    (Jaw, "jaw"),
    (RightArm, "right_arm"),
    (RightForearm, "right_forearm"),
    (LeftArm, "left_arm"),
    (LeftForearm, "left_forearm"),
    (RightLeg, "right_leg"),
    (LeftLeg, "left_leg"),
];

/// A keyframe: seconds into the move, a rotation (degrees) or a position (pixels, Y up) added to
/// the part's pose, and whether the way into it is a straight line (for fast strikes, so the
/// spline doesn't overshoot) or a Catmull-Rom spline.
#[derive(Clone, Copy)]
struct Key {
    time: f32,
    value: [f32; 3],
    linear: bool,
}

const fn smooth(time: f32, x: f32, y: f32, z: f32) -> Key {
    Key { time, value: [x, y, z], linear: false }
}

const fn linear(time: f32, x: f32, y: f32, z: f32) -> Key {
    Key { time, value: [x, y, z], linear: true }
}

struct Channel {
    bone: Bone,
    rotation: bool,
    keys: &'static [Key],
}

const fn rotation(bone: Bone, keys: &'static [Key]) -> Channel {
    Channel { bone, rotation: true, keys }
}

const fn position(bone: Bone, keys: &'static [Key]) -> Channel {
    Channel { bone, rotation: false, keys }
}

/// 50 ticks. Rears up and roars, then four chest beats at ticks 12, 18, 24 and 30: elbows out,
/// forearms folded across so the fists land on the chest.
const ROAR: [Channel; 8] = [
    position(Hips, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.3, 0.0, 2.0, 0.0), smooth(1.8, 0.0, 2.0, 0.0), smooth(2.4, 0.0, 0.0, 0.0)]),
    rotation(Body, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.3, -20.0, 0.0, 0.0), smooth(1.7, -18.0, 0.0, 0.0), smooth(2.1, -6.0, 0.0, 0.0), smooth(2.5, 0.0, 0.0, 0.0)]),
    rotation(Head, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.25, -25.0, 0.0, 0.0), smooth(0.5, -20.0, 0.0, 0.0), smooth(0.7, -3.0, 0.0, 0.0), smooth(1.55, -3.0, 0.0, 0.0), smooth(1.75, -22.0, 0.0, 0.0), smooth(2.1, -18.0, 0.0, 0.0), smooth(2.5, 0.0, 0.0, 0.0)]),
    rotation(Jaw, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.2, 38.0, 0.0, 0.0), smooth(0.5, 38.0, 0.0, 0.0), smooth(0.7, 8.0, 0.0, 0.0), smooth(1.55, 8.0, 0.0, 0.0), smooth(1.75, 35.0, 0.0, 0.0), smooth(2.1, 35.0, 0.0, 0.0), smooth(2.4, 0.0, 0.0, 0.0)]),
    rotation(RightArm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.35, -15.0, 0.0, 55.0), smooth(0.5, -15.0, 0.0, 55.0), linear(0.6, -7.0, 0.0, 37.0), smooth(0.75, -7.0, 0.0, 37.0), smooth(1.05, -15.0, 0.0, 55.0), linear(1.2, -7.0, 0.0, 37.0), smooth(1.35, -7.0, 0.0, 37.0), smooth(1.7, -15.0, 0.0, 55.0), smooth(2.3, 0.0, 0.0, 0.0)]),
    rotation(RightForearm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.35, -105.0, 0.0, 20.0), smooth(0.5, -105.0, 0.0, 20.0), linear(0.6, -142.0, 0.0, 39.0), smooth(0.75, -142.0, 0.0, 39.0), smooth(1.05, -105.0, 0.0, 20.0), linear(1.2, -142.0, 0.0, 39.0), smooth(1.35, -142.0, 0.0, 39.0), smooth(1.7, -105.0, 0.0, 20.0), smooth(2.3, 0.0, 0.0, 0.0)]),
    rotation(LeftArm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.4, -15.0, 0.0, -55.0), smooth(0.8, -15.0, 0.0, -55.0), linear(0.9, -7.0, 0.0, -37.0), smooth(1.05, -7.0, 0.0, -37.0), smooth(1.35, -15.0, 0.0, -55.0), linear(1.5, -7.0, 0.0, -37.0), smooth(1.65, -7.0, 0.0, -37.0), smooth(1.9, -15.0, 0.0, -55.0), smooth(2.4, 0.0, 0.0, 0.0)]),
    rotation(LeftForearm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.4, -105.0, 0.0, -20.0), smooth(0.8, -105.0, 0.0, -20.0), linear(0.9, -142.0, 0.0, -39.0), smooth(1.05, -142.0, 0.0, -39.0), smooth(1.35, -105.0, 0.0, -20.0), linear(1.5, -142.0, 0.0, -39.0), smooth(1.65, -142.0, 0.0, -39.0), smooth(1.9, -105.0, 0.0, -20.0), smooth(2.4, 0.0, 0.0, 0.0)]),
];

/// 30 ticks. A low backhand along the ground: the right arm drawn back out to the side, then
/// swept across the front at tick 14, knuckles skimming the ground.
const SWIPE: [Channel; 9] = [
    position(Hips, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.6, 0.0, -2.0, 0.0), smooth(0.75, 0.0, -3.0, 0.0), smooth(1.5, 0.0, 0.0, 0.0)]),
    rotation(Body, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.6, 18.0, 30.0, 0.0), linear(0.75, 24.0, -32.0, 0.0), smooth(1.0, 18.0, -26.0, 0.0), smooth(1.5, 0.0, 0.0, 0.0)]),
    rotation(Head, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.6, -12.0, -20.0, 0.0), smooth(0.8, -10.0, 18.0, 0.0), smooth(1.5, 0.0, 0.0, 0.0)]),
    rotation(Jaw, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.6, 25.0, 0.0, 0.0), smooth(0.9, 10.0, 0.0, 0.0), smooth(1.2, 0.0, 0.0, 0.0)]),
    rotation(RightArm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.6, -30.0, 75.0, 0.0), linear(0.7, -38.0, 0.0, 0.0), linear(0.8, -35.0, -70.0, 0.0), smooth(1.1, -20.0, -35.0, 0.0), smooth(1.5, 0.0, 0.0, 0.0)]),
    rotation(RightForearm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.6, -20.0, 0.0, 0.0), linear(0.7, 0.0, 0.0, 0.0), smooth(1.5, 0.0, 0.0, 0.0)]),
    rotation(LeftArm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.6, 15.0, 0.0, -15.0), smooth(0.8, -20.0, 0.0, -5.0), smooth(1.5, 0.0, 0.0, 0.0)]),
    rotation(RightLeg, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.6, -15.0, 0.0, 0.0), smooth(0.8, 5.0, 0.0, 0.0), smooth(1.5, 0.0, 0.0, 0.0)]),
    rotation(LeftLeg, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.6, 10.0, 0.0, 0.0), smooth(0.8, -10.0, 0.0, 0.0), smooth(1.5, 0.0, 0.0, 0.0)]),
];

/// 56 ticks. Both fists raised overhead, slammed into the ground at tick 24, a long recovery.
const SLAM: [Channel; 10] = [
    position(Hips, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(1.0, 0.0, 3.0, 0.0), smooth(1.1, 0.0, 3.0, 0.0), linear(1.2, 0.0, -4.0, 0.0), smooth(2.4, 0.0, -4.0, 0.0), smooth(2.8, 0.0, 0.0, 0.0)]),
    rotation(Body, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(1.0, -28.0, 0.0, 0.0), smooth(1.1, -30.0, 0.0, 0.0), linear(1.2, 30.0, 0.0, 0.0), smooth(2.4, 27.0, 0.0, 0.0), smooth(2.8, 0.0, 0.0, 0.0)]),
    rotation(Head, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(1.0, -25.0, 0.0, 0.0), smooth(1.2, -10.0, 0.0, 0.0), smooth(1.5, 10.0, 0.0, 0.0), smooth(2.4, 10.0, 0.0, 0.0), smooth(2.8, 0.0, 0.0, 0.0)]),
    rotation(Jaw, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.4, 35.0, 0.0, 0.0), smooth(1.1, 35.0, 0.0, 0.0), smooth(1.3, 10.0, 0.0, 0.0), smooth(2.0, 0.0, 0.0, 0.0)]),
    rotation(RightArm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.9, -150.0, 0.0, -12.0), smooth(1.1, -165.0, 0.0, -12.0), linear(1.2, -60.0, 0.0, -5.0), smooth(2.4, -58.0, 0.0, -5.0), smooth(2.8, 0.0, 0.0, 0.0)]),
    rotation(LeftArm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.9, -150.0, 0.0, 12.0), smooth(1.1, -165.0, 0.0, 12.0), linear(1.2, -60.0, 0.0, 5.0), smooth(2.4, -58.0, 0.0, 5.0), smooth(2.8, 0.0, 0.0, 0.0)]),
    rotation(RightForearm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(1.1, -15.0, 0.0, 0.0), linear(1.2, 0.0, 0.0, 0.0), smooth(2.8, 0.0, 0.0, 0.0)]),
    rotation(LeftForearm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(1.1, -15.0, 0.0, 0.0), linear(1.2, 0.0, 0.0, 0.0), smooth(2.8, 0.0, 0.0, 0.0)]),
    rotation(RightLeg, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(1.1, 0.0, 0.0, 0.0), linear(1.2, -25.0, 0.0, 0.0), smooth(2.4, -25.0, 0.0, 0.0), smooth(2.8, 0.0, 0.0, 0.0)]),
    rotation(LeftLeg, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(1.1, 0.0, 0.0, 0.0), linear(1.2, -25.0, 0.0, 0.0), smooth(2.4, -25.0, 0.0, 0.0), smooth(2.8, 0.0, 0.0, 0.0)]),
];

/// 44 ticks. Digs in, lifts the boulder overhead, throws at tick 30.
const BOULDER: [Channel; 8] = [
    position(Hips, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.45, 0.0, -3.0, 0.0), smooth(0.8, 0.0, -3.0, 0.0), smooth(1.4, 0.0, 2.0, 0.0), linear(1.55, 0.0, -1.0, 0.0), smooth(2.2, 0.0, 0.0, 0.0)]),
    rotation(Body, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.45, 35.0, 0.0, 0.0), smooth(0.6, 38.0, 0.0, 0.0), smooth(0.8, 30.0, 0.0, 0.0), smooth(1.4, -22.0, 0.0, 0.0), linear(1.55, 28.0, 0.0, 0.0), smooth(2.2, 0.0, 0.0, 0.0)]),
    rotation(Head, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.8, 10.0, 0.0, 0.0), smooth(1.4, -25.0, 0.0, 0.0), linear(1.55, 5.0, 0.0, 0.0), smooth(2.2, 0.0, 0.0, 0.0)]),
    rotation(Jaw, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(1.3, 5.0, 0.0, 0.0), smooth(1.5, 30.0, 0.0, 0.0), smooth(1.9, 0.0, 0.0, 0.0)]),
    rotation(RightArm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.45, -45.0, 0.0, -8.0), smooth(0.8, -50.0, 0.0, -10.0), smooth(1.4, -168.0, 0.0, -10.0), linear(1.55, -70.0, 0.0, -10.0), smooth(2.2, 0.0, 0.0, 0.0)]),
    rotation(LeftArm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.45, -45.0, 0.0, 8.0), smooth(0.8, -50.0, 0.0, 10.0), smooth(1.4, -168.0, 0.0, 10.0), linear(1.55, -70.0, 0.0, 10.0), smooth(2.2, 0.0, 0.0, 0.0)]),
    rotation(RightForearm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(1.4, -25.0, 0.0, 0.0), linear(1.55, 0.0, 0.0, 0.0), smooth(2.2, 0.0, 0.0, 0.0)]),
    rotation(LeftForearm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(1.4, -25.0, 0.0, 0.0), linear(1.55, 0.0, 0.0, 0.0), smooth(2.2, 0.0, 0.0, 0.0)]),
];

/// A crouch, the launch at tick 16, then an airborne pose held until the landing takes over.
const LEAP: [Channel; 8] = [
    position(Hips, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.7, 0.0, -5.0, 0.0), linear(0.95, 0.0, 2.0, 0.0), smooth(1.4, 0.0, 0.0, 0.0)]),
    rotation(Body, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.7, 30.0, 0.0, 0.0), linear(0.95, -15.0, 0.0, 0.0), smooth(1.4, 10.0, 0.0, 0.0)]),
    rotation(Head, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.7, -15.0, 0.0, 0.0), smooth(0.95, -5.0, 0.0, 0.0), smooth(1.4, 10.0, 0.0, 0.0)]),
    rotation(Jaw, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.5, 20.0, 0.0, 0.0), smooth(1.0, 30.0, 0.0, 0.0), smooth(1.4, 15.0, 0.0, 0.0)]),
    rotation(RightArm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.7, 35.0, 0.0, 10.0), linear(0.95, -150.0, 0.0, 20.0), smooth(1.4, -140.0, 0.0, 25.0)]),
    rotation(LeftArm, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.7, 35.0, 0.0, -10.0), linear(0.95, -150.0, 0.0, -20.0), smooth(1.4, -140.0, 0.0, -25.0)]),
    rotation(RightLeg, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.7, -35.0, 0.0, 0.0), linear(0.95, 25.0, 0.0, 0.0), smooth(1.4, -30.0, 0.0, 0.0)]),
    rotation(LeftLeg, &[smooth(0.0, 0.0, 0.0, 0.0), smooth(0.7, -35.0, 0.0, 0.0), linear(0.95, 25.0, 0.0, 0.0), smooth(1.4, -30.0, 0.0, 0.0)]),
];

/// 26 ticks. Touches down from the leap pose, fists planted, then rises.
const LAND: [Channel; 8] = [
    position(Hips, &[smooth(0.0, 0.0, 0.0, 0.0), linear(0.1, 0.0, -5.0, 0.0), smooth(0.9, 0.0, -4.0, 0.0), smooth(1.3, 0.0, 0.0, 0.0)]),
    rotation(Body, &[smooth(0.0, 10.0, 0.0, 0.0), linear(0.1, 40.0, 0.0, 0.0), smooth(0.9, 35.0, 0.0, 0.0), smooth(1.3, 0.0, 0.0, 0.0)]),
    rotation(Head, &[smooth(0.0, 10.0, 0.0, 0.0), smooth(0.1, 15.0, 0.0, 0.0), smooth(0.9, 5.0, 0.0, 0.0), smooth(1.3, 0.0, 0.0, 0.0)]),
    rotation(Jaw, &[smooth(0.0, 15.0, 0.0, 0.0), smooth(0.1, 30.0, 0.0, 0.0), smooth(0.9, 10.0, 0.0, 0.0), smooth(1.3, 0.0, 0.0, 0.0)]),
    rotation(RightArm, &[smooth(0.0, -140.0, 0.0, 25.0), linear(0.1, -60.0, 0.0, -10.0), smooth(0.9, -55.0, 0.0, -10.0), smooth(1.3, 0.0, 0.0, 0.0)]),
    rotation(LeftArm, &[smooth(0.0, -140.0, 0.0, -25.0), linear(0.1, -60.0, 0.0, 10.0), smooth(0.9, -55.0, 0.0, 10.0), smooth(1.3, 0.0, 0.0, 0.0)]),
    rotation(RightLeg, &[smooth(0.0, -30.0, 0.0, 0.0), smooth(0.1, -30.0, 0.0, 0.0), smooth(0.9, -25.0, 0.0, 0.0), smooth(1.3, 0.0, 0.0, 0.0)]),
    rotation(LeftLeg, &[smooth(0.0, -30.0, 0.0, 0.0), smooth(0.1, -30.0, 0.0, 0.0), smooth(0.9, -25.0, 0.0, 0.0), smooth(1.3, 0.0, 0.0, 0.0)]),
];

/// The part indices the animation drives, looked up once.
pub struct ApeRig {
    bones: [usize; 10],
    held_boulder: usize,
    /// A slightly oversized head reads better at a distance on Kong.
    head_scale: f32,
    /// How fast its legs cycle for the distance it walks.
    stride_scale: f32,
}

impl ApeRig {
    pub fn new(model: &Model, species: &Species, head_scale: f32) -> Result<Self, String> {
        let find = |name: &str| model.find_named(name).ok_or_else(|| format!("{}'s model has no part {name}", species.name));
        let mut bones = [0; 10];
        for (bone, name) in BONES {
            bones[bone as usize] = find(name)?;
        }
        let scale = species.ape.map_or(1.0, |ape| ape.scale) as f32;
        Ok(Self { bones, held_boulder: find("held_boulder")?, head_scale, stride_scale: STRIDE_ANIMATION_SCALE / scale })
    }

    /// Poses the model for one frame.
    pub fn pose(&self, model: &Model, s: &AnimState) -> Vec<PartPose> {
        let mut p = model.rest_pose();
        let part = |bone: Bone| self.bones[bone as usize];
        let age = s.age_ticks;

        let head = &mut p[part(Head)];
        (head.x_scale, head.y_scale, head.z_scale) = (self.head_scale, self.head_scale, self.head_scale);
        // The head follows its look.
        head.y_rot += s.head_yaw.clamp(-45.0, 45.0).to_radians() * 0.8;
        head.x_rot += s.head_pitch.clamp(-30.0, 30.0).to_radians() * 0.6;

        // Heavy breathing.
        let breath = (age * 0.09).sin();
        p[part(Body)].x_rot += breath * 0.015;
        p[part(Hips)].y += breath * 0.25;
        p[part(RightArm)].z_rot += breath * 0.02;
        p[part(LeftArm)].z_rot -= breath * 0.02;
        p[part(Jaw)].x_rot += (age * 0.045).sin().max(0.0) * 0.06;

        // A slow, rolling bipedal gait with long arm swings.
        let stride = s.walk_position * self.stride_scale * 0.55;
        let amount = (s.walk_speed * 1.6).min(1.0);
        let swing = stride.cos();
        p[part(RightLeg)].x_rot += swing * 0.6 * amount;
        p[part(LeftLeg)].x_rot -= swing * 0.6 * amount;
        p[part(RightArm)].x_rot -= swing * 0.55 * amount;
        p[part(LeftArm)].x_rot += swing * 0.55 * amount;
        p[part(RightForearm)].x_rot -= (-swing).max(0.0) * 0.35 * amount;
        p[part(LeftForearm)].x_rot -= swing.max(0.0) * 0.35 * amount;
        p[part(Hips)].y -= stride.sin().abs() * 0.9 * amount;
        p[part(Body)].z_rot += stride.sin() * 0.06 * amount;
        p[part(Body)].y_rot += stride.sin() * 0.08 * amount;
        p[part(Head)].z_rot -= stride.sin() * 0.05 * amount;

        let channels: &[Channel] = match s.movement {
            Move::Roar => &ROAR,
            Move::Swipe => &SWIPE,
            Move::Slam => &SLAM,
            Move::Boulder => &BOULDER,
            Move::Leap => &LEAP,
            Move::Land => &LAND,
            _ => &[],
        };
        let seconds = s.move_time / 20.0;
        for channel in channels {
            let value = sample(channel.keys, seconds);
            let pose = &mut p[part(channel.bone)];
            if channel.rotation {
                pose.x_rot += value[0].to_radians();
                pose.y_rot += value[1].to_radians();
                pose.z_rot += value[2].to_radians();
            } else {
                // `KeyframeAnimations.posVec`: authored with Y up, the model's Y points down.
                pose.x += value[0];
                pose.y -= value[1];
                pose.z += value[2];
            }
        }

        // The boulder shows in its hands from when it tears it up until it lets go.
        let release = ape::timing(Move::Boulder).impact as f32;
        p[self.held_boulder].visible = s.movement == Move::Boulder && (ape::BOULDER_PICKUP_TICK..release).contains(&s.move_time);
        p
    }
}

/// A channel's value `seconds` in (`KeyframeAnimation.Entry.apply`): between the keyframes either
/// side, eased the way the later one says; held at the last one after the end.
fn sample(keys: &[Key], seconds: f32) -> [f32; 3] {
    let after = keys.iter().position(|k| seconds <= k.time).unwrap_or(keys.len());
    let a = after.saturating_sub(1);
    let b = (a + 1).min(keys.len() - 1);
    let t = if a == b { 0.0 } else { ((seconds - keys[a].time) / (keys[b].time - keys[a].time)).clamp(0.0, 1.0) };
    if keys[b].linear {
        return std::array::from_fn(|i| keys[a].value[i] + (keys[b].value[i] - keys[a].value[i]) * t);
    }
    let before = keys[a.saturating_sub(1)].value;
    let beyond = keys[(b + 1).min(keys.len() - 1)].value;
    std::array::from_fn(|i| catmull_rom(t, before[i], keys[a].value[i], keys[b].value[i], beyond[i]))
}

/// `Mth.catmullrom`.
fn catmull_rom(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyframes_are_met_and_held() {
        let keys = [smooth(0.0, 0.0, 0.0, 0.0), smooth(0.5, 10.0, 0.0, 0.0), linear(1.0, 30.0, 0.0, 0.0)];
        assert_eq!(sample(&keys, 0.0), [0.0; 3]);
        assert!((sample(&keys, 0.5)[0] - 10.0).abs() < 1e-5);
        assert!((sample(&keys, 0.75)[0] - 20.0).abs() < 1e-5, "a straight line into a linear keyframe");
        assert_eq!(sample(&keys, 4.0), [30.0, 0.0, 0.0], "held at the end");
    }

    #[test]
    fn each_move_ends_where_the_next_begins() {
        // The leap ends in the pose the landing starts from.
        for channel in &LAND {
            let leap = LEAP.iter().find(|c| c.bone as usize == channel.bone as usize && c.rotation == channel.rotation);
            let end = leap.map_or([0.0; 3], |c| c.keys[c.keys.len() - 1].value);
            assert_eq!(end, channel.keys[0].value);
        }
        // The others end at rest.
        for channels in [&ROAR[..], &SWIPE, &SLAM, &BOULDER, &LAND] {
            for c in channels {
                assert_eq!(c.keys[c.keys.len() - 1].value, [0.0; 3]);
            }
        }
    }
}
