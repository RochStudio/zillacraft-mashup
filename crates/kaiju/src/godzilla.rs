//! Godzilla (Godzilla Minus One): a 50-block boss with a tail swipe, a stomp, claws for
//! whatever walks into him, and his atomic breath for whatever is in front of him.

use crate::combat::{Segment, Stomp, Tail};
use crate::species::Species;

pub const HEIGHT: f64 = 50.0;
/// His main column; follow-along boxes cover the rest of the body.
pub const WIDTH: f64 = 12.0;
pub const MAX_HEALTH: f32 = 800.0;
pub const ARMOR: f32 = 12.0;
/// Blocks per tick at full stride.
pub const SPEED: f64 = 0.28;
pub const CLAW_DAMAGE: f32 = 24.0;
pub const FOLLOW_RANGE: f64 = 100.0;
/// Degrees per tick: a 50-block monster turns like a ship.
pub const TURN_RATE: f32 = 3.0;
/// How close something must come for his claws to reach it.
pub const CLAW_REACH: f64 = 2.5;
/// Blocks crushed per tick at most, and the hardest block he crushes.
pub const CRUSH_BUDGET: u32 = 300;
pub const CRUSH_HARDNESS: f32 = 10.0;
pub const XP: u32 = 1000;

/// His tail leaves the hips high, sweeps down to the ground and trails off to his left. These
/// are also the tail's hitboxes, and match the model.
pub const TAIL_SEGMENTS: [Segment; 7] = [
    Segment::new(11.0, 13.0, -11.5, 12.8, 0.1),
    Segment::new(10.0, 12.6, -19.0, 8.3, 1.1),
    Segment::new(9.0, 12.1, -24.7, 2.2, 3.1),
    Segment::new(8.0, 7.9, -30.8, 0.0, 5.9),
    Segment::new(7.0, 4.5, -38.7, 0.0, 8.5),
    Segment::new(6.0, 3.5, -47.0, 0.0, 9.4),
    Segment::new(5.0, 3.0, -54.3, 0.0, 8.7),
];

/// A 1.75 s wind-up, then the tail whips round behind him, up to ~49 blocks from his hips, for 18.
pub const TAIL: Tail = Tail {
    segments: &TAIL_SEGMENTS,
    pivot_back: 7.4,
    windup: 35,
    sweep: 14,
    recover: 21,
    arc: 115.0 * std::f64::consts::PI / 180.0,
    off_nose: 70.0 * std::f64::consts::PI / 180.0,
    damage: 18.0,
    knockback: 2.5,
    lift: 0.6,
};

/// A 1.5 s wind-up, then a shockwave rolls out 24 blocks: 16 under the foot, 8 at the edge.
pub const STOMP: Stomp = Stomp {
    windup: 30,
    total: 55,
    foot_forward: 2.0,
    foot_side: 5.7,
    radius: 24.0,
    trigger: 21.0,
    wave_start: 4.0,
    wave_speed: 3.0,
    damage: 16.0,
    knockback: 1.5,
    lift: 0.5,
};

/// Ticks before another tail swipe or stomp (plus up to the random extra), and the pause after any move.
pub const TAIL_COOLDOWN: (u32, u32) = (160, 80);
pub const STOMP_COOLDOWN: (u32, u32) = (120, 60);
pub const MOVE_RECOVERY: u32 = 20;

pub const SPECIES: Species = Species {
    id: "zillacraft:godzilla",
    name: "Godzilla",
    voice: "godzilla",
    height: HEIGHT,
    width: WIDTH,
    column_height: HEIGHT,
    eye_height: breath::MOUTH_UP,
    max_health: MAX_HEALTH,
    armor: ARMOR,
    toughness: 0.0,
    speed: SPEED,
    step_height: 6,
    turn_rate: TURN_RATE,
    follow_range: FOLLOW_RANGE,
    claw_damage: CLAW_DAMAGE,
    // His `ATTACK_KNOCKBACK` of 3, as `Mob.doHurtTarget` halves it.
    claw_knockback: 1.5,
    claw_reach: CLAW_REACH,
    crush_budget: CRUSH_BUDGET,
    crush_hardness: CRUSH_HARDNESS,
    experience: XP as i32,
    // The mod's scales have no item here: diamonds and emeralds instead.
    loot: &[("minecraft:diamond", 16, 32), ("minecraft:emerald", 16, 32)],
    tail: Some(TAIL),
    stomp: Some(STOMP),
    bite: None,
    roar_ticks: 0,
    tail_cooldown: TAIL_COOLDOWN,
    stomp_cooldown: STOMP_COOLDOWN,
    bite_cooldown: (0, 0),
    roar_cooldown: 0,
    move_recovery: MOVE_RECOVERY,
    breathes: true,
    hitboxes: &crate::hitboxes::GODZILLA,
    move_volume: 6.0,
    move_pitch: 1.0,
    voice_volume: 4.0,
    voice_pitch: 1.0,
    // `BossEvent.BossBarColor.RED`.
    boss_color: [0.85, 0.12, 0.12],
    ape: None,
};

/// The atomic breath: a 2.25 s charge while his plates light up, then almost 4 s of heat ray.
pub mod breath {
    pub const WINDUP_TICKS: u32 = 45;
    pub const TOTAL_TICKS: u32 = 120;
    pub const DAMAGE: f32 = 22.0;
    pub const BURN_SECONDS: f32 = 10.0;
    /// Anything closer than this is under his chin and gets claws and teeth instead...
    pub const MIN_DIST: f64 = 20.0;
    /// ...unless it is a giant, whose head is level with his mouth however close it is.
    pub const GIANT_HEIGHT: f64 = 30.0;
    pub const MAX_RANGE: f64 = 130.0;
    /// He breathes at what is in front of him, never over his shoulder.
    pub const MAX_OFF_NOSE: f64 = 75.0 * std::f64::consts::PI / 180.0;
    /// Where the beam leaves: his mouth, this far ahead of his feet along his head's facing, and up.
    pub const MOUTH_FORWARD: f64 = 16.0;
    pub const MOUTH_UP: f64 = 45.0;
    /// How hard the beam throws what it hits away from him (blocks per tick), and how high.
    pub const KNOCKBACK: f64 = 2.8;
    pub const KNOCKBACK_LIFT: f64 = 0.9;
    /// Where it strikes the ground: a blast this strong (TNT is 4) every so many ticks.
    pub const BLAST_POWER: f32 = 9.0;
    pub const BLAST_INTERVAL: u32 = 2;
    /// Ticks before he breathes again (plus up to the random extra); he starts with a short wait.
    pub const COOLDOWN: (u32, u32) = (400, 200);
    pub const FIRST_COOLDOWN: u32 = 60;
    pub const BEAM_RADIUS: f64 = 5.95;
}
