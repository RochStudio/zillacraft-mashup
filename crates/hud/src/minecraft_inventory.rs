//! The Minecraft map's hotbar and inventory, drawn as MW2 HUD chrome: MW2's
//! fonts, materials and colours over vanilla's slots. The inventory itself
//! (vanilla's container rules) lives with the world; this draws what it
//! publishes in `frame::MinecraftUi` and hands back the player's clicks.
use std::collections::HashMap;

use asset_core::{AssetNamespace, AssetRef};
use asset_game::{FontDef, MenuCatalog};
use assets::{PreparedLocalizedStrings, PreparedWeapons};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use frame::minecraft_ui::{CREATIVE_COLUMNS, CREATIVE_INVENTORY, CREATIVE_ROWS, CREATIVE_SEARCH, CREATIVE_TABS};
use frame::{McClick, McSlot, McStack, MinecraftUi};
use hud_iw4::{normalized_text_scale as r_normalized_text_scale, ui_text_height};

use crate::chrome::ui_text_width;
use crate::draw2d::{Draw2dCmd, Draw2dList, Draw2dOp, Draw2dProvenance, tessellate_fonts};
use crate::gaps::HudPresentationGaps;
use crate::gpu_list::{HudTessPass, TessJob};
use crate::images::{HUD_CHROME_NAMESPACE, HudImages};

/// The item icon atlas, drawn as a HUD material.
const ICONS: &str = "mc_item_icons";

const CENTER: i32 = 2;
const MAX: i32 = 3;

// MW2's theme: the rust orange of its menus and smoke, the pale green-white
// of its logo, on warm dark glass.
const PANEL: [f32; 4] = [0.046, 0.040, 0.034, 0.90];
const HEADER: [f32; 4] = [0.085, 0.068, 0.050, 0.97];
const EDGE: [f32; 4] = [1.0, 0.92, 0.82, 0.14];
const ACCENT: [f32; 4] = [0.91, 0.53, 0.18, 1.0];
const HIGHLIGHT: [f32; 4] = [0.86, 0.96, 0.82, 1.0];
const TEXT: [f32; 4] = [0.93, 0.93, 0.90, 1.0];
const TEXT_DIM: [f32; 4] = [0.66, 0.62, 0.56, 1.0];
const SLOT: [f32; 4] = [0.0, 0.0, 0.0, 0.48];
const SLOT_EDGE: [f32; 4] = [1.0, 0.90, 0.78, 0.10];
const SLOT_HOVER: [f32; 4] = [0.91, 0.53, 0.18, 0.20];
/// The inventory's size over its virtual layout.
const K: f32 = 1.3;

/// Slot size and pitch in virtual pixels.
const S: f32 = 20.0;
const P: f32 = 22.0;
/// The panel, about the screen's centre.
const PANEL_X: f32 = -118.0;
const PANEL_Y: f32 = -128.0;
const PANEL_W: f32 = 236.0;
const PANEL_H: f32 = 238.0;
const HEADER_H: f32 = 20.0;
const GRID_X: f32 = -98.0;
/// The character window.
const BOX: [f32; 4] = [-73.0, -100.0, 62.0, 86.0];

/// The creative screen: its tab strip over the panel (both screens' in creative), then the
/// search field or the tab's name, the list in a scrolling grid, and the hotbar.
const TAB_S: f32 = 18.0;
const TAB_P: f32 = 19.5;
const TAB_X: f32 = PANEL_X + (PANEL_W - (TAB_P * (CREATIVE_TABS.len() - 1) as f32 + TAB_S)) * 0.5;
const TAB_Y: f32 = PANEL_Y - TAB_S - 6.0;
const FIELD_Y: f32 = PANEL_Y + HEADER_H + 6.0;
const FIELD_H: f32 = 12.0;
const LIST_Y: f32 = FIELD_Y + FIELD_H + 6.0;
const HOTBAR_Y: f32 = LIST_Y + CREATIVE_ROWS as f32 * P + 12.0;
const CREATIVE_H: f32 = HOTBAR_Y + S + 8.0 - PANEL_Y;
const SCROLL_X: f32 = GRID_X + CREATIVE_COLUMNS as f32 * P + 2.0;
const SCROLL_W: f32 = 6.0;
const SCROLL_H: f32 = CREATIVE_ROWS as f32 * P - 2.0;
/// The longest search.
const SEARCH_MAX: usize = 40;

/// What the mouse is over on the creative screen.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CreativeHit {
    Tab(usize),
    /// A cell of the list grid (not the item: the list scrolls under it).
    Cell(usize),
    Hotbar(usize),
    /// The scroll bar.
    Scroll,
}

/// Where each slot of the screen sits (x, y, size).
fn layout() -> Vec<(McSlot, f32, f32, f32)> {
    let mut out = Vec::new();
    // Armor, head to feet down the left (slots 39..36).
    for (row, slot) in [39usize, 38, 37, 36].into_iter().enumerate() {
        out.push((McSlot::Inventory(slot), GRID_X, -100.0 + row as f32 * P, S));
    }
    // The offhand beside the character window.
    out.push((McSlot::Inventory(40), BOX[0] + BOX[2] + 3.0, -34.0, S));
    // The 2x2 crafting grid and its result.
    for i in 0..4 {
        out.push((McSlot::Crafting(i), 22.0 + (i % 2) as f32 * P, -86.0 + (i / 2) as f32 * P, S));
    }
    out.push((McSlot::Result, 88.0, -77.0, 24.0));
    // The backpack, three rows of nine, then the hotbar.
    for row in 0..3 {
        for col in 0..9 {
            out.push((McSlot::Inventory(9 + row * 9 + col), GRID_X + col as f32 * P, 6.0 + row as f32 * P, S));
        }
    }
    for col in 0..9 {
        out.push((McSlot::Inventory(col), GRID_X + col as f32 * P, 80.0, S));
    }
    out
}

#[derive(Component)]
pub(crate) struct MinecraftRaster;

/// The input state the screen keeps between frames.
#[derive(Default)]
pub(crate) struct ScreenInput {
    last_click: Option<(McSlot, f64)>,
    /// A drag of the carried stack: the button and the slots crossed.
    drag: Option<(bool, Vec<usize>)>,
    /// The creative list's scroll bar is held.
    scrolling: bool,
}

struct Canvas<'a> {
    surface: &'a crate::surface::Hud2dSurface,
    cmds: Vec<Draw2dCmd>,
    fonts: HashMap<String, &'a FontDef>,
    /// The scale virtual coordinates are drawn at.
    k: f32,
}

