//! Inventory icons and names for items, as MinecraftOSS's viewer makes them
//! (`engine/viewer/src/interface.rs`: `load_catalog_icons`, `item_name`).
use crate::interface::{item_model_reference, item_model_tints};
use crate::pack::{PackStack, ResourceId};
use anyhow::Result;
use image::{imageops::FilterType, RgbaImage};
use std::collections::HashMap;

/// An item's GUI icon: its block model rastered, its generated layers, or
/// its flat texture.
pub fn item_icon(packs: &PackStack, key: &str, icon_size: usize) -> Result<Option<RgbaImage>> {
    let id = ResourceId::parse(key)?;
    let definition = packs.item_definition(&id)?;
    if let Some(icon) = definition
        .as_ref()
        .map(|value| special_icon(packs, &value["model"], icon_size))
        .transpose()?
        .flatten()
    {
        return Ok(Some(icon));
    }
    let model = definition.as_ref().and_then(|value| item_model_reference(&value["model"]));
    let tints = definition
        .as_ref()
        .map(|value| item_model_tints(packs, &value["model"]))
        .transpose()?
        .unwrap_or_default();
    if let Some(icon) = model
        .map(|model| crate::item_icon::block_icon(packs, model, &tints, icon_size))
        .transpose()?
        .flatten()
    {
        return Ok(Some(icon));
    }
    if let Some(icon) = model
        .map(|model| generated_item_icon(packs, model, &tints, icon_size))
        .transpose()?
        .flatten()
    {
        return Ok(Some(icon));
    }
    let texture = model.map(|model| model_texture(packs, model)).transpose()?.flatten();
    let texture = texture.or_else(|| ResourceId::parse(&format!("{}:item/{}", id.namespace, id.path)).ok());
    let Some(texture) = texture else { return Ok(None) };
    let Some(bytes) = packs.texture(&texture)? else { return Ok(None) };
    let decoded = image::load_from_memory(&bytes)?.to_rgba8();
    let side = decoded.width().min(decoded.height());
    let first = image::imageops::crop_imm(&decoded, 0, 0, side, side).to_image();
    Ok(Some(image::imageops::resize(&first, icon_size as u32, icon_size as u32, FilterType::Nearest)))
}

/// The icon of an item vanilla draws with a special renderer, for the kinds built here as
/// the boxes that renderer draws: chests (on each kind's sheet) and banners (in each color).
/// The model is the one shown outside any season or condition (a `select`'s fallback).
fn special_icon(packs: &PackStack, model: &serde_json::Value, icon_size: usize) -> Result<Option<RgbaImage>> {
    let mut node = model;
    for _ in 0..8 {
        node = match node["type"].as_str() {
            Some("minecraft:special") => break,
            Some("minecraft:select" | "minecraft:range_dispatch") => &node["fallback"],
            Some("minecraft:condition") => &node["on_false"],
            _ => return Ok(None),
        };
    }
    let (Some("minecraft:special"), Some(base)) = (node["type"].as_str(), node["base"].as_str()) else {
        return Ok(None);
    };
    let special = &node["model"];
    let (model, tints) = match special["type"].as_str() {
        Some("minecraft:chest") => {
            let sheet = ResourceId::parse(special["texture"].as_str().unwrap_or("minecraft:normal"))?;
            let texture = ResourceId::parse(&format!("{}:entity/chest/{}", sheet.namespace, sheet.path))?;
            (crate::model::chest_model(&texture, "single", "south"), Vec::new())
        }
        Some("minecraft:banner") => (banner_model()?, vec![dye_color(special["color"].as_str().unwrap_or("white"))]),
        _ => return Ok(None),
    };
    let pose = crate::item_icon::GuiPose::of(packs, base)?;
    crate::item_icon::elements_icon(packs, &model.elements, &tints, &pose, icon_size)
}

