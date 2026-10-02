//! ZillaCraft's kaiju: Godzilla's, Zilla's, Kong's and King Kong's models (converted from the
//! mod's geometry) posed by the ported animations, Godzilla's dorsal plates' blue glow and the
//! atomic breath's beam while he breathes, an enraged ape's glowing eyes and the boulders it
//! throws, and the red warnings on the ground before a move.

use crate::cow_render::entity_shade;
use crate::kaiju_server::KaijuView;
use crate::lighting::SkyLight;
use crate::mesh::{Atlas, ChunkMesh, Vertex};
use crate::pack::ResourceId;
use glam::Vec3;
use kaiju::anim::{AnimState, Rig, model_scale};
use kaiju::ape_anim::ApeRig;
use kaiju::model::{Mat34, Model, PartPose, Quad, entity_transform};
use kaiju::zila_anim::ZilaRig;
use std::sync::OnceLock;

struct Loaded<R> {
    model: Model,
    rig: R,
    scale: f32,
}

fn godzilla() -> Option<&'static Loaded<Rig>> {
    static LOADED: OnceLock<Option<Loaded<Rig>>> = OnceLock::new();
    LOADED
        .get_or_init(|| {
            let model = Model::parse(kaiju::GODZILLA_MODEL).ok()?;
            let rig = Rig::new(&model).ok()?;
            let scale = model_scale(&model, kaiju::godzilla::HEIGHT);
            Some(Loaded { model, rig, scale })
        })
        .as_ref()
}

fn zilla() -> Option<&'static Loaded<ZilaRig>> {
    static LOADED: OnceLock<Option<Loaded<ZilaRig>>> = OnceLock::new();
    LOADED
        .get_or_init(|| {
            let model = Model::parse(kaiju::ZILA_MODEL).ok()?;
            let rig = ZilaRig::new(&model).ok()?;
            let scale = model_scale(&model, kaiju::zila::HEIGHT);
            Some(Loaded { model, rig, scale })
        })
        .as_ref()
}

/// An ape: Kong's head a little oversized, as it reads better at a distance (`KongRenderer`).
fn ape(species: &'static kaiju::Species) -> Option<&'static Loaded<ApeRig>> {
    static KONG: OnceLock<Option<Loaded<ApeRig>>> = OnceLock::new();
    static KING_KONG: OnceLock<Option<Loaded<ApeRig>>> = OnceLock::new();
    let king = species.id == kaiju::kong::KING_KONG.id;
    let (loaded, json, head_scale) = if king { (&KING_KONG, kaiju::KING_KONG_MODEL, 1.0) } else { (&KONG, kaiju::KONG_MODEL, 1.15) };
    loaded
        .get_or_init(|| {
            let model = Model::parse(json).ok()?;
            let rig = ApeRig::new(&model, species, head_scale).ok()?;
            let scale = model_scale(&model, species.height);
            Some(Loaded { model, rig, scale })
        })
        .as_ref()
}

fn boulder() -> Option<&'static Model> {
    static MODEL: OnceLock<Option<Model>> = OnceLock::new();
    MODEL.get_or_init(|| Model::parse(kaiju::BOULDER_MODEL).ok()).as_ref()
}

/// A kaiju's model posed for this frame: the model, its pose, its scale, its skin, and an
/// ape's glowing eyes.
fn posed(v: &KaijuView, state: &AnimState) -> Option<(&'static Model, Vec<PartPose>, f32, &'static str, Option<&'static str>)> {
    let species = v.species;
    if species.id == kaiju::godzilla::SPECIES.id {
        godzilla().map(|l| (&l.model, l.rig.pose(&l.model, state, false), l.scale, kaiju::pack::SKIN, None))
    } else if species.ape.is_some() {
        let king = species.id == kaiju::kong::KING_KONG.id;
        let (skin, eyes) = if king { (kaiju::pack::KING_KONG_SKIN, kaiju::pack::KING_KONG_EYES) } else { (kaiju::pack::KONG_SKIN, kaiju::pack::KONG_EYES) };
        ape(species).map(|l| (&l.model, l.rig.pose(&l.model, state), l.scale, skin, Some(eyes)))
    } else {
        zilla().map(|l| (&l.model, l.rig.pose(&l.model, state), l.scale, kaiju::pack::ZILA_SKIN, None))
    }
}

