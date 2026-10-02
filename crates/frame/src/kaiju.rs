use bevy::prelude::*;

/// ZillaCraft's kaiju on the Minecraft map: what the console asked for (`kaiju godzilla
/// [distance]`), spawned by the Minecraft world on its next update, and the one being fought,
/// for its boss bar.
#[derive(Resource, Default)]
pub struct KaijuSummons {
    /// (kind, how many blocks in front of the player).
    pub pending: Vec<(String, f64)>,
    /// Its name and the share of its health left.
    pub boss: Option<(String, f32)>,
}
