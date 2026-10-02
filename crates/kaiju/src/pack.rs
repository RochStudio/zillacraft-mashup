//! The ZillaCraft resource pack the Minecraft map loads beside Minecraft's own: the kaiju's
//! textures and sounds from the mod, plus the breath beam's texture.

macro_rules! files {
    ($($path:literal => $source:literal),* $(,)?) => {
        &[$(($path, include_bytes!($source))),*]
    };
}

/// Every file of the pack: (path inside the pack, contents).
pub const FILES: &[(&str, &[u8])] = files! {
    "pack.mcmeta" => "../assets/pack.mcmeta",
    "assets/zillacraft/sounds.json" => "../assets/sounds.json",
    "assets/zillacraft/textures/entity/godzilla.png" => "../assets/godzilla.png",
    "assets/zillacraft/textures/entity/godzilla_glow.png" => "../assets/godzilla_glow.png",
    "assets/zillacraft/textures/entity/beam.png" => "../assets/beam.png",
    "assets/zillacraft/textures/entity/zila.png" => "../assets/zila.png",
    "assets/zillacraft/textures/entity/kong.png" => "../assets/kong.png",
    "assets/zillacraft/textures/entity/kong_eyes.png" => "../assets/kong_eyes.png",
    "assets/zillacraft/textures/entity/king_kong.png" => "../assets/king_kong.png",
    "assets/zillacraft/textures/entity/king_kong_eyes.png" => "../assets/king_kong_eyes.png",
    "assets/zillacraft/textures/entity/boulder.png" => "../assets/boulder.png",
    "assets/zillacraft/sounds/entity/godzilla/ambient1.ogg" => "../assets/sounds/godzilla/ambient1.ogg",
    "assets/zillacraft/sounds/entity/godzilla/ambient2.ogg" => "../assets/sounds/godzilla/ambient2.ogg",
    "assets/zillacraft/sounds/entity/godzilla/breath_charge.ogg" => "../assets/sounds/godzilla/breath_charge.ogg",
    "assets/zillacraft/sounds/entity/godzilla/breath_fire.ogg" => "../assets/sounds/godzilla/breath_fire.ogg",
    "assets/zillacraft/sounds/entity/godzilla/death.ogg" => "../assets/sounds/godzilla/death.ogg",
    "assets/zillacraft/sounds/entity/godzilla/growl.ogg" => "../assets/sounds/godzilla/growl.ogg",
    "assets/zillacraft/sounds/entity/godzilla/hurt1.ogg" => "../assets/sounds/godzilla/hurt1.ogg",
    "assets/zillacraft/sounds/entity/godzilla/hurt2.ogg" => "../assets/sounds/godzilla/hurt2.ogg",
    "assets/zillacraft/sounds/entity/godzilla/roar.ogg" => "../assets/sounds/godzilla/roar.ogg",
    "assets/zillacraft/sounds/entity/zila/ambient1.ogg" => "../assets/sounds/zila/ambient1.ogg",
    "assets/zillacraft/sounds/entity/zila/ambient2.ogg" => "../assets/sounds/zila/ambient2.ogg",
    "assets/zillacraft/sounds/entity/zila/death.ogg" => "../assets/sounds/zila/death.ogg",
    "assets/zillacraft/sounds/entity/zila/growl.ogg" => "../assets/sounds/zila/growl.ogg",
    "assets/zillacraft/sounds/entity/zila/hurt1.ogg" => "../assets/sounds/zila/hurt1.ogg",
    "assets/zillacraft/sounds/entity/zila/hurt2.ogg" => "../assets/sounds/zila/hurt2.ogg",
    "assets/zillacraft/sounds/entity/zila/roar.ogg" => "../assets/sounds/zila/roar.ogg",
    "assets/zillacraft/sounds/entity/kong/ambient1.ogg" => "../assets/sounds/kong/ambient1.ogg",
    "assets/zillacraft/sounds/entity/kong/ambient2.ogg" => "../assets/sounds/kong/ambient2.ogg",
    "assets/zillacraft/sounds/entity/kong/boulder_impact.ogg" => "../assets/sounds/kong/boulder_impact.ogg",
    "assets/zillacraft/sounds/entity/kong/chest_beat.ogg" => "../assets/sounds/kong/chest_beat.ogg",
    "assets/zillacraft/sounds/entity/kong/death.ogg" => "../assets/sounds/kong/death.ogg",
    "assets/zillacraft/sounds/entity/kong/grunt.ogg" => "../assets/sounds/kong/grunt.ogg",
    "assets/zillacraft/sounds/entity/kong/hurt1.ogg" => "../assets/sounds/kong/hurt1.ogg",
    "assets/zillacraft/sounds/entity/kong/hurt2.ogg" => "../assets/sounds/kong/hurt2.ogg",
    "assets/zillacraft/sounds/entity/kong/leap.ogg" => "../assets/sounds/kong/leap.ogg",
    "assets/zillacraft/sounds/entity/kong/rip.ogg" => "../assets/sounds/kong/rip.ogg",
    "assets/zillacraft/sounds/entity/kong/roar.ogg" => "../assets/sounds/kong/roar.ogg",
    "assets/zillacraft/sounds/entity/kong/slam.ogg" => "../assets/sounds/kong/slam.ogg",
    "assets/zillacraft/sounds/entity/kong/step1.ogg" => "../assets/sounds/kong/step1.ogg",
    "assets/zillacraft/sounds/entity/kong/step2.ogg" => "../assets/sounds/kong/step2.ogg",
    "assets/zillacraft/sounds/entity/kong/swipe.ogg" => "../assets/sounds/kong/swipe.ogg",
    "assets/zillacraft/sounds/entity/kong/throw.ogg" => "../assets/sounds/kong/throw.ogg",
    "assets/zillacraft/sounds/entity/kaiju/bite.ogg" => "../assets/sounds/kaiju/bite.ogg",
    "assets/zillacraft/sounds/entity/kaiju/crush.ogg" => "../assets/sounds/kaiju/crush.ogg",
    "assets/zillacraft/sounds/entity/kaiju/step1.ogg" => "../assets/sounds/kaiju/step1.ogg",
    "assets/zillacraft/sounds/entity/kaiju/step2.ogg" => "../assets/sounds/kaiju/step2.ogg",
    "assets/zillacraft/sounds/entity/kaiju/stomp.ogg" => "../assets/sounds/kaiju/stomp.ogg",
    "assets/zillacraft/sounds/entity/kaiju/tail_swipe.ogg" => "../assets/sounds/kaiju/tail_swipe.ogg",
};