fn region(atlas: &Atlas, id: &str) -> Option<[f32; 4]> {
    let id = ResourceId::parse(id).ok()?;
    atlas.contains(&id).then(|| atlas.entity_region(&id))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_degrees(a: f32, b: f32, t: f32) -> f32 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    a + d * t
}

/// What is on screen `t` of a tick from `p` to `v`.
fn shown(p: &KaijuView, v: &KaijuView, t: f32) -> KaijuView {
    let feet = if (v.feet[0] - p.feet[0]).abs() > 8.0 {
        v.feet
    } else {
        std::array::from_fn(|i| p.feet[i] + (v.feet[i] - p.feet[i]) * f64::from(t))
    };
    KaijuView {
        feet,
        yaw: lerp_degrees(p.yaw, v.yaw, t),
        head_yaw: lerp(p.head_yaw, v.head_yaw, t),
        head_pitch: lerp(p.head_pitch, v.head_pitch, t),
        walk_position: lerp(p.walk_position, v.walk_position, t),
        walk_speed: lerp(p.walk_speed, v.walk_speed, t),
        breath_amount: lerp(p.breath_amount, v.breath_amount, t),
        ..v.clone()
    }
}

/// Where the motion towards a newly arrived tick starts: what was on screen when it came,
/// `t` of the way from `previous` to `current`. The server thread's ticks come a frame late
/// or early, and starting from anywhere else jerks a kaiju back or ahead (its stride most).
pub fn blend(previous: &[KaijuView], current: &[KaijuView], t: f32) -> Vec<KaijuView> {
    current.iter().map(|v| shown(previous.iter().find(|p| p.id == v.id).unwrap_or(v), v, t)).collect()
}

