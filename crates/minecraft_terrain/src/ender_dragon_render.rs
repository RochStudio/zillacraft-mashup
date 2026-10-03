//! The Ender Dragon as vanilla draws it (`EnderDragonRenderer`, `EnderDragonModel`): turned and
//! pitched by where it flew a few ticks ago, its neck and tail laid out along its flight history,
//! wings flapping, jaw working; its eyes glowing; the rays bursting out of it as it dies; and
//! its fireballs and the clouds of breath they leave.

use crate::ender_dragon::{DragonView, HISTORY};
use crate::kaiju_render::{push_ground_mark, push_quads};
use crate::lighting::SkyLight;
use crate::mesh::{Atlas, ChunkMesh, Vertex};
use crate::pack::ResourceId;
use glam::{Mat4, Quat, Vec3};
use kaiju::model::{Mat34, Model, PartPose, Quad};
use std::sync::OnceLock;

const DEG: f32 = std::f32::consts::PI / 180.0;
const TAU: f32 = std::f32::consts::TAU;

/// The model and the parts the animation drives, looked up once.
struct Rig {
    model: Model,
    head: usize,
    jaw: usize,
    neck: [usize; 5],
    tail: [usize; 12],
    body: usize,
    /// Left and right: wing, wing tip, front leg, its tip, its foot, hind leg, its tip, its foot.
    limbs: [[usize; 8]; 2],
}

fn rig() -> Option<&'static Rig> {
    static RIG: OnceLock<Option<Rig>> = OnceLock::new();
    RIG.get_or_init(|| {
        let model = Model::parse(include_str!("../assets/ender_dragon.json")).ok()?;
        let find = |name: &str| model.find_named(name);
        let limbs = |side: &str| -> Option<[usize; 8]> {
            let names = ["wing", "wing_tip", "front_leg", "front_leg_tip", "front_foot", "hind_leg", "hind_leg_tip", "hind_foot"];
            let mut out = [0; 8];
            for (slot, name) in out.iter_mut().zip(names) {
                *slot = find(&format!("{side}_{name}"))?;
            }
            Some(out)
        };
        let mut neck = [0; 5];
        for (i, slot) in neck.iter_mut().enumerate() {
            *slot = find(&format!("neck{i}"))?;
        }
        let mut tail = [0; 12];
        for (i, slot) in tail.iter_mut().enumerate() {
            *slot = find(&format!("tail{i}"))?;
        }
        Some(Rig {
            head: find("head")?,
            jaw: find("jaw")?,
            body: find("body")?,
            limbs: [limbs("left")?, limbs("right")?],
            neck,
            tail,
            model,
        })
    })
    .as_ref()
}

fn region(atlas: &Atlas, name: &str) -> Option<[f32; 4]> {
    let id = ResourceId::parse(&format!("minecraft:entity/enderdragon/{name}")).ok()?;
    atlas.contains(&id).then(|| atlas.entity_region(&id))
}

/// `Mth.wrapDegrees`.
fn wrap(degrees: f32) -> f32 {
    let mut r = degrees % 360.0;
    if r >= 180.0 {
        r -= 360.0;
    }
    if r < -180.0 {
        r += 360.0;
    }
    r
}

/// A flight history sample `index` ticks back, `t` of the way from the one before it
/// (`DragonFlightHistory.get(index, partialTick)`): (Y, yaw in degrees).
fn sample(history: &[(f64, f32)], index: usize, t: f32) -> (f64, f32) {
    let at = |i: usize| history.get(i.min(HISTORY - 1)).copied().unwrap_or((0.0, 0.0));
    let (newer, older) = (at(index), at(index + 1));
    (older.0 + f64::from(t) * (newer.0 - older.0), older.1 + t * wrap(newer.1 - older.1))
}

