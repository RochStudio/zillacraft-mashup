//! Creative mode's inventory on the Minecraft map: the creative screen's catalog of every item
//! in vanilla's tabs and what its clicks do, `give`, and middle click picking the block in
//! sight onto the hotbar (`Minecraft.pickBlock`), all keeping clear of the hotbar's guns.

use std::collections::{HashMap, HashSet};

use frame::minecraft_ui::{CREATIVE_SEARCH, MC_HOTBAR};
use minecraft_terrain::pack::{PackStack, ResourceId};
use minecraftoss_core::registries::{DataPaths, Registries};
use minecraftoss_player::inventory::{Inventory, ItemStack};

use crate::minecraft_inventory::weapon_of;

/// The creative screen's listing tabs, as `frame::minecraft_ui::CREATIVE_TABS` orders them.
const BUILDING: usize = 0;
const COLORED: usize = 1;
const NATURAL: usize = 2;
const FUNCTIONAL: usize = 3;
const REDSTONE: usize = 4;
const TOOLS: usize = 5;
const COMBAT: usize = 6;
const FOOD: usize = 7;
const INGREDIENTS: usize = 8;
const SPAWN_EGGS: usize = 9;

/// Items no tab lists: air, and vanilla's operator utilities (invisible or inert here).
const HIDDEN: [&str; 14] = [
    "air",
    "barrier",
    "light",
    "structure_void",
    "structure_block",
    "jigsaw",
    "command_block",
    "chain_command_block",
    "repeating_command_block",
    "command_block_minecart",
    "debug_stick",
    "knowledge_book",
    "test_block",
    "test_instance_block",
];

/// Every item the creative screen lists, in vanilla's tabs (as near as the data pack's item
/// tags and the items' names tell), with what the search matches.
pub(crate) struct Catalog {
    tabs: Vec<Vec<String>>,
    /// Each item, and its name and id in lower case.
    all: Vec<(String, String)>,
}

impl Catalog {
    pub(crate) fn load(packs: &PackStack, registries: &Registries, language: &HashMap<String, String>) -> Self {
        let ids: Vec<String> = packs
            .list("minecraft", "items/")
            .unwrap_or_else(|_| Vec::new())
            .iter()
            .filter_map(|path| path.strip_prefix("assets/minecraft/items/")?.strip_suffix(".json"))
            .filter(|path| !HIDDEN.contains(path))
            .map(|path| format!("minecraft:{path}"))
            .collect();
        let tags = Tags::load();
        let mut tabs = vec![Vec::new(); CREATIVE_SEARCH];
        for id in &ids {
            tabs[tab_of(id, &tags, registries)].push(id.clone());
        }
        let all = ids
            .iter()
            .map(|id| (id.clone(), format!("{} {id}", minecraft_terrain::item_icons::item_name(language, id).to_lowercase())))
            .collect();
        Self { tabs, all }
    }

    /// The items on `tab`, or on the search tab those whose name or id has every word typed.
    pub(crate) fn items(&self, tab: usize, search: &str) -> Vec<String> {
        if tab == CREATIVE_SEARCH {
            let search = search.to_lowercase();
            let words: Vec<&str> = search.split_whitespace().collect();
            return self.all.iter().filter(|(_, text)| words.iter().all(|w| text.contains(w))).map(|(id, _)| id.clone()).collect();
        }
        self.tabs.get(tab).cloned().unwrap_or_else(Vec::new)
    }
}

/// The data pack's item tags, each resolved to its items.
struct Tags(HashMap<String, HashSet<String>>);

impl Tags {
    fn load() -> Self {
        let Some(root) = assets::minecraft_map::root() else {
            return Self(HashMap::new());
        };
        let dir = DataPaths::under(&root).datapack.join("data/minecraft/tags/item");
        let mut raw = HashMap::new();
        read_tags(&dir, "", &mut raw);
        let mut resolved = HashMap::new();
        for name in raw.keys() {
            resolve(name, &raw, &mut resolved, 0);
        }
        Self(resolved)
    }

    fn has(&self, tag: &str, id: &str) -> bool {
        self.0.get(tag).is_some_and(|items| items.contains(id))
    }
}

/// Reads every tag file under `dir`, named by its path (`swords`, `enchantable/bow`).
fn read_tags(dir: &std::path::Path, prefix: &str, out: &mut HashMap<String, Vec<String>>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(|s| format!("{prefix}{s}")) else { continue };
        if path.is_dir() {
            read_tags(&path, &format!("{name}/"), out);
        } else if let Some(json) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) {
            let values = json["values"].as_array().map_or_else(Vec::new, |values| {
                values.iter().filter_map(|v| v.as_str().or_else(|| v["id"].as_str()).map(str::to_owned)).collect()
            });
            out.insert(name, values);
        }
    }
}

