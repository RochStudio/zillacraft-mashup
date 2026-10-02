use bevy::prelude::*;

/// ZillaCraft's kaiju on the Minecraft map: what the console asked for (`kaiju godzilla
/// [distance]`, `kaiju zilla [distance]`), spawned by the Minecraft world on its next update,
/// and the ones near the player, for their boss bars.
#[derive(Resource, Default)]
pub struct KaijuSummons {
    /// (kind, how many blocks in front of the player).
    pub pending: Vec<(String, f64)>,
    /// One bar each, nearest first: its name, the share of its health left, its bar's colour.
    pub bosses: Vec<(String, f32, [f32; 3])>,
}