/// Appends every kaiju, `partial` of a tick from `previous` to `views`: the skin, an enraged
/// ape's eyes and its boulders to `models`, Godzilla's glow and beam and every kaiju's warnings
/// to `translucent`.
#[allow(clippy::too_many_arguments)]
pub fn append_kaiju(
    models: &mut ChunkMesh,
    translucent: &mut ChunkMesh,
    views: &[KaijuView],
    previous: &[KaijuView],
    atlas: &Atlas,
    light: &SkyLight,
    partial: f32,
    age_ticks: f32,
) {
    let glow = region(atlas, kaiju::pack::GLOW);
    let beam = region(atlas, kaiju::pack::BEAM);
    let rock = region(atlas, kaiju::pack::BOULDER);
    let t = partial.clamp(0.0, 1.0);
    for v in views {
        let before = previous.iter().find(|q| q.id == v.id);
        let s = shown(before.unwrap_or(v), v, t);
        let (feet, yaw) = (s.feet, s.yaw);
        let breathing = v.breath_ticks > 0;
        let state = AnimState {
            walk_position: s.walk_position,
            walk_speed: s.walk_speed,
            age_ticks,
            head_yaw: s.head_yaw,
            head_pitch: s.head_pitch,
            breath_amount: s.breath_amount,
            breath_time: if breathing { v.breath_ticks as f32 + t } else { 0.0 },
            movement: v.movement,
            move_time: v.move_ticks as f32 + t,
            move_direction: v.move_direction,
        };
        // An ape's boulders fly on whatever becomes of it.
        if let (Some(rock), Some(model)) = (rock, boulder()) {
            for b in &v.boulders {
                let was = before.and_then(|p| p.boulders.iter().find(|q| q.id == b.id));
                append_boulder(models, translucent, model, b, was, rock, beam, light, t);
            }
        }
        let is_godzilla = v.species.id == kaiju::godzilla::SPECIES.id;
        let Some((model, pose, scale, skin, eyes)) = posed(v, &state) else { continue };
        let Some(skin) = region(atlas, skin) else { continue };
        // It keels over as it dies, as every mob does (`setupRotations`: 90 degrees over a second).
        let dying = if v.death_ticks > 0 { ((v.death_ticks as f32 + t - 1.0) / 20.0 * 1.6).sqrt().min(1.0) } else { 0.0 };
        let mut transform = entity_transform(scale, yaw);
        if dying > 0.0 {
            let turn = Mat34::rotation_y((180.0 - yaw).to_radians());
            let back = Mat34::rotation_y((yaw - 180.0).to_radians());
            transform = turn.mul(&Mat34::rotation_zyx(dying * std::f32::consts::FRAC_PI_2, 0.0, 0.0)).mul(&back).mul(&transform);
        }
        // Lit as the air round its middle is.
        let middle = feet[1] + v.species.height * 0.6;
        let probe = (feet[0].floor() as i32, middle.floor() as i32, feet[2].floor() as i32);
        let (sky, block) = (f32::from(light.get(probe)), f32::from(light.get_block(probe)));
        let tint = if v.hurt_ticks > 0 || v.death_ticks > 0 { [1.0, 0.55, 0.55] } else { [1.0, 1.0, 1.0] };

        let mut quads: Vec<Quad> = Vec::new();
        model.mesh(&pose, &transform, &mut quads);
        push_quads(models, &quads, feet, skin, tint, 1.0, sky, block, true);
        // Enraged, its eyes glow, even in the dark (`KongEyesLayer`).
        if let Some(eyes) = eyes.filter(|_| v.enraged && v.death_ticks == 0).and_then(|id| region(atlas, id)) {
            push_quads(models, &quads, feet, eyes, [1.0; 3], 1.0, 15.0, 15.0, false);
        }

        if breathing && v.death_ticks == 0 && is_godzilla {
            if let (Some(glow), Some(l)) = (glow, godzilla()) {
                let intensity = (state.breath_amount * 1.4).min(1.0);
                let mut lit: Vec<Quad> = Vec::new();
                let pose = l.rig.pose(&l.model, &state, true);
                // A hair larger, so the glow sits on the plates rather than fighting them.
                l.model.mesh(&pose, &entity_transform(l.scale * 1.004, yaw), &mut lit);
                push_quads(translucent, &lit, feet, glow, [intensity; 3], 1.0, 15.0, 15.0, false);
            }
        }
        if let Some(beam) = beam {
            if let Some((from, to)) = v.beam {
                push_beam(translucent, beam, from, to, 2.6, [0.55, 0.85, 1.0]);
                push_beam(translucent, beam, from, to, 1.0, [1.0, 1.0, 1.0]);
            }
            if let Some((at, strength)) = v.charge {
                let size = 1.0 + strength as f64 * 3.0;
                push_beam(translucent, beam, [at[0], at[1] - size / 2.0, at[2]], [at[0], at[1] + size / 2.0, at[2]], size as f32, [0.45, 0.75, 1.0]);
            }
            for at in &v.warnings {
                push_ground_mark(translucent, beam, *at, 1.2, [1.0, 0.25, 0.1]);
            }
            for at in &v.shockwave {
                push_ground_mark(translucent, beam, [at[0], at[1] + 0.1, at[2]], 2.0, [0.85, 0.75, 0.55]);
            }
        }
    }
}