fn resolve(name: &str, raw: &HashMap<String, Vec<String>>, done: &mut HashMap<String, HashSet<String>>, depth: u32) -> HashSet<String> {
    if let Some(items) = done.get(name) {
        return items.clone();
    }
    let mut items = HashSet::new();
    for value in raw.get(name).into_iter().flatten() {
        match value.strip_prefix("#minecraft:") {
            Some(tag) if depth < 16 => items.extend(resolve(tag, raw, done, depth + 1)),
            Some(_) => {}
            None => {
                items.insert(value.clone());
            }
        }
    }
    done.insert(name.to_owned(), items.clone());
    items
}

/// Which tab lists an item, by its tags and name, the more particular tabs first.
fn tab_of(id: &str, tags: &Tags, registries: &Registries) -> usize {
    let path = id.strip_prefix("minecraft:").unwrap_or(id);
    let tagged = |names: &[&str]| names.iter().any(|tag| tags.has(tag, id));
    let named = |names: &[&str]| names.contains(&path);
    // Wheat the item shares its name with the crop.
    let block = registries.blocks.block_by_name(id).is_some() && path != "wheat";
    if path.ends_with("_spawn_egg") {
        return SPAWN_EGGS;
    }
    if tagged(&["swords", "spears", "head_armor", "chest_armor", "leg_armor", "foot_armor", "arrows"])
        || tagged(&["enchantable/bow", "enchantable/crossbow", "enchantable/trident", "enchantable/mace"])
        || named(&["shield", "totem_of_undying", "wind_charge", "snowball", "egg", "blue_egg", "brown_egg", "firework_rocket", "end_crystal", "wolf_armor"])
        || path.ends_with("_horse_armor")
    {
        return COMBAT;
    }
    if tagged(&["pickaxes", "axes", "shovels", "hoes", "boats", "chest_boats", "bundles", "compasses", "harnesses", "enchantable/fishing"])
        || named(&[
            "shears", "flint_and_steel", "fire_charge", "brush", "spyglass", "clock", "lead", "name_tag", "saddle", "bucket", "elytra",
            "map", "writable_book", "goat_horn", "carrot_on_a_stick", "warped_fungus_on_a_stick", "minecart", "bone_meal",
            "ender_pearl", "ender_eye", "experience_bottle",
        ])
        || (path.ends_with("_bucket") && path != "milk_bucket")
        || path.ends_with("_minecart")
        || path.starts_with("music_disc_")
    {
        return TOOLS;
    }
    if minecraftoss_player::food::catalog().contains_key(id)
        || named(&["potion", "splash_potion", "lingering_potion", "milk_bucket", "ominous_bottle", "honey_bottle", "cake"])
    {
        return FOOD;
    }
    if tagged(&["buttons", "rails", "lightning_rods"])
        || path.contains("pressure_plate")
        || path.contains("copper_bulb")
        || named(&[
            "redstone", "redstone_torch", "redstone_block", "repeater", "comparator", "piston", "sticky_piston", "observer", "hopper",
            "dispenser", "dropper", "lectern", "lever", "target", "tripwire_hook", "daylight_detector", "sculk_sensor",
            "calibrated_sculk_sensor", "trapped_chest", "tnt", "redstone_lamp", "note_block", "slime_block", "honey_block", "crafter",
        ])
    {
        return REDSTONE;
    }
    if tagged(&["signs", "hanging_signs", "beds", "banners", "candles", "lanterns", "shulker_boxes", "skulls", "copper_chests", "copper_golem_statues", "wooden_shelves", "anvil"])
        || named(&[
            "crafting_table", "furnace", "blast_furnace", "smoker", "cartography_table", "fletching_table", "smithing_table",
            "stonecutter", "grindstone", "loom", "enchanting_table", "brewing_stand", "cauldron", "chest", "ender_chest", "barrel",
            "composter", "beehive", "bee_nest", "bell", "campfire", "soul_campfire", "torch", "soul_torch", "copper_torch", "conduit",
            "beacon", "respawn_anchor", "lodestone", "jukebox", "flower_pot", "decorated_pot", "painting", "item_frame",
            "glow_item_frame", "armor_stand", "scaffolding", "ladder", "chiseled_bookshelf", "bookshelf", "end_portal_frame",
            "dragon_egg", "spawner", "trial_spawner", "vault", "heavy_core", "end_rod", "creaking_heart", "dried_ghast",
        ])
    {
        return FUNCTIONAL;
    }
    if tagged(&["wool", "wool_carpets", "wool_slabs", "wool_stairs", "terracotta", "glazed_terracotta", "concrete", "concrete_powders", "concrete_slabs", "concrete_stairs", "dyes"])
        || path.ends_with("stained_glass")
        || path.ends_with("stained_glass_pane")
        || named(&["glass", "glass_pane", "tinted_glass"])
    {
        return COLORED;
    }
    let shaped = tagged(&["planks", "slabs", "stairs", "walls", "fences", "fence_gates", "doors", "trapdoors", "stone_bricks"])
        || ["brick", "polished", "smooth", "chiseled", "cut_", "tiles", "mosaic"].iter().any(|word| path.contains(word));
    let natural_tag = tagged(&["dirt", "sand", "logs", "leaves", "saplings", "flowers", "small_flowers", "mushrooms", "ores", "grass_blocks", "moss_blocks", "mud", "wart_blocks", "crimson_stems", "warped_stems"]);
    let natural_word = [
        "ore", "coral", "ice", "snow", "kelp", "seagrass", "vine", "fern", "grass", "dripleaf", "azalea", "fungus", "roots", "sprouts",
        "cactus", "sugar_cane", "bamboo", "pumpkin", "melon", "hay_block", "nylium", "netherrack", "obsidian", "gravel", "clay", "sculk",
        "amethyst", "dripstone", "lily_pad", "spore_blossom", "bone_block", "magma_block", "glowstone", "shroomlight", "frogspawn",
        "sniffer_egg", "turtle_egg", "mycelium", "podzol", "calcite", "soul_sand", "soul_soil", "bush", "lichen", "petals", "cobweb",
        "bedrock", "end_stone", "chorus", "eyeblossom", "moss", "leaf_litter", "wildflowers",
    ]
    .iter()
    .any(|word| path.contains(word));
    let natural_stone = named(&["stone", "deepslate", "granite", "diorite", "andesite", "tuff", "basalt", "blackstone", "sandstone", "red_sandstone", "prismarine"]);
    if (block && !shaped && (natural_tag || natural_word || natural_stone)) || path.ends_with("_seeds") || tagged(&["villager_plantable_seeds"]) {
        return NATURAL;
    }
    if block { BUILDING } else { INGREDIENTS }
}