/// `EnderDragonModel.setupAnim`.
fn pose(rig: &Rig, v: &DragonView, flap: f32, t: f32) -> Vec<PartPose> {
    let mut p = rig.model.rest_pose();
    let h = |i: usize| sample(&v.history, i, t);
    let f = flap * TAU;
    p[rig.jaw].x_rot = (f.sin() + 1.0) * 0.2;
    // The whole body bobs with its wingbeat.
    let g = (f - 1.0).sin() + 1.0;
    let g = (g * g + g * 2.0) * 0.05;
    p[0].y = (g - 2.0) * 16.0;
    p[0].z = -48.0;
    p[0].x_rot = g * 2.0 * DEG;

    let s6 = h(6);
    let d = wrap(h(5).1 - h(10).1);
    let mid = wrap(h(5).1 + d / 2.0);
    // How far a segment rises or dips against the sixth sample (`getHeadPartYOffset`): sitting,
    // the neck curls down by its place along it.
    let rise = |i: usize, s: (f64, f32)| if v.sitting { i as f32 } else if i == 6 { 0.0 } else { (s.0 - s6.0) as f32 };

    let first = p[rig.neck[0]];
    let (mut x, mut y, mut z) = (first.x, first.y, first.z);
    for (i, &part) in rig.neck.iter().enumerate() {
        let si = h(5 - i);
        let n = &mut p[part];
        n.y_rot = wrap(si.1 - s6.1) * DEG * 1.5;
        n.x_rot = (i as f32 * 0.45 + f).cos() * 0.15 + rise(i, si) * DEG * 1.5 * 5.0;
        n.z_rot = -wrap(si.1 - mid) * DEG * 1.5;
        (n.x, n.y, n.z) = (x, y, z);
        x -= n.y_rot.sin() * n.x_rot.cos() * 10.0;
        y += n.x_rot.sin() * 10.0;
        z -= n.y_rot.cos() * n.x_rot.cos() * 10.0;
    }
    let s0 = h(0);
    let head = &mut p[rig.head];
    (head.x, head.y, head.z) = (x, y, z);
    head.y_rot = wrap(s0.1 - s6.1) * DEG;
    head.x_rot = wrap(rise(6, s0)) * DEG * 1.5 * 5.0;
    head.z_rot = -wrap(s0.1 - mid) * DEG;

    p[rig.body].z_rot = -d * 1.5 * DEG;

    let [left, right] = rig.limbs;
    let wing = (0.125 - f.cos() * 0.2, -0.25, -(f.sin() + 0.125) * 0.8);
    let tip = ((f + 2.0).sin() + 0.5) * 0.75;
    (p[left[0]].x_rot, p[left[0]].y_rot, p[left[0]].z_rot) = wing;
    (p[right[0]].x_rot, p[right[0]].y_rot, p[right[0]].z_rot) = (wing.0, -wing.1, -wing.2);
    p[left[1]].z_rot = tip;
    p[right[1]].z_rot = -tip;
    for side in [left, right] {
        let pitches = [1.3 + g * 0.1, -0.5 - g * 0.1, 0.75 + g * 0.1, 1.0 + g * 0.1, 0.5 + g * 0.1, 0.75 + g * 0.1];
        for (&part, pitch) in side[2..].iter().zip(pitches) {
            p[part].x_rot = pitch;
        }
    }

    let first = p[rig.tail[0]];
    let (mut x, mut y, mut z) = (first.x, first.y, first.z);
    let s11 = h(11);
    let mut swing = 0.0;
    for (i, &part) in rig.tail.iter().enumerate() {
        let si = h(12 + i);
        swing += (i as f32 * 0.45 + f).sin() * 0.05;
        let n = &mut p[part];
        n.y_rot = (wrap(si.1 - s11.1) * 1.5 + 180.0) * DEG;
        n.x_rot = swing + (si.0 - s11.0) as f32 * DEG * 1.5 * 5.0;
        n.z_rot = wrap(si.1 - mid) * DEG * 1.5;
        (n.x, n.y, n.z) = (x, y, z);
        y += n.x_rot.sin() * 10.0;
        z -= n.y_rot.cos() * n.x_rot.cos() * 10.0;
        x -= n.y_rot.sin() * n.x_rot.cos() * 10.0;
    }
    p
}

fn lerp(a: f64, b: f64, t: f32) -> f64 {
    a + (b - a) * f64::from(t)
}

