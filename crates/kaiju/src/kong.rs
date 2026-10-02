//! Kong and King Kong: a territorial giant ape, Kong as tall as Zilla and King Kong as big as
//! Godzilla, with the same moves at their own sizes (see `ape`). Each ignores players who keep
//! their distance, warns anyone who comes near with a chest-beating roar, and fights whoever
//! comes closer or hurts it. Below half health it roars once more and is enraged: its eyes glow,
//! its shockwaves reach further, it throws two boulders and rests less between moves.

use crate::ape::{self, Ape, DESIGN_HEIGHT};
use crate::species::Species;

/// Kong is the design-size ape scaled down to Zilla's 14.5 blocks.
const KONG_SCALE: f64 = 14.5 / DESIGN_HEIGHT;
const KING_KONG_SCALE: f64 = 50.0 / DESIGN_HEIGHT;

/// `BossEvent.BossBarColor.GREEN`, and `RED` once enraged.
pub const BOSS_COLOR: [f32; 3] = [0.12, 0.8, 0.18];
pub const ENRAGED_BOSS_COLOR: [f32; 3] = [0.85, 0.12, 0.12];

pub const KONG: Species = Species {
    id: "zillacraft:kong",
    name: "Kong",
    voice: "kong",
    height: 14.5,
    width: 10.0 * KONG_SCALE,
    column_height: 14.5,
    eye_height: 20.0 * KONG_SCALE,
    max_health: 300.0,
    armor: 6.0,
    toughness: 0.0,
    speed: ape::CHASE_SPEED,
    // `ape::SCAN_ABOVE` at his size.
    step_height: 8,
    turn_rate: 6.0,
    follow_range: 96.0 * KONG_SCALE,
    // No claws: an ape only hurts with its moves.
    claw_damage: 0.0,
    claw_knockback: 0.0,
    claw_reach: 0.0,
    // He wades through trees and foliage without breaking anything.
    crush_budget: 0,
    crush_hardness: 0.0,
    // `xpReward`: 150 at the design size.
    experience: (150.0 * KONG_SCALE) as i32,
    // His fur has no item here.
    loot: &[("minecraft:leather", 6, 12), ("minecraft:bone", 4, 8)],
    tail: None,
    stomp: None,
    bite: None,
    roar_ticks: 50,
    tail_cooldown: (0, 0),
    stomp_cooldown: (0, 0),
    bite_cooldown: (0, 0),
    roar_cooldown: ape::WARNING_MEMORY_TICKS,
    move_recovery: ape::REST_TICKS,
    breathes: false,
    hitboxes: &crate::hitboxes::KONG,
    // `getSoundVolume` (6 at the design size); his voice is a little higher than the design ape's.
    move_volume: (6.0 * KONG_SCALE) as f32,
    move_pitch: 1.15,
    voice_volume: (6.0 * KONG_SCALE) as f32,
    voice_pitch: 0.8 * 1.15,
    boss_color: BOSS_COLOR,
    ape: Some(Ape { scale: KONG_SCALE, damage_scale: 1.0, hands: (6.5 * KONG_SCALE, 25.0 * KONG_SCALE) }),
};

/// King Kong: Kong at Godzilla's size, tougher, hitting harder and with a deeper voice. His moves
/// are telegraphed exactly as long as Kong's, and he walks no faster.
pub const KING_KONG: Species = Species {
    id: "zillacraft:king_kong",
    name: "King Kong",
    height: 50.0,
    width: 20.8,
    column_height: 50.0,
    eye_height: 45.0,
    max_health: 800.0,
    armor: 10.0,
    step_height: 25,
    follow_range: 96.0 * KING_KONG_SCALE,
    experience: (150.0 * KING_KONG_SCALE) as i32,
    loot: &[("minecraft:leather", 16, 28), ("minecraft:bone", 10, 18)],
    hitboxes: &crate::hitboxes::KING_KONG,
    move_volume: (6.0 * KING_KONG_SCALE) as f32,
    move_pitch: 0.72,
    voice_volume: (6.0 * KING_KONG_SCALE) as f32,
    voice_pitch: 0.8 * 0.72,
    ape: Some(Ape { scale: KING_KONG_SCALE, damage_scale: 1.5, hands: (5.0, 64.0) }),
    ..KONG
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn king_kong_is_kong_scaled_to_godzillas_height() {
        let (kong, king) = (KONG.ape.unwrap(), KING_KONG.ape.unwrap());
        assert!((kong.s(DESIGN_HEIGHT) - KONG.height).abs() < 1e-9);
        assert!((king.s(DESIGN_HEIGHT) - KING_KONG.height).abs() < 1e-9);
        assert!((KONG.width * king.scale / kong.scale - KING_KONG.width).abs() < 0.1);
        // Eyes near the top of the head.
        const { assert!(KONG.eye_height > KONG.height * 0.8 && KING_KONG.eye_height > KING_KONG.height * 0.8) };
        assert!(KING_KONG.max_health > KONG.max_health && KING_KONG.armor > KONG.armor && king.damage_scale > kong.damage_scale);
        // Each looks up for ground as far as `SCAN_ABOVE` at its size.
        for species in [&KONG, &KING_KONG] {
            let ape = species.ape.unwrap();
            assert_eq!(species.step_height, ape.s(ape::SCAN_ABOVE).ceil() as i32, "{}", species.name);
            assert!(ape.hands.1 > species.eye_height, "{}: a lifted boulder is held above its head", species.name);
        }
    }
}