/// The standing banner's boxes (`BannerModel`'s pole and bar, `BannerFlagModel`'s flag) as
/// its item transformation places them: two thirds size about the block's middle, Y and Z
/// turned over. The flag takes tint 0, the banner's color.
fn banner_model() -> Result<crate::model::ResolvedModel> {
    let sheet = ResourceId::parse("minecraft:entity/banner/banner_base")?;
    let third = |n: f32| n / 3.0;
    // From and to in block pixels; the part's texture offset; its model width, height, depth.
    let parts = [
        ([third(22.0), 0.0, third(22.0)], [third(26.0), 28.0, third(26.0)], [44.0, 0.0], [2.0, 42.0, 2.0], false),
        ([third(4.0), 28.0, third(22.0)], [third(44.0), third(88.0), third(26.0)], [0.0, 42.0], [20.0, 2.0, 2.0], false),
        ([third(4.0), third(8.0), third(26.0)], [third(44.0), third(88.0), third(28.0)], [0.0, 0.0], [20.0, 40.0, 1.0], true),
    ];
    let elements = parts
        .into_iter()
        .map(|(from, to, [u, v], [w, h, d], flag): ([f32; 3], [f32; 3], [f32; 2], [f32; 3], bool)| {
            // `ModelPart.Cube`'s sheet layout, with Y and Z turned over as the banner is.
            let faces = [
                ("up", [u + d, v + d, u + d + w, v]),
                ("down", [u + d + w, v + d, u + d + 2.0 * w, v]),
                ("south", [u + d, v + d, u + d + w, v + d + h]),
                ("north", [u + 2.0 * d + w, v + d, u + 2.0 * d + 2.0 * w, v + d + h]),
                ("west", [u, v + d, u + d, v + d + h]),
                ("east", [u + d + w, v + d, u + 2.0 * d + w, v + d + h]),
            ];
            crate::model::Element {
                from: from.map(|c| c / 16.0),
                to: to.map(|c| c / 16.0),
                faces: faces
                    .into_iter()
                    .map(|(direction, uv)| crate::model::Face {
                        direction: direction.into(),
                        texture: sheet.clone(),
                        uv: uv.map(|c| c / 64.0),
                        cull: false,
                        cullface: None,
                        tint: flag,
                        tint_index: flag.then_some(0),
                        force_translucent: false,
                    })
                    .collect(),
                rotation_y: 0,
                rotation: None,
                shade_direction_override: None,
            }
        })
        .collect();
    Ok(crate::model::ResolvedModel { elements })
}

/// `DyeColor`'s texture colors.
fn dye_color(name: &str) -> [u8; 3] {
    let rgb: u32 = match name.trim_start_matches("minecraft:") {
        "orange" => 0xF9801D,
        "magenta" => 0xC74EBD,
        "light_blue" => 0x3AB3DA,
        "yellow" => 0xFED83D,
        "lime" => 0x80C71F,
        "pink" => 0xF38BAA,
        "gray" => 0x474F52,
        "light_gray" => 0x9D9D97,
        "cyan" => 0x169C9C,
        "purple" => 0x8932B8,
        "blue" => 0x3C44AA,
        "brown" => 0x835432,
        "green" => 0x5E7C16,
        "red" => 0xB02E26,
        "black" => 0x1D1D21,
        _ => 0xF9FFFE,
    };
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
}

