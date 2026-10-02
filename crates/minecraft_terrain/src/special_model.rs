//! The models vanilla's special item renderers draw, as their `LayerDefinition`s build them
//! (`ModelPart.Cube`'s texture layout, `PartPose` offsets, rotations and scale), baked into
//! model elements for the GUI icon raster: shulker boxes, mob heads, the conduit's shell, the
//! decorated pot, the shield and the copper golem statue.

use crate::model::{Element, ElementRotation, Face};
use crate::pack::ResourceId;
use anyhow::Result;
use glam::{Mat3, Mat4, Quat, Vec3};
use serde_json::Value;
use std::f32::consts::{FRAC_PI_2, PI};

/// One `addBox`: its texture offset, corner and size in model pixels, `CubeDeformation`
/// growth, whether it is mirrored, and the one face it draws when it draws only one.
#[derive(Clone, Copy)]
struct Cube {
    tex: [f32; 2],
    origin: [f32; 3],
    size: [f32; 3],
    grow: f32,
    mirror: bool,
    only: Option<&'static str>,
}

const fn cube(tex: [f32; 2], origin: [f32; 3], size: [f32; 3]) -> Cube {
    Cube { tex, origin, size, grow: 0.0, mirror: false, only: None }
}

impl Cube {
    const fn grown(self, grow: f32) -> Self {
        Self { grow, ..self }
    }

    const fn mirrored(self) -> Self {
        Self { mirror: true, ..self }
    }

    const fn only(self, face: &'static str) -> Self {
        Self { only: Some(face), ..self }
    }
}

/// A `PartDefinition`: its `PartPose` (offset in model pixels, rotation in radians as
/// `rotationZYX` turns it, scale), cubes and children.
struct Part {
    offset: [f32; 3],
    rotation: [f32; 3],
    scale: f32,
    cubes: Vec<Cube>,
    children: Vec<Part>,
}

fn part(offset: [f32; 3], cubes: Vec<Cube>) -> Part {
    Part { offset, rotation: [0.0; 3], scale: 1.0, cubes, children: Vec::new() }
}

impl Part {
    fn turned(self, rotation: [f32; 3]) -> Self {
        Self { rotation, ..self }
    }

    fn scaled(self, scale: f32) -> Self {
        Self { scale, ..self }
    }

    fn with(self, children: Vec<Part>) -> Self {
        Self { children, ..self }
    }
}

/// A layer: its root, the texture it is drawn with and the size its UVs are laid out on.
struct Layer {
    root: Part,
    texture: ResourceId,
    size: [f32; 2],
}

/// The elements of an item definition's `minecraft:special` model object when it is one built
/// here, placed by `transform` (the item model's transformations, block units).
pub(crate) fn special_elements(model: &Value, transform: Mat4) -> Result<Option<Vec<Element>>> {
    let layer = |root: Part, texture: &str, size: [f32; 2]| -> Result<Layer> {
        Ok(Layer { root, texture: ResourceId::parse(texture)?, size })
    };
    let layers = match model["type"].as_str() {
        Some("minecraft:shulker_box") => {
            let sheet = ResourceId::parse(model["texture"].as_str().unwrap_or("minecraft:shulker"))?;
            vec![layer(shulker_box(), &format!("{}:entity/shulker/{}", sheet.namespace, sheet.path), [64.0, 64.0])?]
        }
        Some("minecraft:head") => {
            // `SkullBlockRenderer`'s model and skin for each kind.
            let (root, texture, size) = match model["kind"].as_str() {
                Some("skeleton") => (head(), "minecraft:entity/skeleton/skeleton", [64.0, 32.0]),
                Some("wither_skeleton") => (head(), "minecraft:entity/skeleton/wither_skeleton", [64.0, 32.0]),
                Some("creeper") => (head(), "minecraft:entity/creeper/creeper", [64.0, 32.0]),
                Some("zombie") => (humanoid_head(), "minecraft:entity/zombie/zombie", [64.0, 64.0]),
                Some("player") => (humanoid_head(), DEFAULT_SKIN, [64.0, 64.0]),
                Some("piglin") => (piglin_head(), "minecraft:entity/piglin/piglin", [64.0, 64.0]),
                Some("dragon") => (dragon_head(), "minecraft:entity/enderdragon/dragon", [256.0, 256.0]),
                _ => return Ok(None),
            };
            let mut layer = layer(root, texture, size)?;
            if let Some(texture) = model["texture"].as_str() {
                layer.texture = sprite(texture)?;
            }
            vec![layer]
        }
        Some("minecraft:player_head") => vec![layer(humanoid_head(), DEFAULT_SKIN, [64.0, 64.0])?],
        Some("minecraft:conduit") => vec![layer(conduit_shell(), "minecraft:entity/conduit/base", [32.0, 16.0])?],
        Some("minecraft:decorated_pot") => vec![
            layer(decorated_pot_base(), "minecraft:entity/decorated_pot/decorated_pot_base", [32.0, 32.0])?,
            layer(decorated_pot_sides(), "minecraft:entity/decorated_pot/decorated_pot_side", [16.0, 16.0])?,
        ],
        Some("minecraft:shield") => vec![layer(shield(), "minecraft:entity/shield/shield_base_nopattern", [64.0, 64.0])?],
        // Every pose is drawn standing: an icon only ever shows the statue's default.
        Some("minecraft:copper_golem_statue") => {
            let texture = model["texture"].as_str().unwrap_or("minecraft:textures/entity/copper_golem/copper_golem.png");
            vec![Layer { root: copper_golem_statue(), texture: sprite(texture)?, size: [64.0, 64.0] }]
        }
        _ => return Ok(None),
    };
    let mut elements = Vec::new();
    for layer in &layers {
        bake_part(&layer.root, transform, layer, &mut elements);
    }
    Ok(Some(elements))
}

