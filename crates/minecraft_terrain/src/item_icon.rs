//! Small software raster for axis-aligned GUI block item models. Geometry, face textures,
//! and GUI rotation/scale are read from the selected resource pack.
use crate::pack::{PackStack, ResourceId};
use anyhow::Result;
use glam::{Mat3, Mat4, Vec3};
use image::RgbaImage;
use serde_json::Value;
use std::collections::HashMap;

// GuiRenderer prepares each item atlas slot at 16 physical pixels per GUI
// scale unit. Rasterizing at that size preserves the source texel mapping.
pub fn block_icon(
    packs: &PackStack,
    model: &str,
    tints: &[[u8; 3]],
    icon_size: usize,
) -> Result<Option<RgbaImage>> {
    let mut textures = HashMap::<String, String>::new();
    let mut elements = None;
    let mut gui = None;
    let mut current = Some(ResourceId::parse(model)?);
    for _ in 0..12 {
        let Some(id) = current.take() else { break };
        let Some(value) = packs.model(&id)? else {
            break;
        };
        if let Some(entries) = value.get("textures").and_then(Value::as_object) {
            for (key, texture) in entries {
                if let Some(texture) = texture
                    .as_str()
                    .or_else(|| texture.get("sprite").and_then(Value::as_str))
                {
                    textures
                        .entry(key.clone())
                        .or_insert_with(|| texture.into());
                }
            }
        }
        if elements.is_none() {
            elements = value.get("elements").cloned();
        }
        if gui.is_none() {
            gui = value
                .get("display")
                .and_then(|display| display.get("gui"))
                .cloned();
        }
        current = value
            .get("parent")
            .and_then(Value::as_str)
            .map(ResourceId::parse)
            .transpose()?;
    }
    let Some(elements) = elements.and_then(|value| value.as_array().cloned()) else {
        return Ok(None);
    };
    if elements.is_empty()
        || elements
            .iter()
            .any(|element| element.get("rotation").is_some())
    {
        return Ok(None);
    }
    let rotation = gui.as_ref().and_then(|value| value.get("rotation"));
    let x_angle = angle(rotation, 0).unwrap_or(30.0).to_radians();
    let y_angle = angle(rotation, 1).unwrap_or(225.0).to_radians();
    let scale = gui
        .as_ref()
        .and_then(|value| value.get("scale"))
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .and_then(Value::as_f64)
        .unwrap_or(0.625) as f32;
    let mut output = RgbaImage::new(icon_size as u32, icon_size as u32);
    let mut depth = vec![vec![f32::NEG_INFINITY; icon_size]; icon_size];
    for element in &elements {
        let Some((from, to)) = element_bounds(element) else {
            return Ok(None);
        };
        let Some(faces) = element.get("faces").and_then(Value::as_object) else {
            continue;
        };
        for (name, corners) in cuboid_faces(from, to) {
            // The pinned item render pipeline keeps the default back-face
            // culling state. This also hides the rear edges of glass cubes.
            if !face_faces_camera(name, x_angle, y_angle) {
                continue;
            }
            let Some(face) = faces.get(name) else {
                continue;
            };
            let Some(texture) = face
                .get("texture")
                .and_then(Value::as_str)
                .and_then(|name| texture_id(name, &textures))
            else {
                continue;
            };
            let Some(bytes) = packs.texture(&texture)? else {
                continue;
            };
            let source = image::load_from_memory(&bytes)?.to_rgba8();
            let side = source.width().min(source.height());
            if side == 0 {
                continue;
            }
            let Some(bounds) = face_uv(face, name, from, to) else {
                continue;
            };
            let tint = face
                .get("tintindex")
                .and_then(Value::as_u64)
                .and_then(|index| tints.get(index as usize))
                .copied()
                .unwrap_or([255; 3]);
            let vertices =
                corners.map(|point| transform(point, x_angle, y_angle, scale, icon_size));
            let shade = item_diffuse_light(name, x_angle, y_angle);
            for triangle in [[0, 1, 2], [0, 2, 3]] {
                raster_triangle(
                    &mut output,
                    &mut depth,
                    &source,
                    [side, side],
                    icon_size,
                    shade,
                    tint,
                    [
                        vertices[triangle[0]],
                        vertices[triangle[1]],
                        vertices[triangle[2]],
                    ],
                    [
                        uv(triangle[0], bounds),
                        uv(triangle[1], bounds),
                        uv(triangle[2], bounds),
                    ],
                );
            }
        }
    }
    Ok((output.pixels().any(|pixel| pixel[3] > 0)).then_some(output))
}

