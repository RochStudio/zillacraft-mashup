//! Box models in Minecraft's entity-model form: a tree of parts, each a pivot offset and a
//! Z-Y-X rotation, holding cubes laid out on the texture the way Minecraft lays them out.
//! Units are model pixels (1/16 block) with Y pointing down, as authored.

use serde::Deserialize;

#[derive(Deserialize)]
struct RawCube {
    uv: [f32; 2],
    from: [f32; 3],
    size: [f32; 3],
    #[serde(default)]
    grow: f32,
    #[serde(default)]
    mirror: bool,
}

#[derive(Deserialize)]
struct RawPart {
    name: String,
    offset: [f32; 3],
    rotation: [f32; 3],
    cubes: Vec<RawCube>,
    children: Vec<RawPart>,
}

#[derive(Deserialize)]
struct RawModel {
    texture: [u32; 2],
    #[serde(default)]
    constants: serde_json::Map<String, serde_json::Value>,
    root: RawPart,
}

#[derive(Clone, Copy, Debug)]
pub struct Cube {
    pub uv: [f32; 2],
    pub from: [f32; 3],
    pub size: [f32; 3],
    pub grow: f32,
    pub mirror: bool,
}

/// Where a part sits relative to its parent: a pivot offset and a rotation (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PartPose {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub x_rot: f32,
    pub y_rot: f32,
    pub z_rot: f32,
    pub visible: bool,
}

#[derive(Debug)]
pub struct Part {
    pub name: String,
    /// "root/hips/body/neck": names from the top, as the mod refers to parts.
    pub path: String,
    pub parent: Option<usize>,
    pub rest: PartPose,
    pub cubes: Vec<Cube>,
}

#[derive(Debug)]
pub struct Model {
    pub texture_size: [u32; 2],
    /// Parents always come before their children.
    pub parts: Vec<Part>,
    pub constants: serde_json::Map<String, serde_json::Value>,
}

/// One face of a cube, ready to draw: four corners and their texture coordinates (0..1).
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub positions: [[f32; 3]; 4],
    pub uvs: [[f32; 2]; 4],
}

impl Model {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let raw: RawModel = serde_json::from_str(json)?;
        let mut parts = Vec::new();
        flatten(&raw.root, None, "", &mut parts);
        Ok(Self { texture_size: raw.texture, parts, constants: raw.constants })
    }

    pub fn find(&self, path: &str) -> Option<usize> {
        self.parts.iter().position(|p| p.path == path)
    }

    pub fn rest_pose(&self) -> Vec<PartPose> {
        self.parts.iter().map(|p| p.rest).collect()
    }

    /// A table of numbers the model class exported (e.g. Godzilla's plate directions).
    pub fn table(&self, name: &str) -> Vec<Vec<f32>> {
        self.constants
            .get(name)
            .and_then(|v| v.as_array())
            .map(|rows| {
                rows.iter()
                    .map(|row| row.as_array().map(|r| r.iter().filter_map(|v| v.as_f64()).map(|v| v as f32).collect()).unwrap_or_else(Vec::new))
                    .collect()
            })
            .unwrap_or_else(Vec::new)
    }

    pub fn strings(&self, name: &str) -> Vec<String> {
        self.constants
            .get(name)
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
            .unwrap_or_else(Vec::new)
    }

    pub fn number(&self, name: &str) -> Option<f32> {
        self.constants.get(name).and_then(|v| v.as_f64()).map(|v| v as f32)
    }

    /// Every visible face of the posed model, in `to_world` space. `to_world` receives points in
    /// blocks, still in model orientation (Y down); see [`entity_transform`].
    pub fn mesh(&self, pose: &[PartPose], to_world: &Mat34, out: &mut Vec<Quad>) {
        let [tw, th] = [self.texture_size[0] as f32, self.texture_size[1] as f32];
        let mut world = Vec::with_capacity(self.parts.len());
        let mut shown = Vec::with_capacity(self.parts.len());
        for (i, part) in self.parts.iter().enumerate() {
            let p = pose[i];
            let local = Mat34::translation(p.x / 16.0, p.y / 16.0, p.z / 16.0).mul(&Mat34::rotation_zyx(p.z_rot, p.y_rot, p.x_rot));
            let (parent, parent_shown) = match part.parent {
                Some(j) => (world[j], shown[j]),
                None => (*to_world, true),
            };
            world.push(parent.mul(&local));
            shown.push(parent_shown && p.visible);
            if !shown[i] {
                continue;
            }
            for cube in &part.cubes {
                cube_quads(cube, tw, th, &world[i], out);
            }
        }
    }
}