pub const SKIN: &str = "zillacraft:entity/godzilla";
pub const GLOW: &str = "zillacraft:entity/godzilla_glow";
pub const BEAM: &str = "zillacraft:entity/beam";
pub const ZILA_SKIN: &str = "zillacraft:entity/zila";
pub const KONG_SKIN: &str = "zillacraft:entity/kong";
/// Kong's and King Kong's eyes, which glow once they are enraged.
pub const KONG_EYES: &str = "zillacraft:entity/kong_eyes";
pub const KING_KONG_SKIN: &str = "zillacraft:entity/king_kong";
pub const KING_KONG_EYES: &str = "zillacraft:entity/king_kong_eyes";
pub const BOULDER: &str = "zillacraft:entity/boulder";

/// Every texture of the pack, for the entity atlas.
pub const TEXTURES: [&str; 9] = [SKIN, GLOW, BEAM, ZILA_SKIN, KONG_SKIN, KONG_EYES, KING_KONG_SKIN, KING_KONG_EYES, BOULDER];

/// Writes the pack into `dir` (a folder pack), rewriting any file that differs. Returns `dir`.
pub fn install(dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    for (path, bytes) in FILES {
        let target = dir.join(path);
        if std::fs::read(&target).is_ok_and(|existing| existing == *bytes) {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, bytes)?;
    }
    Ok(dir.to_path_buf())
}