/// The pack's English names (`lang/en_us.json`).
pub fn language(packs: &PackStack) -> Result<HashMap<String, String>> {
    let id = ResourceId::parse("minecraft:lang/en_us")?;
    let Some(value) = packs.json(&id, "lang/en_us.json")? else {
        return Ok(HashMap::new());
    };
    Ok(value
        .as_object()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(key, text)| Some((key.clone(), text.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default())
}

/// `item_name`: the item's or its block's name, else its id's path.
pub fn item_name(language: &HashMap<String, String>, id: &str) -> String {
    let path = id.split(':').nth(1).unwrap_or(id);
    language
        .get(&format!("item.minecraft.{path}"))
        .or_else(|| language.get(&format!("block.minecraft.{path}")))
        .cloned()
        .unwrap_or_else(|| path.replace('_', " "))
}


/// Item/generated renders each layer in order; layer 0's tint is the liquid,
/// and the untinted bottle on layer 1 is drawn over it.
fn generated_item_icon(
    packs: &PackStack,
    model: &str,
    tints: &[[u8; 3]],
    icon_size: usize,
) -> Result<Option<RgbaImage>> {
    let mut textures = HashMap::<String, String>::new();
    let mut current = Some(ResourceId::parse(model)?);
    for _ in 0..12 {
        let Some(id) = current.take() else { break };
        let Some(value) = packs.model(&id)? else {
            break;
        };
        if let Some(entries) = value.get("textures").and_then(serde_json::Value::as_object) {
            for (key, texture) in entries {
                if let Some(texture) = texture
                    .as_str()
                    .or_else(|| texture.get("sprite").and_then(serde_json::Value::as_str))
                {
                    textures
                        .entry(key.clone())
                        .or_insert_with(|| texture.into());
                }
            }
        }
        current = value
            .get("parent")
            .and_then(serde_json::Value::as_str)
            .map(ResourceId::parse)
            .transpose()?;
    }
    if !textures.contains_key("layer1") {
        return Ok(None);
    }
    let mut icon = RgbaImage::new(icon_size as u32, icon_size as u32);
    for layer in 0..16 {
        let key = format!("layer{layer}");
        let Some(mut texture) = textures.get(&key).map(String::as_str) else {
            break;
        };
        for _ in 0..8 {
            if let Some(reference) = texture.strip_prefix('#') {
                let Some(next) = textures.get(reference) else {
                    break;
                };
                texture = next;
            } else {
                break;
            }
        }
        let Some(bytes) = packs.texture(&ResourceId::parse(texture)?)? else {
            continue;
        };
        let source = image::load_from_memory(&bytes)?.to_rgba8();
        let side = source.width().min(source.height());
        if side == 0 {
            continue;
        }
        let tint = tints.get(layer).copied().unwrap_or([255; 3]);
        let mut frame = image::imageops::crop_imm(&source, 0, 0, side, side).to_image();
        for pixel in frame.pixels_mut() {
            for channel in 0..3 {
                pixel[channel] = (pixel[channel] as u16 * tint[channel] as u16 / 255) as u8;
            }
        }
        let scaled = image::imageops::resize(
            &frame,
            icon_size as u32,
            icon_size as u32,
            FilterType::Nearest,
        );
        image::imageops::overlay(&mut icon, &scaled, 0, 0);
    }
    Ok((icon.pixels().any(|pixel| pixel[3] > 0)).then_some(icon))
}

fn model_texture(packs: &PackStack, model: &str) -> Result<Option<ResourceId>> {
    let mut textures = HashMap::<String, String>::new();
    let mut current = Some(ResourceId::parse(model)?);
    for _ in 0..12 {
        let Some(id) = current.take() else { break };
        let Some(value) = packs.model(&id)? else {
            break;
        };
        if let Some(entries) = value.get("textures").and_then(serde_json::Value::as_object) {
            for (key, texture) in entries {
                if let Some(texture) = texture
                    .as_str()
                    .or_else(|| texture.get("sprite").and_then(serde_json::Value::as_str))
                {
                    textures
                        .entry(key.clone())
                        .or_insert_with(|| texture.into());
                }
            }
        }
        current = value
            .get("parent")
            .and_then(serde_json::Value::as_str)
            .map(ResourceId::parse)
            .transpose()?;
    }
    for key in [
        "layer0", "all", "top", "up", "side", "front", "end", "particle",
    ] {
        let Some(mut texture) = textures.get(key).map(String::as_str) else {
            continue;
        };
        for _ in 0..8 {
            if let Some(reference) = texture.strip_prefix('#') {
                let Some(next) = textures.get(reference) else {
                    break;
                };
                texture = next;
            } else {
                return ResourceId::parse(texture).map(Some);
            }
        }
    }
    Ok(None)
}