impl Canvas<'_> {
    fn quad(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], material: &str, st: [f32; 4], align: (i32, i32)) {
        self.quad_ns(x, y, w, h, color, material, HUD_CHROME_NAMESPACE, st, align);
    }

    #[allow(clippy::too_many_arguments)]
    fn quad_ns(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
        material: &str,
        namespace: AssetNamespace,
        st: [f32; 4],
        align: (i32, i32),
    ) {
        let k = self.k;
        let r = self.surface.apply_rect(x * k, y * k, w * k, h * k, align.0, align.1);
        self.cmds.push(Draw2dCmd {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            s0: st[0],
            t0: st[1],
            s1: st[2],
            t1: st[3],
            color,
            material: material.to_owned(),
            material_namespace: namespace,
            op: Draw2dOp::StretchPic,
            provenance: Draw2dProvenance::CgDraw { site: "minecraft_inventory" },
            layer: 1,
        });
    }

    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], align: (i32, i32)) {
        self.quad(x, y, w, h, color, "white", [0.0, 0.0, 1.0, 1.0], align);
    }

    /// A hairline frame.
    fn frame(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], align: (i32, i32)) {
        let t = 0.5;
        self.fill(x, y, w, t, color, align);
        self.fill(x, y + h - t, w, t, color, align);
        self.fill(x, y, t, h, color, align);
        self.fill(x + w - t, y, t, h, color, align);
    }

    /// MW2's corner brackets.
    fn brackets(&mut self, x: f32, y: f32, w: f32, h: f32, len: f32, color: [f32; 4], align: (i32, i32)) {
        let t = 1.0;
        for (cx, cy, dx, dy) in [(x, y, 1.0, 1.0), (x + w, y, -1.0, 1.0), (x, y + h, 1.0, -1.0), (x + w, y + h, -1.0, -1.0)] {
            let hx = if dx > 0.0 { cx } else { cx - len };
            let vy = if dy > 0.0 { cy } else { cy - len };
            self.fill(hx, if dy > 0.0 { cy } else { cy - t }, len, t, color, align);
            self.fill(if dx > 0.0 { cx } else { cx - t }, vy, t, len, color, align);
        }
    }

    /// Text in a font `px` virtual pixels tall, its top-left at (x, y);
    /// `right` aligns its right edge to x instead. Returns its width.
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, font: &str, x: f32, y: f32, px: f32, color: [f32; 4], text: &str, right: bool, align: (i32, i32)) -> f32 {
        let Some(def) = self.fonts.get(font).copied() else {
            return 0.0;
        };
        let text_scale = px / ui_text_height(1.0);
        let width = ui_text_width(def, text, text_scale);
        let x = if right { x - width } else { x };
        let k = self.k;
        let nscale = r_normalized_text_scale(def.pixel_height, text_scale * k);
        let r = self.surface.apply_rect(x * k, (y + px) * k, nscale, nscale, align.0, align.1);
        self.cmds.push(Draw2dCmd {
            x: (r.x + 0.5).floor(),
            y: (r.y + 0.5).floor(),
            w: r.w,
            h: r.h,
            s0: 0.0,
            t0: 0.0,
            s1: 1.0,
            t1: 1.0,
            color,
            material: AssetRef::bare_name(&def.material).to_owned(),
            material_namespace: HUD_CHROME_NAMESPACE,
            op: Draw2dOp::TextRun {
                font: font.to_owned(),
                scale: nscale,
                text: text.to_owned(),
                loc_key: String::new(),
                style: crate::draw2d::TEXT_STYLE_HUDELEM,
                fx: None,
                glow: None,
            },
            provenance: Draw2dProvenance::CgDraw { site: "minecraft_inventory" },
            layer: 1,
        });
        width
    }
}

const FONT_TITLE: &str = "fonts/objectivefont";
const FONT_SMALL: &str = "fonts/smallfont";
const FONT_COUNT: &str = "fonts/hudsmallfont";

/// A slot's item: its icon, a gun's MW2 kill icon, and its count.
fn draw_stack(
    canvas: &mut Canvas<'_>,
    ui: &MinecraftUi,
    weapons: Option<&PreparedWeapons>,
    stack: &McStack,
    x: f32,
    y: f32,
    size: f32,
    align: (i32, i32),
) {
    let inset = size * 0.1;
    if let Some(weapon) = stack.weapon {
        let Some(reg) = weapons else { return };
        let namespace = reg.0.namespace_of(weapon).unwrap_or(HUD_CHROME_NAMESPACE);
        let Some(icon) = reg.0.kill_icon_image_of(weapon).or_else(|| reg.0.kill_icon_of(weapon)) else {
            return;
        };
        let facts = reg.0.facts_of(weapon);
        let ratio = match facts.map(|f| f.kill_icon_ratio) {
            Some(1) => 2.0,
            Some(2) => 4.0,
            _ => 1.0,
        };
        let flip = facts.is_some_and(|f| f.flip_kill_icon);
        // Guns sit across the slot, as wide as it allows.
        let w = size * 1.18;
        let h = (w / ratio).min(size - inset);
        let st = if flip { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 1.0] };
        canvas.quad_ns(x + (size - w) * 0.5, y + (size - h) * 0.5, w, h, TEXT, icon, namespace, st, align);
        return;
    }
    if let Some(rect) = ui.icon_rects.get(&stack.id) {
        canvas.quad(x + inset, y + inset, size - inset * 2.0, size - inset * 2.0, [1.0; 4], ICONS, *rect, align);
    } else {
        // No icon (yet, or a model only a special renderer draws): its name's first and last
        // words, short.
        let name = ui.names.get(&stack.id).map_or(stack.id.trim_start_matches("minecraft:"), String::as_str).to_uppercase();
        let words: Vec<&str> = name.split([' ', '_']).filter(|w| !w.is_empty()).collect();
        let short = |word: &str| word.chars().take(6).collect::<String>();
        let lines = match words.as_slice() {
            [] => Vec::new(),
            [one] => vec![short(one)],
            [first, .., last] => vec![short(first), short(last)],
        };
        let px = 4.0;
        let top = y + (size - lines.len() as f32 * (px + 1.0)) * 0.5;
        for (i, line) in lines.iter().enumerate() {
            let w = canvas.fonts.get(FONT_SMALL).map_or(0.0, |def| ui_text_width(def, line, px / ui_text_height(1.0)));
            canvas.text(FONT_SMALL, x + (size - w) * 0.5, top + i as f32 * (px + 1.0), px, [TEXT[0], TEXT[1], TEXT[2], 0.8], line, false, align);
        }
    }
    if stack.count > 1 {
        let label = stack.count.to_string();
        canvas.text(FONT_COUNT, x + size - 1.0, y + size - 8.5, 7.5, TEXT, &label, true, align);
    }
}