/// An item model's `display.gui` pose, and whether it is lit from the front (`gui_light:
/// front`: flat, unshaded).
pub(crate) struct GuiPose {
    /// `ItemTransform`'s rotation, `rotationXYZ` of its angles.
    rotation: Mat3,
    scale: f32,
    /// In GUI pixels, x right and y up.
    translation: [f32; 2],
    flat: bool,
}

impl GuiPose {
    /// The pose `model` (or the nearest model it inherits from) gives the GUI.
    pub(crate) fn of(packs: &PackStack, model: &str) -> Result<Self> {
        let mut gui = None;
        let mut light = None;
        let mut current = Some(ResourceId::parse(model)?);
        for _ in 0..12 {
            let Some(id) = current.take() else { break };
            let Some(value) = packs.model(&id)? else { break };
            if gui.is_none() {
                gui = value.get("display").and_then(|display| display.get("gui")).cloned();
            }
            if light.is_none() {
                light = value.get("gui_light").and_then(Value::as_str).map(str::to_owned);
            }
            current = value.get("parent").and_then(Value::as_str).map(ResourceId::parse).transpose()?;
        }
        let rotation = gui.as_ref().and_then(|value| value.get("rotation"));
        let number = |key: &str, axis: usize| {
            gui.as_ref()
                .and_then(|value| value.get(key))
                .and_then(Value::as_array)
                .and_then(|values| values.get(axis))
                .and_then(Value::as_f64)
                .map(|v| v as f32)
        };
        Ok(Self {
            rotation: Mat3::from_rotation_x(angle(rotation, 0).unwrap_or(30.0).to_radians())
                * Mat3::from_rotation_y(angle(rotation, 1).unwrap_or(225.0).to_radians())
                * Mat3::from_rotation_z(angle(rotation, 2).unwrap_or(0.0).to_radians()),
            scale: number("scale", 0).unwrap_or(0.625),
            translation: [number("translation", 0).unwrap_or(0.0), number("translation", 1).unwrap_or(0.0)],
            flat: light.as_deref() == Some("front"),
        })
    }

    fn place(&self, point: Point3, icon_size: usize) -> Point3 {
        let turned = self.rotation * Vec3::from(point);
        let pixels_per_gui_unit = icon_size as f32 / 16.0;
        [
            (8.0 + self.translation[0] + turned.x * self.scale) * pixels_per_gui_unit,
            (8.0 - (self.translation[1] + turned.y * self.scale)) * pixels_per_gui_unit,
            turned.z,
        ]
    }
}

/// An icon from resolved model boxes (block units, each face's texture and UVs as the
/// world's meshes take them, each box turned by its own rotation), posed for the GUI: the
/// item models a special renderer draws in vanilla (chests, banners, shulker boxes, heads),
/// built as the boxes those renderers draw. UVs are fractions of each texture's width and
/// height.
pub(crate) fn elements_icon(
    packs: &PackStack,
    elements: &[crate::model::Element],
    tints: &[[u8; 3]],
    pose: &GuiPose,
    icon_size: usize,
) -> Result<Option<RgbaImage>> {
    let mut output = RgbaImage::new(icon_size as u32, icon_size as u32);
    let mut depth = vec![vec![f32::NEG_INFINITY; icon_size]; icon_size];
    let mut sheets = HashMap::<ResourceId, Option<RgbaImage>>::new();
    for element in elements {
        let (from, to) = (element.from.map(|v| v * 16.0), element.to.map(|v| v * 16.0));
        let turn = element.rotation.map(|rotation| (rotation, Mat3::from_cols_array_2d(&rotation.matrix)));
        for (name, corners) in cuboid_faces(from, to) {
            let normal = face_normal(name);
            let (corners, normal) = match turn {
                // About its origin in block units, between the pixel corners.
                Some((rotation, matrix)) => (
                    corners.map(|corner| rotation.apply(corner.map(|v| (v + 8.0) / 16.0)).map(|v| v * 16.0 - 8.0)),
                    matrix * normal,
                ),
                None => (corners, normal),
            };
            let turned = pose.rotation * normal;
            if turned.z <= 0.0 {
                continue;
            }
            let Some(face) = element.faces.iter().find(|face| face.direction == name) else {
                continue;
            };
            if !sheets.contains_key(&face.texture) {
                let sheet = match packs.texture(&face.texture)? {
                    Some(bytes) => Some(image::load_from_memory(&bytes)?.to_rgba8()),
                    None => None,
                };
                sheets.insert(face.texture.clone(), sheet);
            }
            let Some(source) = sheets.get(&face.texture).and_then(Option::as_ref) else {
                continue;
            };
            if source.width() == 0 || source.height() == 0 {
                continue;
            }
            let tint = face.tint_index.and_then(|index| tints.get(index)).copied().unwrap_or([255; 3]);
            let vertices = corners.map(|point| pose.place(point, icon_size));
            let shade = if pose.flat { 1.0 } else { gui_diffuse_light(turned) };
            for triangle in [[0, 1, 2], [0, 2, 3]] {
                raster_triangle(
                    &mut output,
                    &mut depth,
                    source,
                    [source.width(), source.height()],
                    icon_size,
                    shade,
                    tint,
                    triangle.map(|corner| vertices[corner]),
                    triangle.map(|corner| uv(corner, face.uv)),
                );
            }
        }
    }
    Ok(output.pixels().any(|pixel| pixel[3] > 0).then_some(output))
}