/// A thrown boulder (`KongBoulderRenderer`), tumbling as it flies, and the red ring round where
/// it will land.
#[allow(clippy::too_many_arguments)]
fn append_boulder(
    models: &mut ChunkMesh,
    translucent: &mut ChunkMesh,
    model: &Model,
    b: &kaiju::ape::Boulder,
    was: Option<&kaiju::ape::Boulder>,
    rock: [f32; 4],
    beam: Option<[f32; 4]>,
    light: &SkyLight,
    t: f32,
) {
    let from = was.map_or(b.at, |w| w.at);
    let at: [f64; 3] = std::array::from_fn(|i| from[i] + (b.at[i] - from[i]) * f64::from(t));
    let centre = [at[0], at[1] + b.size() / 2.0, at[2]];
    let age = b.life as f32 + t;
    // Its model is a 16-pixel rock, drawn 7.8 times its box at the design size.
    let size = 7.8 * b.scale as f32;
    let transform = Mat34::rotation_zyx(0.0, 0.0, (age * 17.0).to_radians())
        .mul(&Mat34::rotation_y((age * 6.0).to_radians()))
        .mul(&Mat34::scale(size, size, size));
    let mut quads = Vec::new();
    model.mesh(&model.rest_pose(), &transform, &mut quads);
    let probe = (centre[0].floor() as i32, centre[1].floor() as i32, centre[2].floor() as i32);
    let (sky, block) = (f32::from(light.get(probe)), f32::from(light.get_block(probe)));
    push_quads(models, &quads, centre, rock, [1.0; 3], 1.0, sky, block, true);
    if let Some(beam) = beam {
        let radius = b.blast_radius();
        let points = (28.0 * b.scale) as usize;
        for i in 0..points {
            let a = std::f64::consts::TAU * i as f64 / points as f64;
            let p = [b.landing[0] + a.cos() * radius, b.landing[1], b.landing[2] + a.sin() * radius];
            push_ground_mark(translucent, beam, p, 1.2, [1.0, 0.25, 0.1]);
        }
        push_ground_mark(translucent, beam, b.landing, 1.2 * b.scale as f32, [1.0, 0.25, 0.1]);
    }
}

#[allow(clippy::too_many_arguments)]
fn push_quads(mesh: &mut ChunkMesh, quads: &[Quad], feet: [f64; 3], region: [f32; 4], tint: [f32; 3], alpha: f32, sky: f32, block: f32, shaded: bool) {
    let origin = Vec3::new(feet[0] as f32, feet[1] as f32, feet[2] as f32);
    for q in quads {
        let p: [Vec3; 4] = q.positions.map(Vec3::from_array);
        let normal = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        let shade = if shaded { entity_shade(normal) } else { 1.0 };
        let start = mesh.vertices.len() as u32;
        for (corner, uv) in p.iter().zip(q.uvs) {
            mesh.vertices.push(Vertex {
                position: (origin + *corner).to_array(),
                uv: [region[0] + (region[2] - region[0]) * uv[0], region[1] + (region[3] - region[1]) * uv[1]],
                color: [shade * tint[0], shade * tint[1], shade * tint[2], alpha],
                sky_light: sky,
                block_light: block,
            });
        }
        mesh.indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
        mesh.faces += 1;
    }
}

/// A glowing square-section bar from `from` to `to`, `width` blocks across, full bright.
fn push_beam(mesh: &mut ChunkMesh, region: [f32; 4], from: [f64; 3], to: [f64; 3], width: f32, color: [f32; 3]) {
    let a = Vec3::new(from[0] as f32, from[1] as f32, from[2] as f32);
    let b = Vec3::new(to[0] as f32, to[1] as f32, to[2] as f32);
    let axis = (b - a).normalize_or_zero();
    if axis == Vec3::ZERO {
        return;
    }
    let up = if axis.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let side = axis.cross(up).normalize() * (width / 2.0);
    let lift = side.cross(axis).normalize() * (width / 2.0);
    let length = (b - a).length();
    for (u, v) in [(side, lift), (lift, -side), (-side, -lift), (-lift, side)] {
        let corners = [a + u + v, a - u + v, b - u + v, b + u + v];
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, length], [0.0, length]];
        push_flat(mesh, region, corners, uvs, color);
    }
}

/// A flat mark lying on the ground at `at`.
fn push_ground_mark(mesh: &mut ChunkMesh, region: [f32; 4], at: [f64; 3], size: f32, color: [f32; 3]) {
    let c = Vec3::new(at[0] as f32, at[1] as f32 + 0.05, at[2] as f32);
    let h = size / 2.0;
    let corners = [c + Vec3::new(-h, 0.0, -h), c + Vec3::new(-h, 0.0, h), c + Vec3::new(h, 0.0, h), c + Vec3::new(h, 0.0, -h)];
    push_flat(mesh, region, corners, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], color);
}

