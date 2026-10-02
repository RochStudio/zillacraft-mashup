//! The ZillaCraft resource pack the Minecraft map loads beside Minecraft's own: Godzilla's
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
    "assets/zillacraft/sounds/entity/godzilla/ambient1.ogg" => "../assets/sounds/godzilla/ambient1.ogg",
    "assets/zillacraft/sounds/entity/godzilla/ambient2.ogg" => "../assets/sounds/godzilla/ambient2.ogg",
    "assets/zillacraft/sounds/entity/godzilla/breath_charge.ogg" => "../assets/sounds/godzilla/breath_charge.ogg",
    "assets/zillacraft/sounds/entity/godzilla/breath_fire.ogg" => "../assets/sounds/godzilla/breath_fire.ogg",
    "assets/zillacraft/sounds/entity/godzilla/death.ogg" => "../assets/sounds/godzilla/death.ogg",
    "assets/zillacraft/sounds/entity/godzilla/growl.ogg" => "../assets/sounds/godzilla/growl.ogg",
    "assets/zillacraft/sounds/entity/godzilla/hurt1.ogg" => "../assets/sounds/godzilla/hurt1.ogg",
    "assets/zillacraft/sounds/entity/godzilla/hurt2.ogg" => "../assets/sounds/godzilla/hurt2.ogg",
    "assets/zillacraft/sounds/entity/godzilla/roar.ogg" => "../assets/sounds/godzilla/roar.ogg",
    "assets/zillacraft/sounds/entity/kaiju/crush.ogg" => "../assets/sounds/kaiju/crush.ogg",
    "assets/zillacraft/sounds/entity/kaiju/step1.ogg" => "../assets/sounds/kaiju/step1.ogg",
    "assets/zillacraft/sounds/entity/kaiju/step2.ogg" => "../assets/sounds/kaiju/step2.ogg",
    "assets/zillacraft/sounds/entity/kaiju/stomp.ogg" => "../assets/sounds/kaiju/stomp.ogg",
    "assets/zillacraft/sounds/entity/kaiju/tail_swipe.ogg" => "../assets/sounds/kaiju/tail_swipe.ogg",
};

pub const SKIN: &str = "zillacraft:entity/godzilla";
pub const GLOW: &str = "zillacraft:entity/godzilla_glow";
pub const BEAM: &str = "zillacraft:entity/beam";

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