/// `DefaultPlayerSkin`'s first skin, what a player head without a profile shows.
const DEFAULT_SKIN: &str = "minecraft:entity/player/wide/steve";

/// A texture named as a file (`minecraft:textures/entity/copper_golem/copper_golem.png`) or as
/// a sprite (`minecraft:entity/copper_golem/copper_golem`).
fn sprite(name: &str) -> Result<ResourceId> {
    let id = ResourceId::parse(name)?;
    let path = id.path.strip_prefix("textures/").unwrap_or(&id.path);
    let path = path.strip_suffix(".png").unwrap_or(path);
    ResourceId::parse(&format!("{}:{path}", id.namespace))
}

/// An item model's `transformation`: a row-major matrix, or `Transformation`'s translation,
/// left rotation, scale and right rotation (each rotation a quaternion `[x, y, z, w]` or an
/// axis and angle in radians). None is the identity.
pub(crate) fn transformation(value: &Value) -> Result<Mat4> {
    let numbers = |value: &Value| -> Option<Vec<f32>> {
        value.as_array()?.iter().map(|v| v.as_f64().map(|v| v as f32)).collect()
    };
    if value.is_null() {
        return Ok(Mat4::IDENTITY);
    }
    if let Some(matrix) = numbers(value).filter(|m| m.len() == 16) {
        let mut columns = [0.0; 16];
        columns.copy_from_slice(&matrix);
        return Ok(Mat4::from_cols_array(&columns).transpose());
    }
    let vector = |key: &str, default: f32| -> Vec3 {
        numbers(&value[key])
            .filter(|v| v.len() == 3)
            .map_or(Vec3::splat(default), |v| Vec3::new(v[0], v[1], v[2]))
    };
    let rotation = |key: &str| -> Quat {
        let raw = &value[key];
        if let Some(q) = numbers(raw).filter(|q| q.len() == 4) {
            return Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize();
        }
        match (numbers(&raw["axis"]).filter(|a| a.len() == 3), raw["angle"].as_f64()) {
            (Some(axis), Some(angle)) => Quat::from_axis_angle(Vec3::new(axis[0], axis[1], axis[2]).normalize(), angle as f32),
            _ => Quat::IDENTITY,
        }
    };
    Ok(Mat4::from_translation(vector("translation", 0.0))
        * Mat4::from_quat(rotation("left_rotation"))
        * Mat4::from_scale(vector("scale", 1.0))
        * Mat4::from_quat(rotation("right_rotation")))
}

/// `ModelPart.translateAndRotate` and on down the children: offset, then `rotationZYX`, then
/// scale, in block units.
fn bake_part(part: &Part, parent: Mat4, layer: &Layer, out: &mut Vec<Element>) {
    let [x, y, z] = part.rotation;
    let placed = parent
        * Mat4::from_translation(Vec3::from(part.offset) / 16.0)
        * Mat4::from_rotation_z(z)
        * Mat4::from_rotation_y(y)
        * Mat4::from_rotation_x(x)
        * Mat4::from_scale(Vec3::splat(part.scale));
    for cube in &part.cubes {
        out.push(bake_cube(cube, placed, layer));
    }
    for child in &part.children {
        bake_part(child, placed, layer, out);
    }
}