fn push_flat(mesh: &mut ChunkMesh, region: [f32; 4], corners: [Vec3; 4], uvs: [[f32; 2]; 4], color: [f32; 3]) {
    let start = mesh.vertices.len() as u32;
    for (corner, uv) in corners.iter().zip(uvs) {
        mesh.vertices.push(Vertex {
            position: corner.to_array(),
            // The beam texture only varies across the beam: its middle row is used all along it.
            uv: [region[0] + (region[2] - region[0]) * uv[0], region[1] + (region[3] - region[1]) * 0.5],
            color: [color[0], color[1], color[2], 1.0],
            sky_light: 15.0,
            block_light: 15.0,
        });
    }
    // Both sides: a beam is seen from anywhere.
    mesh.indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
    mesh.indices.extend_from_slice(&[start, start + 2, start + 1, start, start + 3, start + 2]);
    mesh.faces += 2;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kaiju_server::KaijuWorld;
    use kaiju::anim::Move;

    fn standing(species: &'static kaiju::Species, yaw: f32) -> KaijuView {
        KaijuView {
            id: 1,
            species,
            feet: [0.0; 3],
            yaw,
            head_yaw: 0.0,
            head_pitch: 0.0,
            walk_position: 0.0,
            walk_speed: 0.0,
            movement: Move::None,
            move_ticks: 0,
            move_direction: 1,
            breath_ticks: 0,
            breath_amount: 0.0,
            health: species.max_health,
            max_health: species.max_health,
            death_ticks: 0,
            hurt_ticks: 0,
            enraged: false,
            boulders: Vec::new(),
            beam: None,
            charge: None,
            shockwave: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// The share of the corners of a kaiju's model, standing and facing `yaw`, that lie outside
    /// every one of its hitboxes (by more than a quarter block): what bullets would pass through.
    fn uncovered(species: &'static kaiju::Species, model: &Model, pose: &[kaiju::model::PartPose], yaw: f32) -> f32 {
        let mut quads = Vec::new();
        model.mesh(pose, &entity_transform(model_scale(model, species.height), yaw), &mut quads);
        let boxes = KaijuWorld::boxes(&[standing(species, yaw)]);
        let corners: Vec<[f32; 3]> = quads.iter().flat_map(|q| q.positions).collect();
        let outside = corners
            .iter()
            .filter(|p| {
                !boxes.iter().any(|(_, b)| {
                    (0..3).all(|i| f64::from(p[i]) >= b[i] - 0.25 && f64::from(p[i]) <= b[i + 3] + 0.25)
                })
            })
            .count();
        outside as f32 / corners.len() as f32
    }

    #[test]
    fn hitboxes_cover_each_kaiju_however_it_faces() {
        let godzilla = godzilla().unwrap();
        let zilla = zilla().unwrap();
        for yaw in [0.0, 37.0, 90.0, 145.0, 200.0, 290.0] {
            let pose = godzilla.rig.pose(&godzilla.model, &AnimState::default(), false);
            let share = uncovered(&kaiju::godzilla::SPECIES, &godzilla.model, &pose, yaw);
            assert!(share < 0.02, "Godzilla facing {yaw}: {:.1}% of his model is outside his hitboxes", share * 100.0);
            let pose = zilla.rig.pose(&zilla.model, &AnimState::default());
            let share = uncovered(&kaiju::zila::SPECIES, &zilla.model, &pose, yaw);
            assert!(share < 0.02, "Zilla facing {yaw}: {:.1}% of its model is outside its hitboxes", share * 100.0);
            for species in [&kaiju::kong::KONG, &kaiju::kong::KING_KONG] {
                let ape = ape(species).unwrap();
                let pose = ape.rig.pose(&ape.model, &AnimState::default());
                let share = uncovered(species, &ape.model, &pose, yaw);
                assert!(share < 0.02, "{} facing {yaw}: {:.1}% of his model is outside his hitboxes", species.name, share * 100.0);
            }
        }
    }
}
