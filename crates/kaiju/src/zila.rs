//! Zilla (Zila in the mod): Godzilla as he was on Odo Island in Godzilla Minus One, before the
//! bomb tests made him a giant. A 14.5-block, animal-like kaiju that leans forward like a
//! dinosaur. No atomic breath yet: it bites and flings what it catches aside, it has the tail
//! swipe and the stomp of all kaiju, and it roars at whatever it first sets eyes on. Quicker
//! than Godzilla, but every move still has a clear wind-up.

use crate::combat::{Bite, Segment, Stomp, Tail};
use crate::species::Species;

pub const HEIGHT: f64 = 14.5;

/// Its tail leaves the hips about 7.5 blocks up, runs back and down to the ground, and flicks
/// its tip up. These are also the tail's hitboxes, and match the model.
pub const TAIL_SEGMENTS: [Segment; 8] = [
    Segment::new(2.8, 3.13, -3.21, 5.57, 0.0),
    Segment::new(2.5, 3.22, -5.47, 4.82, 0.01),
    Segment::new(2.3, 3.13, -7.49, 3.76, 0.2),
    Segment::new(2.3, 3.03, -9.24, 2.46, 0.42),
    Segment::new(2.2, 2.7, -10.88, 1.24, 0.81),
    Segment::new(2.1, 2.19, -12.62, 0.37, 1.1),
    Segment::new(2.0, 1.57, -14.5, 0.0, 1.41),
    Segment::new(1.8, 1.05, -16.37, 0.13, 1.54),
];

/// A 1.25 s wind-up, then the tail whips round behind it, up to ~15 blocks from its hips, for 12.
pub const TAIL: Tail = Tail {
    segments: &TAIL_SEGMENTS,
    pivot_back: 2.1,
    windup: 25,
    sweep: 10,
    recover: 15,
    arc: 115.0 * std::f64::consts::PI / 180.0,
    off_nose: 70.0 * std::f64::consts::PI / 180.0,
    damage: 12.0,
    knockback: 1.6,
    lift: 0.5,
};

/// A 1.1 s wind-up, then a shockwave rolls out 11 blocks: 12 under the foot, 6 at the edge.
pub const STOMP: Stomp = Stomp {
    windup: 22,
    total: 40,
    foot_forward: 1.2,
    foot_side: 2.0,
    radius: 11.0,
    trigger: 8.0,
    wave_start: 2.0,
    wave_speed: 2.0,
    damage: 12.0,
    knockback: 1.0,
    lift: 0.4,
};

/// It rears back with its jaws open (0.8 s), then snaps at the ground 5 to 13.5 blocks in front
/// of it, up to 2.6 blocks either side of its nose: 15, and what it catches is flung aside.
pub const BITE: Bite = Bite {
    windup: 16,
    snap: 4,
    recover: 14,
    min_forward: 5.0,
    max_forward: 13.5,
    half_width: 2.6,
    max_height: 9.0,
    damage: 15.0,
    fling: 1.3,
    fling_lift: 0.9,
};

/// How long it roars at something it has just set eyes on. It does no harm.
pub const ROAR_TICKS: u32 = 40;

pub const SPECIES: Species = Species {
    id: "zillacraft:zila",
    name: "Zilla",
    voice: "zila",
    height: HEIGHT,
    width: 6.0,
    column_height: 9.0,
    // `Entity.getEyeHeight`: 0.85 of its 9-block column.
    eye_height: 7.65,
    max_health: 400.0,
    armor: 12.0,
    toughness: 6.0,
    speed: 0.34,
    step_height: 3,
    // It turns faster than Godzilla, but still like a big animal.
    turn_rate: 6.0,
    follow_range: 64.0,
    claw_damage: 14.0,
    // No knockback of its own: a mob's hit throws as hard as any (`LivingEntity.knockback`, 0.4).
    claw_knockback: 0.4,
    claw_reach: 2.5,
    // It smashes through wood, earth and stone, but not metal or anything tougher.
    crush_budget: 60,
    crush_hardness: 3.0,
    experience: 400,
    // Its scales are all it drops in the mod, and they have no item here.
    loot: &[],
    tail: Some(TAIL),
    stomp: Some(STOMP),
    bite: Some(BITE),
    roar_ticks: ROAR_TICKS,
    tail_cooldown: (110, 50),
    stomp_cooldown: (80, 40),
    bite_cooldown: (30, 20),
    roar_cooldown: 600,
    move_recovery: 10,
    breathes: false,
    hitboxes: &crate::hitboxes::ZILA,
    move_volume: 4.0,
    move_pitch: 1.35,
    voice_volume: 2.5,
    voice_pitch: 1.0,
    // `BossEvent.BossBarColor.GREEN`.
    boss_color: [0.12, 0.8, 0.18],
    enraged_boss_color: [0.12, 0.8, 0.18],
    ape: None,
};