/// One cube as an element: the box turned by the axis turn nearest its part's (a signed
/// permutation, so the box stays square to the axes), with what is left of the turn as the
/// element's own rotation about the part's pivot. Each face takes `ModelPart.Cube`'s UVs at
/// the corners the raster draws it by.
fn bake_cube(cube: &Cube, placed: Mat4, layer: &Layer) -> Element {
    let linear = Mat3::from_mat4(placed);
    let pivot = placed.w_axis.truncate();
    let mut axes = Mat3::ZERO;
    let mut taken = [false; 3];
    for column in 0..3 {
        let image = linear.col(column);
        let row = (0..3)
            .filter(|row| !taken[*row])
            .max_by(|a, b| image[*a].abs().total_cmp(&image[*b].abs()))
            .unwrap_or(column);
        taken[row] = true;
        let mut axis = Vec3::ZERO;
        axis[row] = image[row].signum() * image.length();
        *axes.col_mut(column) = axis;
    }
    let residual = linear * axes.inverse();
    let place = |pixels: Vec3| pivot + axes * (pixels / 16.0);
    let lo = Vec3::from(cube.origin) - cube.grow;
    let hi = Vec3::from(cube.origin) + Vec3::from(cube.size) + cube.grow;
    let (a, b) = (place(lo), place(hi));
    let (from, to) = (a.min(b), a.max(b));

    // `ModelPart.Cube`: the texture strip's edges, and each face's corners (0 at the low side
    // of an axis, 1 at the high; a mirrored cube swaps its x) with the UV `Polygon` gives each.
    let [w, h, d] = cube.size;
    let [u, v] = cube.tex;
    let (u0, u1, u2, u3, u4, u5) = (u, u + d, u + d + w, u + d + w + w, u + d + w + d, u + d + w + d + w);
    let (v0, v1, v2) = (v, v + d, v + d + h);
    let polygons: [Polygon; 6] = [
        ("down", -Vec3::Y, [([1, 0, 1], [u2, v0]), ([0, 0, 1], [u1, v0]), ([0, 0, 0], [u1, v1]), ([1, 0, 0], [u2, v1])]),
        ("up", Vec3::Y, [([1, 1, 0], [u3, v1]), ([0, 1, 0], [u2, v1]), ([0, 1, 1], [u2, v0]), ([1, 1, 1], [u3, v0])]),
        ("west", -Vec3::X, [([0, 0, 0], [u1, v1]), ([0, 0, 1], [u0, v1]), ([0, 1, 1], [u0, v2]), ([0, 1, 0], [u1, v2])]),
        ("north", -Vec3::Z, [([1, 0, 0], [u2, v1]), ([0, 0, 0], [u1, v1]), ([0, 1, 0], [u1, v2]), ([1, 1, 0], [u2, v2])]),
        ("east", Vec3::X, [([1, 0, 1], [u4, v1]), ([1, 0, 0], [u2, v1]), ([1, 1, 0], [u2, v2]), ([1, 1, 1], [u4, v2])]),
        ("south", Vec3::Z, [([0, 0, 1], [u5, v1]), ([1, 0, 1], [u4, v1]), ([1, 1, 1], [u4, v2]), ([0, 1, 1], [u5, v2])]),
    ];
    let corner = |[x, y, z]: [usize; 3]| {
        Vec3::new(
            if (x == 1) != cube.mirror { hi.x } else { lo.x },
            if y == 1 { hi.y } else { lo.y },
            if z == 1 { hi.z } else { lo.z },
        )
    };
    let mut faces = Vec::new();
    for (name, normal, corners) in polygons {
        if cube.only.is_some_and(|only| only != name) {
            continue;
        }
        let normal = if cube.mirror { normal * Vec3::new(-1.0, 1.0, 1.0) } else { normal };
        let direction = direction_of(axes * normal);
        // Where the raster puts this face's corners, in its centred pixels.
        let corners = corners.map(|(at, uv)| (place(corner(at)) * 16.0 - 8.0, [uv[0] / layer.size[0], uv[1] / layer.size[1]]));
        let Some((_, targets)) = crate::item_icon::cuboid_faces((from * 16.0).to_array(), (to * 16.0).to_array())
            .into_iter()
            .find(|(face, _)| *face == direction)
        else {
            continue;
        };
        let uv_at = |target: [f32; 3]| {
            let target = Vec3::from(target);
            corners
                .iter()
                .min_by(|a, b| a.0.distance_squared(target).total_cmp(&b.0.distance_squared(target)))
                .map_or([0.0; 2], |(_, uv)| *uv)
        };
        let (first, third) = (uv_at(targets[0]), uv_at(targets[2]));
        faces.push(Face {
            direction: direction.into(),
            texture: layer.texture.clone(),
            uv: [first[0], first[1], third[0], third[1]],
            cull: false,
            cullface: None,
            tint: false,
            tint_index: None,
            force_translucent: false,
        });
    }
    Element {
        from: from.to_array(),
        to: to.to_array(),
        faces,
        rotation_y: 0,
        rotation: (!residual.abs_diff_eq(Mat3::IDENTITY, 1e-4))
            .then(|| ElementRotation { origin: pivot.to_array(), matrix: residual.to_cols_array_2d() }),
        shade_direction_override: None,
    }
}