fn face_normal(face: &str) -> Vec3 {
    match face {
        "north" => -Vec3::Z,
        "south" => Vec3::Z,
        "east" => Vec3::X,
        "west" => -Vec3::X,
        "up" => Vec3::Y,
        _ => -Vec3::Y,
    }
}

fn item_diffuse_light(face: &str, x_angle: f32, y_angle: f32) -> f32 {
    gui_diffuse_light(Mat3::from_rotation_x(x_angle) * Mat3::from_rotation_y(y_angle) * face_normal(face))
}

/// The GUI's item light on a face whose normal the display pose has turned to `normal`.
fn gui_diffuse_light(normal: Vec3) -> f32 {
    // Lighting.Entry.ITEMS_3D and minecraft_mix_light in the pinned 26.3
    // Lighting.java / assets/minecraft/shaders/include/light.glsl.
    let light_pose = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0))
        * Mat4::from_rotation_y(1.0821041)
        * Mat4::from_rotation_x(3.2375858)
        * Mat4::from_rotation_y(-std::f32::consts::PI / 8.0)
        * Mat4::from_rotation_x(std::f32::consts::PI * 3.0 / 4.0);
    // GuiItemAtlas scales the item pose by (size, -size, size) before the
    // model transform. That Y inversion also transforms the vertex normal.
    let normal = (normal * Vec3::new(1.0, -1.0, 1.0)).normalize();
    let light0 = light_pose
        .transform_vector3(Vec3::new(0.2, 1.0, -0.7).normalize())
        .normalize();
    let light1 = light_pose
        .transform_vector3(Vec3::new(-0.2, 1.0, 0.7).normalize())
        .normalize();
    ((normal.dot(light0).max(0.0) + normal.dot(light1).max(0.0)) * 0.6 + 0.4).min(1.0)
}

fn face_faces_camera(face: &str, x_angle: f32, y_angle: f32) -> bool {
    (Mat3::from_rotation_x(x_angle) * Mat3::from_rotation_y(y_angle) * face_normal(face)).z > 0.0
}

fn angle(rotation: Option<&Value>, axis: usize) -> Option<f32> {
    rotation?.as_array()?.get(axis)?.as_f64().map(|v| v as f32)
}

type Point3 = [f32; 3];

fn element_bounds(element: &Value) -> Option<(Point3, Point3)> {
    let coords = |key: &str| -> Option<Point3> {
        let values = element.get(key)?.as_array()?;
        Some([
            values.first()?.as_f64()? as f32,
            values.get(1)?.as_f64()? as f32,
            values.get(2)?.as_f64()? as f32,
        ])
    };
    Some((coords("from")?, coords("to")?))
}

fn face_uv(face: &Value, direction: &str, from: Point3, to: Point3) -> Option<[f32; 4]> {
    if let Some(uv) = face.get("uv").and_then(Value::as_array) {
        return Some([
            uv.first()?.as_f64()? as f32 / 16.0,
            uv.get(1)?.as_f64()? as f32 / 16.0,
            uv.get(2)?.as_f64()? as f32 / 16.0,
            uv.get(3)?.as_f64()? as f32 / 16.0,
        ]);
    }
    let [x, y, z] = from;
    let [xx, yy, zz] = to;
    Some(
        match direction {
            "down" => [x, 16.0 - zz, xx, 16.0 - z],
            "up" => [x, z, xx, zz],
            "north" => [16.0 - xx, 16.0 - yy, 16.0 - x, 16.0 - y],
            "south" => [x, 16.0 - yy, xx, 16.0 - y],
            "west" => [z, 16.0 - yy, zz, 16.0 - y],
            "east" => [16.0 - zz, 16.0 - yy, 16.0 - z, 16.0 - y],
            _ => return None,
        }
        .map(|value| value / 16.0),
    )
}