/// A click on the creative screen's list, by vanilla's rules (`CreativeModeInventoryScreen`):
/// with nothing carried it picks one up (a stack with shift); carrying the same item a left
/// click adds one (fills the stack with shift) and a right takes one; carrying another, a
/// left click throws it away and a right takes one off it. A carried gun goes back into the
/// inventory first, never away.
pub(crate) fn take(inventory: &mut Inventory, registries: &Registries, id: &str, right: bool, shift: bool, selected: usize) {
    if !stow_carried_gun(inventory, selected) {
        return;
    }
    match inventory.cursor.as_mut() {
        None => inventory.cursor = Some(stack(registries, id, if shift { stack(registries, id, 1).max } else { 1 })),
        Some(carried) if carried.id == id && !right => carried.count = if shift { carried.max } else { (carried.count + 1).min(carried.max) },
        Some(_) if !right => inventory.cursor = None,
        Some(carried) => {
            carried.count -= 1;
            if carried.count == 0 {
                inventory.cursor = None;
            }
        }
    }
}

/// A number key over a listed item: a stack of it in that hotbar slot. A gun there moves to
/// the backpack first, or the key does nothing.
pub(crate) fn to_hotbar(inventory: &mut Inventory, registries: &Registries, id: &str, hotbar: usize) {
    let hotbar = hotbar.min(MC_HOTBAR - 1);
    if inventory.slots[hotbar].as_ref().and_then(weapon_of).is_some() {
        let Some(free) = (MC_HOTBAR..36).find(|&i| inventory.slots[i].is_none()) else { return };
        inventory.slots[free] = inventory.slots[hotbar].take();
    }
    let max = stack(registries, id, 1).max;
    inventory.slots[hotbar] = Some(stack(registries, id, max));
}

/// The carried stack put down on the list: gone, unless it is a gun.
pub(crate) fn destroy_carried(inventory: &mut Inventory, selected: usize) {
    if stow_carried_gun(inventory, selected) {
        inventory.cursor = None;
    }
}

/// A carried gun back into the inventory. False if it has nowhere to go.
fn stow_carried_gun(inventory: &mut Inventory, selected: usize) -> bool {
    match inventory.cursor.take() {
        Some(gun) if weapon_of(&gun).is_some() => match inventory.add_item(gun, selected) {
            None => true,
            Some(back) => {
                inventory.cursor = Some(back);
                false
            }
        },
        other => {
            inventory.cursor = other;
            true
        }
    }
}

