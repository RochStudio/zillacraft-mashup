//! ZillaCraft's kaiju for the Minecraft map, ported from the Forge mod: Godzilla's, Zilla's,
//! Kong's and King Kong's models and textures (converted from the mod's geometry), their
//! animation, their moves and stats.

pub mod anim;
pub mod ape;
pub mod ape_anim;
pub mod brain;
pub mod combat;
pub mod godzilla;
pub mod hitboxes;
pub mod kong;
pub mod model;
pub mod pack;
pub mod species;
pub mod zila;
pub mod zila_anim;

pub use species::Species;

pub const GODZILLA_MODEL: &str = include_str!("../assets/godzilla.json");
pub const GODZILLA_TEXTURE: &[u8] = include_bytes!("../assets/godzilla.png");
pub const GODZILLA_GLOW_TEXTURE: &[u8] = include_bytes!("../assets/godzilla_glow.png");
pub const ZILA_MODEL: &str = include_str!("../assets/zila.json");
pub const KONG_MODEL: &str = include_str!("../assets/kong.json");
pub const KING_KONG_MODEL: &str = include_str!("../assets/king_kong.json");
/// The boulder an ape throws.
pub const BOULDER_MODEL: &str = include_str!("../assets/boulder.json");