fn stack_at<'a>(ui: &'a MinecraftUi, slot: McSlot) -> Option<&'a McStack> {
    match slot {
        McSlot::Inventory(i) => ui.slots.get(i)?.as_ref(),
        McSlot::Crafting(i) => ui.crafting.get(i)?.as_ref(),
        McSlot::Result => ui.result.as_ref(),
    }
}

fn stack_name(
    stack: &McStack,
    ui: &MinecraftUi,
    weapons: Option<&PreparedWeapons>,
    strings: Option<&PreparedLocalizedStrings>,
    gaps: &mut HudPresentationGaps,
) -> String {
    match stack.weapon {
        Some(weapon) => crate::weapon_name::localized_weapon_name(weapon, weapons, strings, gaps)
            .unwrap_or_else(|| "Weapon".to_owned()),
        None => ui.names.get(&stack.id).cloned().unwrap_or_else(|| stack.id.clone()),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update_minecraft_hud(
    surface: Res<crate::surface::Hud2dSurface>,
    catalog: Option<Res<MenuCatalog>>,
    ui: Option<ResMut<MinecraftUi>>,
    (weapons, strings): (Option<Res<PreparedWeapons>>, Option<Res<PreparedLocalizedStrings>>),
    (keys, buttons): (Res<ButtonInput<KeyCode>>, Res<ButtonInput<MouseButton>>),
    (mut wheel, mut typed): (MessageReader<MouseWheel>, MessageReader<KeyboardInput>),
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    time: Res<Time>,
    mut input: Local<ScreenInput>,
    (mut hud_images, mut images): (ResMut<HudImages>, ResMut<Assets<Image>>),
    mut gaps: ResMut<HudPresentationGaps>,
    mut pass: ResMut<HudTessPass>,
) {
    let Some(mut ui) = ui else {
        pass.minecraft = TessJob::Hide;
        return;
    };
    if !ui.active || !surface.is_ready() {
        pass.minecraft = TessJob::Hide;
        ui.character_box = None;
        for _ in wheel.read() {}
        for _ in typed.read() {}
        return;
    }
    if let Some(icons) = ui.icons.clone() {
        hud_images.insert_runtime(ICONS, icons);
    }
    let now = time.elapsed_secs_f64();
    let window = windows.single().ok();
    let mouse = window.and_then(Window::cursor_position);

    // Opening and closing, and the hotbar's keys; on the creative screen's search tab the
    // keys type instead (Escape still closes).
    let typing = ui.inventory_open && ui.creative && ui.creative_tab == CREATIVE_SEARCH;
    let open_before = ui.inventory_open;
    if (keys.just_pressed(KeyCode::KeyE) && !typing) || (ui.inventory_open && keys.just_pressed(KeyCode::Escape)) {
        ui.inventory_open = !ui.inventory_open && !keys.just_pressed(KeyCode::Escape);
    }
    if open_before && !ui.inventory_open {
        ui.clicks.push(McClick::Close);
        input.drag = None;
        input.scrolling = false;
    }
    if typing && ui.inventory_open {
        let before = ui.creative_search.clone();
        for event in typed.read().filter(|e| e.state == ButtonState::Pressed) {
            match &event.logical_key {
                Key::Backspace => {
                    ui.creative_search.pop();
                }
                Key::Space => ui.creative_search.push(' '),
                Key::Character(text) => ui.creative_search.extend(text.chars().filter(|c| !c.is_control())),
                _ => {}
            }
        }
        ui.creative_search = ui.creative_search.chars().take(SEARCH_MAX).collect();
        if ui.creative_search != before {
            ui.creative_row = 0;
        }
    } else {
        for _ in typed.read() {}
    }
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let digit = digits.iter().position(|k| keys.just_pressed(*k)).filter(|_| !typing);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);

    let fonts: HashMap<String, &FontDef> = catalog
        .as_deref()
        .map(|catalog| {
            [FONT_TITLE, FONT_SMALL, FONT_COUNT]
                .into_iter()
                .filter_map(|name| catalog.font(name).map(|font| (name.to_owned(), font)))
                .collect()
        })
        .unwrap_or_default();
    let mut canvas = Canvas { surface: &surface, cmds: Vec::new(), fonts, k: 1.0 };
    let weapons = weapons.as_deref();
    let strings = strings.as_deref();

    if !ui.inventory_open {
        // The hotbar: scroll and number keys pick, Q throws.
        let mut scroll = 0i32;
        for event in wheel.read() {
            scroll += if event.y < 0.0 { 1 } else if event.y > 0.0 { -1 } else { 0 };
        }
        if scroll != 0 {
            ui.select = Some((ui.selected as i32 + scroll).rem_euclid(9) as usize);
        }
        if let Some(digit) = digit {
            ui.select = Some(digit);
        }
        if keys.just_pressed(KeyCode::KeyQ) {
            ui.drop_selected = Some(ctrl);
        }
        canvas.k = 1.15;
        draw_hotbar(&mut canvas, &mut ui, weapons, strings, &mut gaps, time.delta_secs());
        ui.character_box = None;
    } else {
        let scroll: i32 = wheel.read().map(|e| if e.y < 0.0 { 1 } else if e.y > 0.0 { -1 } else { 0 }).sum();
        canvas.k = K;
        // In creative the tab strip over the panel switches between the lists, the search and
        // the player's own inventory.
        let tab = mouse.filter(|_| ui.creative).and_then(|m| tab_at(&surface, m));
        if let Some(tab) = tab
            && tab != ui.creative_tab
            && (buttons.just_pressed(MouseButton::Left) || buttons.just_pressed(MouseButton::Right))
        {
            ui.creative_tab = tab;
            ui.creative_row = 0;
            input.drag = None;
        }
        if ui.creative && ui.creative_tab != CREATIVE_INVENTORY {
            let hit = mouse.and_then(|m| creative_hit(&surface, m));
            let inside = tab.is_some() || mouse.is_some_and(|m| in_rect(&surface, m, PANEL_X, PANEL_Y, PANEL_W, CREATIVE_H));
            let fraction = mouse.map(|m| scroll_fraction(&surface, m));
            handle_creative(&mut ui, &mut input, &buttons, hit, inside, shift, digit, scroll, fraction);
            draw_creative(&mut canvas, &mut ui, weapons, strings, &mut gaps, hit, mouse, typing, now);
            ui.character_box = None;
        } else {
            let hovered = mouse.and_then(|m| hovered_slot(&surface, m));
            let inside = tab.is_some() || mouse.is_some_and(|m| over_panel(&surface, m));
            handle_clicks(&mut ui, &mut input, &buttons, hovered, inside, shift, digit, now);
            draw_inventory(&mut canvas, &mut ui, weapons, strings, &mut gaps, hovered, mouse);
            if ui.creative {
                draw_tabs(&mut canvas, &ui, tab);
            }
            // Where the character stands, and the mouse it follows.
            let b = surface.apply_rect(BOX[0] * K, BOX[1] * K, BOX[2] * K, BOX[3] * K, CENTER, CENTER);
            let centre = [b.x + b.w * 0.5, b.y + b.h * 0.36];
            ui.character_box = Some([b.x + b.w * 0.5, b.y + b.h * 0.5, b.w, b.h]);
            if let Some(m) = mouse {
                ui.gaze = [(m.x - centre[0]) / (b.h * 0.5), (m.y - centre[1]) / (b.h * 0.5)];
            }
        }
    }

    let list = Draw2dList { cmds: canvas.cmds };
    let fonts = canvas.fonts;
    let (quads, _) = tessellate_fonts(&list, &fonts);
    for font in fonts.values() {
        let _ = hud_images.get(HUD_CHROME_NAMESPACE, AssetRef::bare_name(&font.material), &mut images);
    }
    pass.minecraft = if quads.is_empty() { TessJob::Hide } else { TessJob::Quads(quads) };
}

fn hovered_slot(surface: &crate::surface::Hud2dSurface, mouse: Vec2) -> Option<McSlot> {
    layout().into_iter().find_map(|(slot, x, y, size)| {
        let r = surface.apply_rect(x * K, y * K, size * K, size * K, CENTER, CENTER);
        (mouse.x >= r.x && mouse.x < r.x + r.w && mouse.y >= r.y && mouse.y < r.y + r.h).then_some(slot)
    })
}

fn over_panel(surface: &crate::surface::Hud2dSurface, mouse: Vec2) -> bool {
    let r = surface.apply_rect(PANEL_X * K, PANEL_Y * K, PANEL_W * K, PANEL_H * K, CENTER, CENTER);
    mouse.x >= r.x && mouse.x < r.x + r.w && mouse.y >= r.y && mouse.y < r.y + r.h
}

/// Vanilla's container input: a click picks up, puts down or swaps; with
/// shift it moves the stack across; a double click gathers; dragging a
/// carried stack over slots shares it out; a number key swaps with the
/// hotbar; a click outside throws.
#[allow(clippy::too_many_arguments)]
fn handle_clicks(
    ui: &mut MinecraftUi,
    input: &mut ScreenInput,
    buttons: &ButtonInput<MouseButton>,
    hovered: Option<McSlot>,
    inside: bool,
    shift: bool,
    digit: Option<usize>,
    now: f64,
) {
    if let (Some(digit), Some(slot)) = (digit, hovered) {
        ui.clicks.push(McClick::Swap { slot, hotbar: digit });
    }
    // A drag keeps collecting the slots it crosses.
    if let Some((_, slots)) = input.drag.as_mut()
        && let Some(McSlot::Inventory(index)) = hovered
        && !slots.contains(&index)
    {
        slots.push(index);
    }
    for (button, right) in [(MouseButton::Left, false), (MouseButton::Right, true)] {
        if buttons.just_pressed(button) {
            let Some(slot) = hovered else {
                if !inside {
                    ui.clicks.push(McClick::Outside { right });
                }
                continue;
            };
            if !right
                && !shift
                && input.last_click.is_some_and(|(last, at)| last == slot && now - at < 0.25)
                && ui.cursor.is_some()
            {
                ui.clicks.push(McClick::Gather { right: false });
                input.last_click = None;
                continue;
            }
            input.last_click = Some((slot, now));
            match slot {
                McSlot::Inventory(index) if ui.cursor.is_some() && !shift => {
                    input.drag = Some((right, vec![index]));
                }
                _ => ui.clicks.push(McClick::Slot { slot, right, shift }),
            }
        }
        if buttons.just_released(button)
            && let Some((drag_right, slots)) = input.drag.take()
        {
            if drag_right != right {
                input.drag = Some((drag_right, slots));
                continue;
            }
            if slots.len() > 1 {
                ui.clicks.push(McClick::Spread { slots, right });
            } else if let Some(&index) = slots.first() {
                ui.clicks.push(McClick::Slot { slot: McSlot::Inventory(index), right, shift: false });
            }
        }
    }
}

/// The hotbar along the bottom of the screen, between MW2's score and ammo.
fn draw_hotbar(
    canvas: &mut Canvas<'_>,
    ui: &mut MinecraftUi,
    weapons: Option<&PreparedWeapons>,
    strings: Option<&PreparedLocalizedStrings>,
    gaps: &mut HudPresentationGaps,
    dt: f32,
) {
    let align = (CENTER, MAX);
    let size = 22.0;
    let pitch = 25.0;
    let width = pitch * 9.0 - (pitch - size);
    let x0 = -width * 0.5;
    let y = -size - 3.0;
    // The strip behind it.
    canvas.fill(x0 - 3.0, y - 3.0, width + 6.0, size + 6.0, [0.04, 0.03, 0.02, 0.50], align);
    canvas.quad(x0 - 3.0, y - 3.0, width + 6.0, size + 6.0, [ACCENT[0], ACCENT[1], ACCENT[2], 0.10], "gradient_fadein_fadebottom", [0.0, 0.0, 1.0, 1.0], align);
    canvas.frame(x0 - 3.0, y - 3.0, width + 6.0, size + 6.0, EDGE, align);
    for i in 0..9 {
        let selected = i == ui.selected;
        let x = x0 + i as f32 * pitch;
        let lift = if selected { -2.0 } else { 0.0 };
        if selected {
            canvas.quad(x - 7.0, y - 9.0 + lift, size + 14.0, size + 16.0, [ACCENT[0], ACCENT[1], ACCENT[2], 0.55], "mockup_bg_glow", [0.0, 0.0, 1.0, 1.0], align);
        }
        canvas.fill(x, y + lift, size, size, if selected { [0.16, 0.09, 0.03, 0.80] } else { SLOT }, align);
        canvas.frame(x, y + lift, size, size, if selected { HIGHLIGHT } else { SLOT_EDGE }, align);
        if selected {
            canvas.fill(x, y + lift + size - 1.5, size, 1.5, ACCENT, align);
        }
        canvas.text(FONT_SMALL, x + 1.5, y + lift + 0.5, 5.5, if selected { HIGHLIGHT } else { TEXT_DIM }, &(i + 1).to_string(), false, align);
        if let Some(stack) = ui.slots.get(i).and_then(Option::as_ref).cloned() {
            draw_stack(canvas, ui, weapons, &stack, x, y + lift, size, align);
        }
    }
    // The selected item's name, as vanilla shows it over the hotbar.
    let name = ui.slots.get(ui.selected).and_then(Option::as_ref).map(|stack| {
        stack_name(stack, ui, weapons, strings, gaps)
    });
    if let Some((shown, age)) = ui.selected_name.as_mut() {
        *age += dt;
        let fade = (1.0 - ((*age - 1.6) / 0.5).clamp(0.0, 1.0)).clamp(0.0, 1.0);
        if fade > 0.0 {
            let text = name.as_deref().unwrap_or(shown).to_uppercase();
            let px = 9.0;
            let w = canvas.fonts.get(FONT_TITLE).map_or(0.0, |def| {
                ui_text_width(def, &text, px / ui_text_height(1.0))
            });
            canvas.text(FONT_TITLE, -w * 0.5, y - 17.0, px, [HIGHLIGHT[0], HIGHLIGHT[1], HIGHLIGHT[2], fade], &text, false, align);
        }
    }
}

/// The inventory screen.
#[allow(clippy::too_many_arguments)]
fn draw_inventory(
    canvas: &mut Canvas<'_>,
    ui: &mut MinecraftUi,
    weapons: Option<&PreparedWeapons>,
    strings: Option<&PreparedLocalizedStrings>,
    gaps: &mut HudPresentationGaps,
    hovered: Option<McSlot>,
    mouse: Option<Vec2>,
) {
    let c = (CENTER, CENTER);
    // Dim the world around the panel; the character window stays clear.
    let dim = [0.0, 0.0, 0.0, 0.55];
    let [bx, by, bw, bh] = BOX;
    let around = [
        (-2000.0, -2000.0, 4000.0, 2000.0 + by),
        (-2000.0, by + bh, 4000.0, 2000.0),
        (-2000.0, by, 2000.0 + bx, bh),
        (bx + bw, by, 2000.0, bh),
    ];
    for (x, y, w, h) in around {
        canvas.fill(x, y, w, h, dim, c);
        canvas.quad(x, y, w, h, [ACCENT[0], ACCENT[1], ACCENT[2], 0.07], "gradient_fadein_fadebottom", [0.0, 0.0, 1.0, 1.0], c);
    }

    // The panel: glass, header, edges and brackets.
    for (x, y, w, h) in [
        (PANEL_X, PANEL_Y + HEADER_H, PANEL_W, by - PANEL_Y - HEADER_H),
        (PANEL_X, by + bh, PANEL_W, PANEL_Y + PANEL_H - by - bh),
        (PANEL_X, by, bx - PANEL_X, bh),
        (bx + bw, by, PANEL_X + PANEL_W - bx - bw, bh),
    ] {
        canvas.fill(x, y, w, h, PANEL, c);
    }
    canvas.fill(PANEL_X, PANEL_Y, PANEL_W, HEADER_H, HEADER, c);
    canvas.quad(PANEL_X, PANEL_Y, PANEL_W, HEADER_H, [ACCENT[0], ACCENT[1], ACCENT[2], 0.22], "gradient_fadein_fadebottom", [0.0, 0.0, 1.0, 1.0], c);
    canvas.fill(PANEL_X, PANEL_Y + HEADER_H - 1.0, PANEL_W, 1.0, [ACCENT[0], ACCENT[1], ACCENT[2], 0.85], c);
    canvas.fill(PANEL_X, PANEL_Y, 3.0, HEADER_H, ACCENT, c);
    canvas.text(FONT_TITLE, PANEL_X + 9.0, PANEL_Y + 5.0, 10.0, HIGHLIGHT, "INVENTORY", false, c);
    canvas.text(FONT_SMALL, PANEL_X + PANEL_W - 8.0, PANEL_Y + 7.5, 6.0, TEXT_DIM, "SURVIVAL  //  E TO CLOSE", true, c);
    canvas.frame(PANEL_X, PANEL_Y, PANEL_W, PANEL_H, EDGE, c);
    canvas.brackets(PANEL_X - 2.0, PANEL_Y - 2.0, PANEL_W + 4.0, PANEL_H + 4.0, 8.0, [ACCENT[0], ACCENT[1], ACCENT[2], 0.9], c);

    // The character window: clear, with a frame and MW2 brackets.
    canvas.quad(bx, by + bh - 26.0, bw, 26.0, [0.0, 0.0, 0.0, 0.35], "gradient_fadein_fadebottom", [0.0, 0.0, 1.0, 1.0], c);
    canvas.frame(bx, by, bw, bh, EDGE, c);
    canvas.brackets(bx, by, bw, bh, 5.0, ACCENT, c);
    canvas.text(FONT_SMALL, bx + 4.0, by + bh - 9.0, 5.5, [HIGHLIGHT[0], HIGHLIGHT[1], HIGHLIGHT[2], 0.75], "OPERATOR", false, c);

    // Section labels.
    canvas.text(FONT_SMALL, 22.0, -100.0, 6.0, TEXT_DIM, "CRAFTING", false, c);
    canvas.fill(22.0, -92.5, 90.0, 0.5, EDGE, c);
    canvas.text(FONT_TITLE, 68.0, -73.5, 11.0, TEXT_DIM, ">", false, c);
    canvas.text(FONT_SMALL, GRID_X, -6.5, 6.0, TEXT_DIM, "BACKPACK", false, c);
    canvas.fill(GRID_X + 38.0, -3.5, 158.0, 0.5, EDGE, c);
    canvas.fill(GRID_X, 75.0, 196.0, 0.5, [ACCENT[0], ACCENT[1], ACCENT[2], 0.45], c);

    // The slots.
    for (slot, x, y, size) in layout() {
        let hover = hovered == Some(slot);
        let is_result = slot == McSlot::Result;
        canvas.fill(x, y, size, size, if hover { SLOT_HOVER } else { SLOT }, c);
        canvas.frame(x, y, size, size, if hover { HIGHLIGHT } else if is_result { [ACCENT[0], ACCENT[1], ACCENT[2], 0.55] } else { SLOT_EDGE }, c);
        if let McSlot::Inventory(i) = slot
            && i < 9
            && i == ui.selected
        {
            canvas.fill(x, y + size - 1.5, size, 1.5, ACCENT, c);
        }
        if let Some(stack) = stack_at(ui, slot).cloned() {
            draw_stack(canvas, ui, weapons, &stack, x, y, size, c);
        } else if let McSlot::Inventory(i) = slot
            && (36..=40).contains(&i)
        {
            // Empty armor and offhand slots name what they take.
            let label = match i {
                39 => "HEAD",
                38 => "CHEST",
                37 => "LEGS",
                36 => "FEET",
                _ => "OFF",
            };
            let px = 4.5;
            let w = canvas.fonts.get(FONT_SMALL).map_or(0.0, |def| ui_text_width(def, label, px / ui_text_height(1.0)));
            canvas.text(FONT_SMALL, x + (size - w) * 0.5, y + (size - px) * 0.5 - 0.5, px, [1.0, 1.0, 1.0, 0.22], label, false, c);
        }
    }

    // The carried stack follows the mouse; otherwise the hovered item's card.
    let Some(mouse) = mouse else { return };
    let (vx, vy) = virtual_mouse(canvas, mouse);
    if let Some(stack) = ui.cursor.clone() {
        draw_stack(canvas, ui, weapons, &stack, vx - 10.0, vy - 10.0, 20.0, c);
        return;
    }
    let Some(slot) = hovered else { return };
    let Some(stack) = stack_at(ui, slot).cloned() else { return };
    let name = stack_name(&stack, ui, weapons, strings, gaps).to_uppercase();
    let sub = if stack.weapon.is_some() { "PRIMARY WEAPON".to_owned() } else { stack.id.clone() };
    draw_card(canvas, &name, &sub, vx, vy);
}

/// The mouse in the screens' centred virtual space.
fn virtual_mouse(canvas: &Canvas<'_>, mouse: Vec2) -> (f32, f32) {
    let [sx, sy] = canvas.surface.placement().scale_virtual_to_real;
    let (w_real, h_real) = (canvas.surface.width(), canvas.surface.height());
    ((mouse.x - w_real * 0.5) / sx / K, (mouse.y - h_real * 0.5) / sy / K)
}

/// A hovered item's card beside the mouse: its name, and a line under it.
fn draw_card(canvas: &mut Canvas<'_>, name: &str, sub: &str, vx: f32, vy: f32) {
    let c = (CENTER, CENTER);
    let title_px = 8.0;
    let sub_px = 5.5;
    let title_w = canvas.fonts.get(FONT_TITLE).map_or(60.0, |def| ui_text_width(def, name, title_px / ui_text_height(1.0)));
    let sub_w = canvas.fonts.get(FONT_SMALL).map_or(60.0, |def| ui_text_width(def, sub, sub_px / ui_text_height(1.0)));
    let w = title_w.max(sub_w) + 14.0;
    let (x, y) = (vx + 10.0, vy - 22.0);
    canvas.fill(x, y, w, 22.0, [0.02, 0.025, 0.028, 0.94], c);
    canvas.frame(x, y, w, 22.0, EDGE, c);
    canvas.fill(x, y, 2.0, 22.0, ACCENT, c);
    canvas.text(FONT_TITLE, x + 7.0, y + 3.0, title_px, HIGHLIGHT, name, false, c);
    canvas.text(FONT_SMALL, x + 7.0, y + 13.5, sub_px, TEXT_DIM, sub, false, c);
}

fn in_rect(surface: &crate::surface::Hud2dSurface, mouse: Vec2, x: f32, y: f32, w: f32, h: f32) -> bool {
    let r = surface.apply_rect(x * K, y * K, w * K, h * K, CENTER, CENTER);
    mouse.x >= r.x && mouse.x < r.x + r.w && mouse.y >= r.y && mouse.y < r.y + r.h
}

fn tab_at(surface: &crate::surface::Hud2dSurface, mouse: Vec2) -> Option<usize> {
    (0..CREATIVE_TABS.len()).find(|&i| in_rect(surface, mouse, TAB_X + i as f32 * TAB_P, TAB_Y, TAB_S, TAB_S))
}

fn creative_hit(surface: &crate::surface::Hud2dSurface, mouse: Vec2) -> Option<CreativeHit> {
    if let Some(tab) = tab_at(surface, mouse) {
        return Some(CreativeHit::Tab(tab));
    }
    let cell = (0..CREATIVE_COLUMNS * CREATIVE_ROWS).find(|&i| {
        let (col, row) = (i % CREATIVE_COLUMNS, i / CREATIVE_COLUMNS);
        in_rect(surface, mouse, GRID_X + col as f32 * P, LIST_Y + row as f32 * P, S, S)
    });
    if let Some(cell) = cell {
        return Some(CreativeHit::Cell(cell));
    }
    if let Some(slot) = (0..frame::minecraft_ui::MC_HOTBAR).find(|&i| in_rect(surface, mouse, GRID_X + i as f32 * P, HOTBAR_Y, S, S)) {
        return Some(CreativeHit::Hotbar(slot));
    }
    in_rect(surface, mouse, SCROLL_X, LIST_Y, SCROLL_W, SCROLL_H).then_some(CreativeHit::Scroll)
}

/// How far down the creative list's scroll bar the mouse is, 0 to 1.
fn scroll_fraction(surface: &crate::surface::Hud2dSurface, mouse: Vec2) -> f32 {
    let r = surface.apply_rect(SCROLL_X * K, LIST_Y * K, SCROLL_W * K, SCROLL_H * K, CENTER, CENTER);
    ((mouse.y - r.y) / r.h.max(1.0)).clamp(0.0, 1.0)
}

/// The item a cell of the creative grid shows, as the list is scrolled.
fn listed(ui: &MinecraftUi, cell: usize) -> Option<String> {
    ui.creative_items.get(ui.creative_row * CREATIVE_COLUMNS + cell).cloned()
}

/// The last row the creative list can scroll to.
fn last_row(ui: &MinecraftUi) -> usize {
    ui.creative_items.len().div_ceil(CREATIVE_COLUMNS).saturating_sub(CREATIVE_ROWS)
}

/// The creative screen's input, as vanilla's: the wheel and the scroll bar move the list; a
/// click on a listed item takes it (middle click a stack); a click on the list while
/// carrying throws the carried stack away; a number key over a listed item puts a stack in
/// that hotbar slot; the hotbar row works as in the inventory; a click outside throws.
#[allow(clippy::too_many_arguments)]
fn handle_creative(
    ui: &mut MinecraftUi,
    input: &mut ScreenInput,
    buttons: &ButtonInput<MouseButton>,
    hit: Option<CreativeHit>,
    inside: bool,
    shift: bool,
    digit: Option<usize>,
    scroll: i32,
    fraction: Option<f32>,
) {
    let last = last_row(ui);
    if scroll != 0 {
        ui.creative_row = (ui.creative_row as i32 + scroll).clamp(0, last as i32) as usize;
    }
    if buttons.just_pressed(MouseButton::Left) && hit == Some(CreativeHit::Scroll) {
        input.scrolling = true;
    }
    if !buttons.pressed(MouseButton::Left) {
        input.scrolling = false;
    }
    if input.scrolling
        && let Some(fraction) = fraction
    {
        ui.creative_row = (fraction * last as f32).round() as usize;
    }
    ui.creative_row = ui.creative_row.min(last);
    match (digit, hit) {
        (Some(hotbar), Some(CreativeHit::Cell(cell))) => {
            if let Some(id) = listed(ui, cell) {
                ui.clicks.push(McClick::CreativeHotbar { id, hotbar });
            }
        }
        (Some(hotbar), Some(CreativeHit::Hotbar(slot))) => ui.clicks.push(McClick::Swap { slot: McSlot::Inventory(slot), hotbar }),
        _ => {}
    }
    for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
        if !buttons.just_pressed(button) {
            continue;
        }
        let (right, middle) = (button == MouseButton::Right, button == MouseButton::Middle);
        match hit {
            Some(CreativeHit::Cell(cell)) => match listed(ui, cell) {
                Some(id) => ui.clicks.push(McClick::CreativeTake { id, right, shift: shift || middle }),
                None if ui.cursor.is_some() && button == MouseButton::Left => ui.clicks.push(McClick::CreativeDestroy),
                None => {}
            },
            Some(CreativeHit::Hotbar(slot)) if !middle => ui.clicks.push(McClick::Slot { slot: McSlot::Inventory(slot), right, shift }),
            None if !inside && !middle => ui.clicks.push(McClick::Outside { right }),
            _ => {}
        }
    }
}