/// A `ModelPart.Cube` face: its direction, outward normal, and corners (by the side of each
/// axis they are on) with their UVs.
type Polygon = (&'static str, Vec3, [([usize; 3], [f32; 2]); 4]);

/// The face an outward normal along an axis names.
fn direction_of(normal: Vec3) -> &'static str {
    let a = normal.abs();
    if a.x >= a.y && a.x >= a.z {
        if normal.x > 0.0 { "east" } else { "west" }
    } else if a.y >= a.z {
        if normal.y > 0.0 { "up" } else { "down" }
    } else if normal.z > 0.0 {
        "south"
    } else {
        "north"
    }
}

/// `ShulkerModel.createShellMesh`, closed: lid and base.
fn shulker_box() -> Part {
    part([0.0; 3], Vec::new()).with(vec![
        part([0.0, 24.0, 0.0], vec![cube([0.0, 0.0], [-8.0, -16.0, -8.0], [16.0, 12.0, 16.0])]),
        part([0.0, 24.0, 0.0], vec![cube([0.0, 28.0], [-8.0, -8.0, -8.0], [16.0, 8.0, 16.0])]),
    ])
}

/// `SkullModel.createHeadModel` (`createMobHeadLayer`'s).
fn head() -> Part {
    part([0.0; 3], vec![cube([0.0, 0.0], [-4.0, -8.0, -4.0], [8.0, 8.0, 8.0])])
}

/// `SkullModel.createHumanoidHeadLayer`: the head and its hat.
fn humanoid_head() -> Part {
    part(
        [0.0; 3],
        vec![
            cube([0.0, 0.0], [-4.0, -8.0, -4.0], [8.0, 8.0, 8.0]),
            cube([32.0, 0.0], [-4.0, -8.0, -4.0], [8.0, 8.0, 8.0]).grown(0.25),
        ],
    )
}

/// `AbstractPiglinModel.addHead`, its ears as `PiglinHeadModel.setupAnim` hangs them at rest.
fn piglin_head() -> Part {
    part(
        [0.0; 3],
        vec![
            cube([0.0, 0.0], [-5.0, -8.0, -4.0], [10.0, 8.0, 8.0]),
            cube([31.0, 1.0], [-2.0, -4.0, -5.0], [4.0, 4.0, 1.0]),
            cube([2.0, 4.0], [2.0, -2.0, -5.0], [1.0, 2.0, 1.0]),
            cube([2.0, 0.0], [-3.0, -2.0, -5.0], [1.0, 2.0, 1.0]),
        ],
    )
    .with(vec![
        part([4.5, -6.0, 0.0], vec![cube([51.0, 6.0], [0.0, 0.0, -2.0], [1.0, 5.0, 4.0])]).turned([0.0, 0.0, -0.7]),
        part([-4.5, -6.0, 0.0], vec![cube([39.0, 6.0], [-1.0, 0.0, -2.0], [1.0, 5.0, 4.0])]).turned([0.0, 0.0, 0.7]),
    ])
}

/// `DragonHeadModel.createHeadLayer`, its jaw as `setupAnim` opens it at rest.
fn dragon_head() -> Part {
    part(
        [0.0, -7.986666, 0.0],
        vec![
            cube([176.0, 44.0], [-6.0, -1.0, -24.0], [12.0, 5.0, 16.0]),
            cube([112.0, 30.0], [-8.0, -8.0, -10.0], [16.0, 16.0, 16.0]),
            cube([0.0, 0.0], [-5.0, -12.0, -4.0], [2.0, 4.0, 6.0]).mirrored(),
            cube([112.0, 0.0], [-5.0, -3.0, -22.0], [2.0, 2.0, 4.0]).mirrored(),
            cube([0.0, 0.0], [3.0, -12.0, -4.0], [2.0, 4.0, 6.0]),
            cube([112.0, 0.0], [3.0, -3.0, -22.0], [2.0, 2.0, 4.0]),
        ],
    )
    .scaled(0.75)
    .with(vec![
        part([0.0, 4.0, -8.0], vec![cube([176.0, 65.0], [-6.0, 0.0, -16.0], [12.0, 4.0, 16.0])]).turned([0.2, 0.0, 0.0]),
    ])
}