/// Appends every dragon, `t` of a tick on from its last position: its body to `models`, its
/// glowing eyes, death rays, fireballs and breath to `translucent`.
pub fn append_dragons(models: &mut ChunkMesh, translucent: &mut ChunkMesh, views: &[DragonView], atlas: &Atlas, light: &SkyLight, t: f32) {
    let (Some(rig), Some(skin)) = (rig(), region(atlas, "dragon")) else { return };
    let eyes = region(atlas, "dragon_eyes");
    let fireball = region(atlas, "dragon_fireball");
    let glow = ResourceId::parse(kaiju::pack::BEAM).ok().filter(|id| atlas.contains(id)).map(|id| atlas.entity_region(&id));
    let t = t.clamp(0.0, 1.0);
    for v in views {
        // Dying, it freezes in place (vanilla stops recording its flight and its partial tick).
        let dying = v.death_ticks > 0;
        let pt = if dying { 0.0 } else { t };
        let at: [f64; 3] = std::array::from_fn(|i| lerp(v.previous_position[i], v.position[i], t));
        let flap = v.flap_previous + (v.flap - v.flap_previous) * t;
        let posed = pose(rig, v, flap, pt);
        let yaw = sample(&v.history, 7, pt).1;
        let pitch = (sample(&v.history, 5, pt).0 - sample(&v.history, 10, pt).0) as f32;
        let transform = Mat34::rotation_y(-yaw * DEG)
            .mul(&Mat34::rotation_zyx(0.0, 0.0, pitch * 10.0 * DEG))
            .mul(&Mat34::translation(0.0, 0.0, 1.0))
            .mul(&Mat34::scale(-1.0, -1.0, 1.0))
            .mul(&Mat34::translation(0.0, -1.501, 0.0));
        let mut quads: Vec<Quad> = Vec::new();
        rig.model.mesh(&posed, &transform, &mut quads);
        let probe = (at[0].floor() as i32, (at[1] + 2.0).floor() as i32, at[2].floor() as i32);
        let (sky, block) = (f32::from(light.get(probe)), f32::from(light.get_block(probe)));
        let tint = if v.hurt_ticks > 0 && !dying { [1.0, 0.7, 0.7] } else { [1.0; 3] };
        push_quads(models, &quads, at, skin, tint, 1.0, sky, block, true);
        if let Some(eyes) = eyes {
            push_quads(translucent, &quads, at, eyes, [1.0; 3], 1.0, 15.0, 15.0, false);
        }
        if let Some(glow) = glow.filter(|_| dying) {
            let death = (v.death_ticks as f32 + t) / 200.0;
            let base = Mat4::from_cols_array_2d(&mat4_rows(&transform)).transpose() * Mat4::from_translation(Vec3::new(0.0, -1.0, -2.0));
            push_rays(translucent, glow, at, base, death.min(1.0));
        }
        if let Some(fireball) = fireball {
            for &(_, ball) in &v.fireballs {
                push_billboard(translucent, fireball, ball, 1.0);
            }
        }
        if let Some(glow) = glow {
            for &(centre, radius) in &v.clouds {
                push_cloud(translucent, glow, centre, radius);
            }
        }
    }
}

/// A 3x4 transform as the rows of a 4x4 one.
fn mat4_rows(m: &Mat34) -> [[f32; 4]; 4] {
    [m.0[0], m.0[1], m.0[2], [0.0, 0.0, 0.0, 1.0]]
}

/// `EnderDragonRenderer.submitRays`: white rays bursting out of it as it dies, more and longer
/// as death goes on, fading at the end. Vanilla re-seeds them with 432 every frame, so they
/// keep their places, each turned on from the last.
fn push_rays(mesh: &mut ChunkMesh, region: [f32; 4], at: [f64; 3], base: Mat4, t: f32) {
    let fade = if t > 0.8 { ((t - 0.8) / 0.2).min(1.0) } else { 0.0 };
    let mut random = JavaRandom::new(432);
    let mut pose = base;
    let origin = Vec3::new(at[0] as f32, at[1] as f32, at[2] as f32);
    let half_sqrt3 = 3f32.sqrt() / 2.0;
    let count = ((t + t * t) / 2.0 * 60.0).floor() as i32;
    for _ in 0..count {
        let mut r = || random.next_float() * TAU;
        let q = Quat::from_rotation_x(r()) * Quat::from_rotation_y(r()) * Quat::from_rotation_z(r());
        let q = q * Quat::from_rotation_x(r()) * Quat::from_rotation_y(r()) * Quat::from_rotation_z(r() + t * std::f32::consts::FRAC_PI_2);
        pose *= Mat4::from_quat(q);
        let length = random.next_float() * 20.0 + 5.0 + fade * 10.0;
        let width = random.next_float() * 2.0 + 1.0 + fade * 2.0;
        let corners = [
            Vec3::new(-half_sqrt3 * width, length, -0.5 * width),
            Vec3::new(half_sqrt3 * width, length, -0.5 * width),
            Vec3::new(0.0, length, width),
        ];
        let centre = origin + pose.transform_point3(Vec3::ZERO);
        let ends = corners.map(|c| origin + pose.transform_point3(c));
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            push_triangle(mesh, region, [centre, ends[a], ends[b]], [[1.0, 1.0, 1.0, 1.0 - fade], [1.0, 0.0, 1.0, 0.0], [1.0, 0.0, 1.0, 0.0]]);
        }
    }
}

