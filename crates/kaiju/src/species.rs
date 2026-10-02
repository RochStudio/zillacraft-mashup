//! What sets one kaiju apart from another: its size, its stats, its moves and its voice. The
//! brain, the server and the renderer read everything kind-specific from here.

use crate::combat::{Bite, Stomp, Tail};
use crate::hitboxes::BodyBox;

#[derive(Debug)]
pub struct Species {
    /// Its summon id, and the name on its boss bar.
    pub id: &'static str,
    pub name: &'static str,
    /// Its sounds' group: `zillacraft:entity.<voice>.roar` and the rest.
    pub voice: &'static str,
    /// How tall it stands, to the top of its head.
    pub height: f64,
    /// Its main column (its own hitbox): follow-along boxes cover the rest of the body.
    pub width: f64,
    pub column_height: f64,
    /// How high above its feet it sees from.
    pub eye_height: f64,
    pub max_health: f32,
    pub armor: f32,
    pub toughness: f32,
    /// Blocks per tick at full stride.
    pub speed: f64,
    /// It climbs rises this tall without stopping; anything taller it has to crush first.
    pub step_height: i32,
    /// Degrees per tick.
    pub turn_rate: f32,
    pub follow_range: f64,
    /// Its claws for whatever walks into it: damage, how hard they throw, and how close
    /// something must come.
    pub claw_damage: f32,
    pub claw_knockback: f64,
    pub claw_reach: f64,
    /// Blocks crushed per tick at most, and the hardest block it crushes.
    pub crush_budget: u32,
    pub crush_hardness: f32,
    pub experience: i32,
    pub tail: Tail,
    pub stomp: Stomp,
    pub bite: Option<Bite>,
    /// How long it roars at something it has just set eyes on (0: it doesn't roar).
    pub roar_ticks: u32,
    /// Ticks before it makes each move again (plus up to the random extra).
    pub tail_cooldown: (u32, u32),
    pub stomp_cooldown: (u32, u32),
    pub bite_cooldown: (u32, u32),
    pub roar_cooldown: u32,
    /// The pause after a move before the next (none after a roar: it may strike straight after).
    pub move_recovery: u32,
    /// Whether it has the atomic breath.
    pub breathes: bool,
    pub hitboxes: &'static [BodyBox],
    /// Loudness and pitch of its moves' sounds, and how loud its voice is.
    pub move_volume: f32,
    pub move_pitch: f32,
    pub voice_volume: f32,
    pub boss_color: [f32; 3],
}

/// Every kind there is, for looking one up by name.
pub const ALL: [&Species; 2] = [&crate::godzilla::SPECIES, &crate::zila::SPECIES];

/// The kind `name` names: its id or its name, in any case (`zilla`, `zillacraft:zila`).
pub fn named(name: &str) -> Option<&'static Species> {
    let name = name.to_ascii_lowercase();
    ALL.into_iter().find(|s| {
        s.id == name || s.name.to_ascii_lowercase() == name || s.id.rsplit(':').next() == Some(name.as_str())
    })
}