fn flatten(raw: &RawPart, parent: Option<usize>, parent_path: &str, out: &mut Vec<Part>) {
    let path = match (parent, parent_path.is_empty()) {
        (None, _) => String::new(),
        (Some(_), true) => raw.name.clone(),
        (Some(_), false) => format!("{parent_path}/{}", raw.name),
    };
    let index = out.len();
    out.push(Part {
        name: raw.name.clone(),
        path: path.clone(),
        parent,
        rest: PartPose {
            x: raw.offset[0],
            y: raw.offset[1],
            z: raw.offset[2],
            x_rot: raw.rotation[0],
            y_rot: raw.rotation[1],
            z_rot: raw.rotation[2],
            visible: true,
        },
        cubes: raw
            .cubes
            .iter()
            .map(|c| Cube { uv: c.uv, from: c.from, size: c.size, grow: c.grow, mirror: c.mirror })
            .collect(),
    });
    for child in &raw.children {
        flatten(child, Some(index), &path, out);
    }
}

/// The six faces of a cube, with corners and texture coordinates exactly as Minecraft's
/// `ModelPart.Cube` builds them (including mirrored cubes).
fn cube_quads(c: &Cube, tw: f32, th: f32, m: &Mat34, out: &mut Vec<Quad>) {
    let g = c.grow;
    let (mut x0, y0, z0) = (c.from[0] - g, c.from[1] - g, c.from[2] - g);
    let (mut x1, y1, z1) = (c.from[0] + c.size[0] + g, c.from[1] + c.size[1] + g, c.from[2] + c.size[2] + g);
    if c.mirror {
        std::mem::swap(&mut x0, &mut x1);
    }
    let v = [
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y1, z0],
        [x0, y1, z0],
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
    ];
    let [u, vv] = c.uv;
    let [dx, dy, dz] = c.size;
    let (j, k, l, mm, n, o) = (u, u + dz, u + dz + dx, u + dz + dx + dx, u + dz + dx + dz, u + dz + dx + dz + dx);
    let (p, q, r) = (vv, vv + dz, vv + dz + dy);
    // (corners as indices into v, then u1, v1, u2, v2), in Minecraft's order: down, up, west, north, east, south.
    let faces: [([usize; 4], f32, f32, f32, f32); 6] = [
        ([5, 4, 0, 1], k, p, l, q),
        ([2, 3, 7, 6], l, q, mm, p),
        ([0, 4, 7, 3], j, q, k, r),
        ([1, 0, 3, 2], k, q, l, r),
        ([6, 5, 1, 2], l, q, n, r),
        ([7, 6, 5, 4], n, q, o, r),
    ];
    for (ids, u1, v1, u2, v2) in faces {
        let mut positions = ids.map(|i| m.apply([v[i][0] / 16.0, v[i][1] / 16.0, v[i][2] / 16.0]));
        let mut uvs = [[u2 / tw, v1 / th], [u1 / tw, v1 / th], [u1 / tw, v2 / th], [u2 / tw, v2 / th]];
        if c.mirror {
            positions.reverse();
            uvs.reverse();
        }
        out.push(Quad { positions, uvs });
    }
}

/// A 3x4 affine transform (rotation/scale + translation), row-major.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat34(pub [[f32; 4]; 3]);

impl Mat34 {
    pub const IDENTITY: Self = Self([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);

    pub fn translation(x: f32, y: f32, z: f32) -> Self {
        Self([[1.0, 0.0, 0.0, x], [0.0, 1.0, 0.0, y], [0.0, 0.0, 1.0, z]])
    }

    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        Self([[x, 0.0, 0.0, 0.0], [0.0, y, 0.0, 0.0], [0.0, 0.0, z, 0.0]])
    }

    /// Rz(z) * Ry(y) * Rx(x): how a model part applies its rotations.
    pub fn rotation_zyx(z: f32, y: f32, x: f32) -> Self {
        let (sx, cx) = x.sin_cos();
        let (sy, cy) = y.sin_cos();
        let (sz, cz) = z.sin_cos();
        Self([
            [cz * cy, cz * sy * sx - sz * cx, cz * sy * cx + sz * sx, 0.0],
            [sz * cy, sz * sy * sx + cz * cx, sz * sy * cx - cz * sx, 0.0],
            [-sy, cy * sx, cy * cx, 0.0],
        ])
    }

    pub fn rotation_y(angle: f32) -> Self {
        Self::rotation_zyx(0.0, angle, 0.0)
    }

    pub fn mul(&self, o: &Self) -> Self {
        let a = &self.0;
        let b = &o.0;
        let mut r = [[0.0; 4]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate().take(3) {
                *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
            }
            row[3] = a[i][0] * b[0][3] + a[i][1] * b[1][3] + a[i][2] * b[2][3] + a[i][3];
        }
        Self(r)
    }

    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.0;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
        ]
    }
}

/// How a living entity's model is placed in the world, as Minecraft does it: scaled, turned to
/// face its body yaw (degrees), flipped upright, and lifted 1.501 blocks. Result is relative to
/// the entity's feet, Y up.
pub fn entity_transform(scale: f32, body_yaw_degrees: f32) -> Mat34 {
    let turn = Mat34::rotation_y((180.0 - body_yaw_degrees).to_radians());
    Mat34::scale(scale, scale, scale)
        .mul(&turn)
        .mul(&Mat34::scale(-1.0, -1.0, 1.0))
        .mul(&Mat34::translation(0.0, -1.501, 0.0))
}