fn texture_id(raw: &str, textures: &HashMap<String, String>) -> Option<ResourceId> {
    let mut current = raw;
    for _ in 0..8 {
        if let Some(name) = current.strip_prefix('#') {
            current = textures.get(name)?;
        } else {
            return ResourceId::parse(current).ok();
        }
    }
    None
}

/// Each face of a box (pixels) and its corners, centred on the block, in the order the raster
/// takes a face's UVs: the first at `[u0, v0]`, the third at `[u1, v1]`.
pub(crate) fn cuboid_faces(from: Point3, to: Point3) -> [(&'static str, [Point3; 4]); 6] {
    let [x, y, z] = from.map(|v| v - 8.0);
    let [xx, yy, zz] = to.map(|v| v - 8.0);
    [
        ("north", [[xx, yy, z], [xx, y, z], [x, y, z], [x, yy, z]]),
        (
            "south",
            [[x, yy, zz], [x, y, zz], [xx, y, zz], [xx, yy, zz]],
        ),
        ("east", [[xx, yy, zz], [xx, y, zz], [xx, y, z], [xx, yy, z]]),
        ("west", [[x, yy, z], [x, y, z], [x, y, zz], [x, yy, zz]]),
        ("up", [[x, yy, z], [x, yy, zz], [xx, yy, zz], [xx, yy, z]]),
        ("down", [[x, y, zz], [x, y, z], [xx, y, z], [xx, y, zz]]),
    ]
}

fn transform(point: Point3, x_angle: f32, y_angle: f32, scale: f32, icon_size: usize) -> Point3 {
    let [x, y, z] = point;
    let (sy, cy) = y_angle.sin_cos();
    let (sx, cx) = x_angle.sin_cos();
    let rotated_x = x * cy + z * sy;
    let rotated_z = -x * sy + z * cy;
    let rotated_y = y * cx - rotated_z * sx;
    let depth = y * sx + rotated_z * cx;
    let pixels_per_gui_unit = icon_size as f32 / 16.0;
    [
        (8.0 + rotated_x * scale) * pixels_per_gui_unit,
        (8.0 - rotated_y * scale) * pixels_per_gui_unit,
        depth,
    ]
}

fn uv(corner: usize, [u0, v0, u1, v1]: [f32; 4]) -> [f32; 2] {
    match corner {
        0 => [u0, v0],
        1 => [u0, v1],
        2 => [u1, v1],
        _ => [u1, v0],
    }
}

/// `size` is the texels a UV of 1 spans across and down: a block texture's first square
/// frame, or an entity sheet's whole width and height.
fn raster_triangle(
    output: &mut RgbaImage,
    depth: &mut [Vec<f32>],
    source: &RgbaImage,
    size: [u32; 2],
    icon_size: usize,
    shade: f32,
    tint: [u8; 3],
    points: [Point3; 3],
    uvs: [[f32; 2]; 3],
) {
    let edge = |a: Point3, b: Point3, x: f32, y: f32| {
        (x - a[0]) * (b[1] - a[1]) - (y - a[1]) * (b[0] - a[0])
    };
    let area = edge(points[0], points[1], points[2][0], points[2][1]);
    if area.abs() < 0.001 {
        return;
    }
    let min_x = points
        .iter()
        .map(|p| p[0])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_x = points
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(icon_size as f32) as usize;
    let min_y = points
        .iter()
        .map(|p| p[1])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_y = points
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(icon_size as f32) as usize;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let a = edge(points[1], points[2], px, py) / area;
            let b = edge(points[2], points[0], px, py) / area;
            let c = 1.0 - a - b;
            if a < -0.001 || b < -0.001 || c < -0.001 {
                continue;
            }
            let z = a * points[0][2] + b * points[1][2] + c * points[2][2];
            if z < depth[y][x] {
                continue;
            }
            let u = a * uvs[0][0] + b * uvs[1][0] + c * uvs[2][0];
            let v = a * uvs[0][1] + b * uvs[1][1] + c * uvs[2][1];
            let texel = source.get_pixel(
                ((u * size[0] as f32) as u32).min(size[0] - 1),
                ((v * size[1] as f32) as u32).min(size[1] - 1),
            );
            if texel[3] == 0 {
                continue;
            }
            let mut color = *texel;
            for channel in 0..3 {
                color[channel] =
                    (color[channel] as f32 * shade * tint[channel] as f32 / 255.0).round() as u8;
            }
            output.put_pixel(x as u32, y as u32, color);
            depth[y][x] = z;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use std::fs;

    #[test]
    fn glass_sprite_object_resolves_to_full_cube_item_model() {
        let temp = crate::test_dir::tempdir().unwrap();
        let root = temp.path();
        fs::write(
            root.join("pack.mcmeta"),
            r#"{"pack":{"min_format":[97,1],"max_format":[97,1]}}"#,
        )
        .unwrap();
        let model = root.join("assets/test/models/block/glass.json");
        fs::create_dir_all(model.parent().unwrap()).unwrap();
        fs::write(&model, r##"{"textures":{"all":{"force_translucent":true,"sprite":"test:block/glass"}},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#all"},"north":{"texture":"#all"},"west":{"texture":"#all"},"east":{"texture":"#all"},"south":{"texture":"#all"}}}]}"##).unwrap();
        let texture = root.join("assets/test/textures/block/glass.png");
        fs::create_dir_all(texture.parent().unwrap()).unwrap();
        RgbaImage::from_pixel(16, 16, Rgba([180, 200, 210, 255]))
            .save(texture)
            .unwrap();
        let packs = PackStack::open(vec![root.into()]).unwrap();
        let icon = block_icon(&packs, "test:block/glass", &[], 48)
            .unwrap()
            .unwrap();
        let xs = icon
            .enumerate_pixels()
            .filter(|(_, _, p)| p[3] > 0)
            .map(|(x, _, _)| x)
            .collect::<Vec<_>>();
        assert!(xs.iter().max().unwrap() - xs.iter().min().unwrap() > 15);
    }

    #[test]
    fn multi_element_pack_model_renders_with_item_tint() {
        let temp = crate::test_dir::tempdir().unwrap();
        let root = temp.path();
        fs::write(
            root.join("pack.mcmeta"),
            r#"{"pack":{"min_format":[97,1],"max_format":[97,1]}}"#,
        )
        .unwrap();
        let model = root.join("assets/test/models/block/shape.json");
        fs::create_dir_all(model.parent().unwrap()).unwrap();
        fs::write(
            model,
            r##"{"textures":{"all":"test:block/white"},"display":{"gui":{"rotation":[30,135,0],"scale":[0.625,0.625,0.625]}},"elements":[{"from":[0,0,0],"to":[16,8,16],"faces":{"up":{"texture":"#all","tintindex":0},"north":{"texture":"#all","tintindex":0},"east":{"texture":"#all","tintindex":0}}},{"from":[8,8,0],"to":[16,16,16],"faces":{"up":{"texture":"#all","tintindex":0},"north":{"texture":"#all","tintindex":0},"east":{"texture":"#all","tintindex":0}}}]}"##,
        )
        .unwrap();
        let texture = root.join("assets/test/textures/block/white.png");
        fs::create_dir_all(texture.parent().unwrap()).unwrap();
        RgbaImage::from_pixel(16, 16, Rgba([255, 255, 255, 255]))
            .save(texture)
            .unwrap();
        let packs = PackStack::open(vec![root.into()]).unwrap();
        let plain = block_icon(&packs, "test:block/shape", &[], 48)
            .unwrap()
            .unwrap();
        let tinted = block_icon(&packs, "test:block/shape", &[[32, 160, 64]], 48)
            .unwrap()
            .unwrap();
        assert_eq!(plain.dimensions(), (48, 48));
        let visible = plain.pixels().filter(|pixel| pixel[3] != 0).count();
        assert!(visible > 48 * 48 / 10 && visible < 48 * 48);
        assert_eq!(
            visible,
            tinted.pixels().filter(|pixel| pixel[3] != 0).count()
        );
        assert!(plain
            .pixels()
            .zip(tinted.pixels())
            .any(|(a, b)| { a[3] != 0 && b[0] < a[0] && b[1] < a[1] && b[2] < a[2] }));
    }
}