/// The item for what was typed: `Stone` and `minecraft:stone` alike.
fn item_id(typed: &str) -> String {
    let typed = typed.trim().to_ascii_lowercase();
    if typed.contains(':') { typed } else { format!("minecraft:{typed}") }
}

/// Whether the resource packs know the item.
fn known(packs: &PackStack, id: &str) -> bool {
    ResourceId::parse(id).is_ok_and(|id| packs.item_definition(&id).is_ok_and(|d| d.is_some()))
}

fn stack(registries: &Registries, id: &str, count: u8) -> ItemStack {
    let mut stack = ItemStack::new(id.to_owned(), count);
    stack.max = registries.items.max_stack(id).clamp(1, 99) as u8;
    stack
}

/// `give <item> [count]`: that many (a stack unless counted) into the inventory, as pickups
/// go in. Returns the console's answer.
pub(crate) fn give(
    inventory: &mut Inventory,
    selected: usize,
    packs: &PackStack,
    registries: &Registries,
    typed: &str,
    count: Option<u32>,
) -> String {
    let id = item_id(typed);
    if !known(packs, &id) {
        return format!("give: there is no item called {typed}");
    }
    let name = id.strip_prefix("minecraft:").unwrap_or(&id);
    let max = stack(registries, &id, 1).max;
    let wanted = count.unwrap_or(u32::from(max));
    let mut given = 0;
    while given < wanted {
        let n = (wanted - given).min(u32::from(max)) as u8;
        if let Some(rest) = inventory.add_item(stack(registries, &id, n), selected) {
            given += u32::from(n - rest.count);
            return format!("give: inventory full - {given} {name}");
        }
        given += u32::from(n);
    }
    format!("give: {given} {name}")
}

/// The item a block is picked as: itself, or what places it (`wall_torch` is a torch,
/// `redstone_wire` redstone, `carrots` a carrot).
fn block_item(packs: &PackStack, block: &str) -> Option<String> {
    let (namespace, path) = block.split_once(':').unwrap_or(("minecraft", block));
    let named = match path {
        "redstone_wire" => "redstone".to_owned(),
        "tripwire" => "string".to_owned(),
        "carrots" => "carrot".to_owned(),
        "potatoes" => "potato".to_owned(),
        "beetroots" => "beetroot_seeds".to_owned(),
        "wheat" => "wheat_seeds".to_owned(),
        "cocoa" => "cocoa_beans".to_owned(),
        "sweet_berry_bush" => "sweet_berries".to_owned(),
        "cave_vines" | "cave_vines_plant" => "glow_berries".to_owned(),
        "melon_stem" | "attached_melon_stem" => "melon_seeds".to_owned(),
        "pumpkin_stem" | "attached_pumpkin_stem" => "pumpkin_seeds".to_owned(),
        "bamboo_sapling" => "bamboo".to_owned(),
        "tall_seagrass" => "seagrass".to_owned(),
        "piston_head" => "piston".to_owned(),
        _ => path
            .strip_prefix("potted_")
            .map(str::to_owned)
            .or_else(|| path.strip_suffix("_plant").map(str::to_owned))
            .unwrap_or_else(|| path.replace("wall_", "")),
    };
    [format!("{namespace}:{path}"), format!("{namespace}:{named}")]
        .into_iter()
        .find(|id| known(packs, id))
}

/// Middle click in creative: the block in sight onto the hotbar, selected. The slot already
/// holding it, else the selected one if empty, else an empty one, else the selected one unless
/// it holds a gun. Returns the slot to select.
pub(crate) fn pick_block(
    inventory: &mut Inventory,
    selected: usize,
    packs: &PackStack,
    registries: &Registries,
    block: &str,
) -> Option<usize> {
    let id = block_item(packs, block)?;
    let slots = &inventory.slots;
    if let Some(slot) = (0..MC_HOTBAR).find(|&i| slots[i].as_ref().is_some_and(|s| s.id == id)) {
        return Some(slot);
    }
    let gun = |i: usize| slots[i].as_ref().and_then(weapon_of).is_some();
    let slot = if slots[selected].is_none() {
        selected
    } else if let Some(empty) = (0..MC_HOTBAR).find(|&i| slots[i].is_none()) {
        empty
    } else if !gun(selected) {
        selected
    } else {
        (0..MC_HOTBAR).find(|&i| !gun(i))?
    };
    inventory.slots[slot] = Some(stack(registries, &id, 1));
    Some(slot)
}