/// The creative tabs over the panel, each its item's icon; the open one lit, the hovered
/// one named.
fn draw_tabs(canvas: &mut Canvas<'_>, ui: &MinecraftUi, hovered: Option<usize>) {
    let c = (CENTER, CENTER);
    for (i, (_, icon)) in CREATIVE_TABS.iter().enumerate() {
        let open = i == ui.creative_tab;
        let x = TAB_X + i as f32 * TAB_P;
        let y = TAB_Y + if open { -2.0 } else { 0.0 };
        let fill = if open {
            [0.16, 0.09, 0.03, 0.94]
        } else if hovered == Some(i) {
            [0.24, 0.13, 0.05, 0.90]
        } else {
            [0.03, 0.025, 0.02, 0.88]
        };
        canvas.fill(x, y, TAB_S, TAB_S, fill, c);
        canvas.frame(x, y, TAB_S, TAB_S, if open { HIGHLIGHT } else { EDGE }, c);
        if open {
            canvas.fill(x, y + TAB_S - 1.5, TAB_S, 1.5, ACCENT, c);
        }
        if let Some(rect) = ui.icon_rects.get(*icon) {
            canvas.quad(x + 2.5, y + 2.5, TAB_S - 5.0, TAB_S - 5.0, [1.0; 4], ICONS, *rect, c);
        }
    }
    if let Some(i) = hovered {
        let name = CREATIVE_TABS[i].0;
        canvas.text(FONT_SMALL, TAB_X + i as f32 * TAB_P, TAB_Y - 9.0, 6.0, HIGHLIGHT, name, false, c);
    }
}