/// `ConduitRenderer.createShellLayer`.
fn conduit_shell() -> Part {
    part([0.0; 3], vec![cube([0.0, 0.0], [-3.0, -3.0, -3.0], [6.0, 6.0, 6.0])])
}

/// `DecoratedPotRenderer.createBaseLayer`: the neck, and the top and bottom.
fn decorated_pot_base() -> Part {
    let plane = || vec![cube([-14.0, 13.0], [0.0, 0.0, 0.0], [14.0, 0.0, 14.0])];
    part([0.0; 3], Vec::new()).with(vec![
        part(
            [0.0, 37.0, 16.0],
            vec![
                cube([0.0, 0.0], [4.0, 17.0, 4.0], [8.0, 3.0, 8.0]).grown(-0.1),
                cube([0.0, 5.0], [5.0, 20.0, 5.0], [6.0, 1.0, 6.0]).grown(0.2),
            ],
        )
        .turned([PI, 0.0, 0.0]),
        part([1.0, 16.0, 1.0], plane()),
        part([1.0, 0.0, 1.0], plane()),
    ])
}

/// `DecoratedPotRenderer.createSidesLayer`: back, left, right and front, one face each.
fn decorated_pot_sides() -> Part {
    let side = || vec![cube([1.0, 0.0], [0.0, 0.0, 0.0], [14.0, 16.0, 0.0]).only("north")];
    part([0.0; 3], Vec::new()).with(vec![
        part([15.0, 16.0, 1.0], side()).turned([0.0, 0.0, PI]),
        part([1.0, 16.0, 1.0], side()).turned([0.0, -FRAC_PI_2, PI]),
        part([15.0, 16.0, 15.0], side()).turned([0.0, FRAC_PI_2, PI]),
        part([1.0, 16.0, 15.0], side()).turned([PI, 0.0, 0.0]),
    ])
}

/// `ShieldModel.createLayer`: plate and handle.
fn shield() -> Part {
    part(
        [0.0; 3],
        vec![
            cube([0.0, 0.0], [-6.0, -11.0, -2.0], [12.0, 22.0, 1.0]),
            cube([26.0, 0.0], [-1.0, -3.0, -1.0], [2.0, 6.0, 6.0]),
        ],
    )
}

/// `CopperGolemModel.createBodyLayer` as `CopperGolemStatueModel.setupAnim` sets it: its root
/// back at the origin and turned over.
fn copper_golem_statue() -> Part {
    let head = part(
        [0.0, -6.0, 0.0],
        vec![
            cube([0.0, 0.0], [-4.0, -5.0, -5.0], [8.0, 5.0, 10.0]).grown(0.015),
            cube([56.0, 0.0], [-1.0, -2.0, -6.0], [2.0, 3.0, 2.0]),
            cube([37.0, 8.0], [-1.0, -9.0, -1.0], [2.0, 4.0, 2.0]).grown(-0.015),
            cube([37.0, 0.0], [-2.0, -13.0, -2.0], [4.0, 4.0, 4.0]).grown(-0.015),
        ],
    );
    let body = part([0.0, -5.0, 0.0], vec![cube([0.0, 15.0], [-4.0, -6.0, -3.0], [8.0, 6.0, 6.0])]).with(vec![
        head,
        part([-4.0, -6.0, 0.0], vec![cube([36.0, 16.0], [-3.0, -1.0, -2.0], [3.0, 10.0, 4.0])]),
        part([4.0, -6.0, 0.0], vec![cube([50.0, 16.0], [0.0, -1.0, -2.0], [3.0, 10.0, 4.0])]),
    ]);
    part([0.0; 3], Vec::new()).turned([0.0, 0.0, PI]).with(vec![
        body,
        part([0.0, -5.0, 0.0], vec![cube([0.0, 27.0], [-4.0, 0.0, -2.0], [4.0, 5.0, 4.0])]),
        part([0.0, -5.0, 0.0], vec![cube([16.0, 27.0], [0.0, 0.0, -2.0], [4.0, 5.0, 4.0])]),
    ])
}