/// A full-bright triangle, both sides, in the texture's brightest texel.
fn push_triangle(mesh: &mut ChunkMesh, region: [f32; 4], corners: [Vec3; 3], colors: [[f32; 4]; 3]) {
    let start = mesh.vertices.len() as u32;
    let uv = [(region[0] + region[2]) / 2.0, (region[1] + region[3]) / 2.0];
    for (corner, color) in corners.iter().zip(colors) {
        mesh.vertices.push(Vertex { position: corner.to_array(), uv, color, sky_light: 15.0, block_light: 15.0 });
    }
    mesh.indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 1]);
    mesh.faces += 2;
}

/// A fireball (`DragonFireballRenderer`): its texture on crossed squares `size` blocks across.
fn push_billboard(mesh: &mut ChunkMesh, region: [f32; 4], at: [f64; 3], size: f32) {
    let c = Vec3::new(at[0] as f32, at[1] as f32, at[2] as f32);
    let h = size / 2.0;
    let uv = [[region[0], region[3]], [region[2], region[3]], [region[2], region[1]], [region[0], region[1]]];
    for (right, up) in [(Vec3::X, Vec3::Y), (Vec3::Z, Vec3::Y), (Vec3::X, Vec3::Z)] {
        let corners = [c - right * h - up * h, c + right * h - up * h, c + right * h + up * h, c - right * h + up * h];
        let start = mesh.vertices.len() as u32;
        for (corner, uv) in corners.iter().zip(uv) {
            mesh.vertices.push(Vertex { position: corner.to_array(), uv, color: [1.0; 4], sky_light: 15.0, block_light: 15.0 });
        }
        mesh.indices.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
        mesh.indices.extend_from_slice(&[start, start + 2, start + 1, start, start + 3, start + 2]);
        mesh.faces += 2;
    }
}

/// A cloud of dragon's breath (`AreaEffectCloud`): a purple haze over the ground it covers.
fn push_cloud(mesh: &mut ChunkMesh, region: [f32; 4], centre: [f64; 3], radius: f32) {
    let purple = [0.75, 0.2, 0.9];
    let rings = (radius / 1.2).ceil().max(1.0) as i32;
    for ring in 0..=rings {
        let r = f64::from(radius) * f64::from(ring) / f64::from(rings);
        let points = (r * 2.5).ceil().max(1.0) as i32;
        for i in 0..points {
            let a = std::f64::consts::TAU * f64::from(i) / f64::from(points);
            push_ground_mark(mesh, region, [centre[0] + a.cos() * r, centre[1] + 0.1, centre[2] + a.sin() * r], 1.6, purple);
        }
    }
}

/// `java.util.Random`, for the death rays' fixed pattern.
struct JavaRandom(u64);

impl JavaRandom {
    fn new(seed: u64) -> Self {
        Self((seed ^ 0x5_DEEC_E66D) & ((1 << 48) - 1))
    }

    fn next(&mut self, bits: u32) -> u32 {
        self.0 = (self.0.wrapping_mul(0x5_DEEC_E66D).wrapping_add(11)) & ((1 << 48) - 1);
        (self.0 >> (48 - bits)) as u32
    }

    fn next_float(&mut self) -> f32 {
        self.next(24) as f32 * 5.960_464_5e-8
    }
}