/// The creative screen.
#[allow(clippy::too_many_arguments)]
fn draw_creative(
    canvas: &mut Canvas<'_>,
    ui: &mut MinecraftUi,
    weapons: Option<&PreparedWeapons>,
    strings: Option<&PreparedLocalizedStrings>,
    gaps: &mut HudPresentationGaps,
    hit: Option<CreativeHit>,
    mouse: Option<Vec2>,
    typing: bool,
    now: f64,
) {
    let c = (CENTER, CENTER);
    // Dim the world; the panel, its header, edges and brackets, as the inventory's.
    canvas.fill(-2000.0, -2000.0, 4000.0, 4000.0, [0.0, 0.0, 0.0, 0.55], c);
    canvas.quad(-2000.0, -2000.0, 4000.0, 4000.0, [ACCENT[0], ACCENT[1], ACCENT[2], 0.07], "gradient_fadein_fadebottom", [0.0, 0.0, 1.0, 1.0], c);
    canvas.fill(PANEL_X, PANEL_Y + HEADER_H, PANEL_W, CREATIVE_H - HEADER_H, PANEL, c);
    canvas.fill(PANEL_X, PANEL_Y, PANEL_W, HEADER_H, HEADER, c);
    canvas.quad(PANEL_X, PANEL_Y, PANEL_W, HEADER_H, [ACCENT[0], ACCENT[1], ACCENT[2], 0.22], "gradient_fadein_fadebottom", [0.0, 0.0, 1.0, 1.0], c);
    canvas.fill(PANEL_X, PANEL_Y + HEADER_H - 1.0, PANEL_W, 1.0, [ACCENT[0], ACCENT[1], ACCENT[2], 0.85], c);
    canvas.fill(PANEL_X, PANEL_Y, 3.0, HEADER_H, ACCENT, c);
    canvas.text(FONT_TITLE, PANEL_X + 9.0, PANEL_Y + 5.0, 10.0, HIGHLIGHT, "CREATIVE", false, c);
    let hint = if typing { "TYPE TO SEARCH  //  ESC TO CLOSE" } else { "E TO CLOSE" };
    canvas.text(FONT_SMALL, PANEL_X + PANEL_W - 8.0, PANEL_Y + 7.5, 6.0, TEXT_DIM, hint, true, c);
    canvas.frame(PANEL_X, PANEL_Y, PANEL_W, CREATIVE_H, EDGE, c);
    canvas.brackets(PANEL_X - 2.0, PANEL_Y - 2.0, PANEL_W + 4.0, CREATIVE_H + 4.0, 8.0, [ACCENT[0], ACCENT[1], ACCENT[2], 0.9], c);

    // The search field, or the tab's name; and how many items the list holds.
    let field_w = CREATIVE_COLUMNS as f32 * P - 2.0;
    if ui.creative_tab == CREATIVE_SEARCH {
        canvas.fill(GRID_X, FIELD_Y, field_w, FIELD_H, [0.0, 0.0, 0.0, 0.55], c);
        canvas.frame(GRID_X, FIELD_Y, field_w, FIELD_H, if typing { HIGHLIGHT } else { EDGE }, c);
        let caret = if (now * 2.0) as i64 % 2 == 0 { "_" } else { "" };
        let (text, color) = if ui.creative_search.is_empty() {
            (format!("SEARCH ITEMS{caret}"), TEXT_DIM)
        } else {
            (format!("{}{caret}", ui.creative_search.to_uppercase()), TEXT)
        };
        canvas.text(FONT_SMALL, GRID_X + 3.0, FIELD_Y + 3.0, 6.0, color, &text, false, c);
    } else {
        canvas.text(FONT_SMALL, GRID_X, FIELD_Y + 3.0, 6.0, TEXT_DIM, CREATIVE_TABS[ui.creative_tab.min(CREATIVE_TABS.len() - 1)].0, false, c);
    }
    let count = format!("{} ITEMS", ui.creative_items.len());
    canvas.text(FONT_SMALL, SCROLL_X + SCROLL_W, FIELD_Y + 3.0, 5.5, TEXT_DIM, &count, true, c);

    // The list.
    for cell in 0..CREATIVE_COLUMNS * CREATIVE_ROWS {
        let (x, y) = (GRID_X + (cell % CREATIVE_COLUMNS) as f32 * P, LIST_Y + (cell / CREATIVE_COLUMNS) as f32 * P);
        let hover = hit == Some(CreativeHit::Cell(cell));
        canvas.fill(x, y, S, S, if hover { SLOT_HOVER } else { SLOT }, c);
        canvas.frame(x, y, S, S, if hover { HIGHLIGHT } else { SLOT_EDGE }, c);
        if let Some(id) = listed(ui, cell) {
            let stack = McStack { id, count: 1, weapon: None, durability: None };
            draw_stack(canvas, ui, weapons, &stack, x, y, S, c);
        }
    }

    // The scroll bar: the thumb as long as the share of the list in view.
    let rows = ui.creative_items.len().div_ceil(CREATIVE_COLUMNS).max(1);
    let last = last_row(ui);
    canvas.fill(SCROLL_X, LIST_Y, SCROLL_W, SCROLL_H, [0.0, 0.0, 0.0, 0.45], c);
    canvas.frame(SCROLL_X, LIST_Y, SCROLL_W, SCROLL_H, SLOT_EDGE, c);
    let thumb_h = (SCROLL_H * CREATIVE_ROWS as f32 / rows as f32).clamp(8.0, SCROLL_H);
    let along = if last == 0 { 0.0 } else { ui.creative_row as f32 / last as f32 };
    let thumb_y = LIST_Y + (SCROLL_H - thumb_h) * along;
    let lit = hit == Some(CreativeHit::Scroll);
    canvas.fill(SCROLL_X + 1.0, thumb_y + 1.0, SCROLL_W - 2.0, thumb_h - 2.0, [ACCENT[0], ACCENT[1], ACCENT[2], if lit { 1.0 } else { 0.75 }], c);

    // The hotbar.
    canvas.text(FONT_SMALL, GRID_X, HOTBAR_Y - 9.5, 6.0, TEXT_DIM, "HOTBAR", false, c);
    canvas.fill(GRID_X + 30.0, HOTBAR_Y - 6.5, field_w - 30.0, 0.5, [ACCENT[0], ACCENT[1], ACCENT[2], 0.45], c);
    for i in 0..frame::minecraft_ui::MC_HOTBAR {
        let x = GRID_X + i as f32 * P;
        let hover = hit == Some(CreativeHit::Hotbar(i));
        canvas.fill(x, HOTBAR_Y, S, S, if hover { SLOT_HOVER } else { SLOT }, c);
        canvas.frame(x, HOTBAR_Y, S, S, if hover { HIGHLIGHT } else { SLOT_EDGE }, c);
        if i == ui.selected {
            canvas.fill(x, HOTBAR_Y + S - 1.5, S, 1.5, ACCENT, c);
        }
        if let Some(stack) = ui.slots.get(i).and_then(Option::as_ref).cloned() {
            draw_stack(canvas, ui, weapons, &stack, x, HOTBAR_Y, S, c);
        }
    }

    let tab = match hit {
        Some(CreativeHit::Tab(tab)) => Some(tab),
        _ => None,
    };
    draw_tabs(canvas, ui, tab);

    // The carried stack follows the mouse; otherwise the hovered item's card.
    let Some(mouse) = mouse else { return };
    let (vx, vy) = virtual_mouse(canvas, mouse);
    if let Some(stack) = ui.cursor.clone() {
        draw_stack(canvas, ui, weapons, &stack, vx - 10.0, vy - 10.0, 20.0, c);
        return;
    }
    let card = match hit {
        Some(CreativeHit::Cell(cell)) => listed(ui, cell).map(|id| {
            let name = ui.names.get(&id).cloned().unwrap_or_else(|| id.clone());
            (name.to_uppercase(), id)
        }),
        Some(CreativeHit::Hotbar(slot)) => ui.slots.get(slot).and_then(Option::as_ref).cloned().map(|stack| {
            let name = stack_name(&stack, ui, weapons, strings, gaps).to_uppercase();
            (name, if stack.weapon.is_some() { "PRIMARY WEAPON".to_owned() } else { stack.id })
        }),
        _ => None,
    };
    if let Some((name, sub)) = card {
        draw_card(canvas, &name, &sub, vx, vy);
    }
}
