//! ZillaCraft's kaiju for the Minecraft map, ported from the Forge mod: Godzilla's model and
//! texture (converted from the mod's generated geometry), his animation, his moves and stats.

pub mod anim;
pub mod brain;
pub mod combat;
pub mod godzilla;
pub mod hitboxes;
pub mod model;
pub mod pack;

pub const GODZILLA_MODEL: &str = include_str!("../assets/godzilla.json");
pub const GODZILLA_TEXTURE: &[u8] = include_bytes!("../assets/godzilla.png");
pub const GODZILLA_GLOW_TEXTURE: &[u8] = include_bytes!("../assets/godzilla_glow.png");
