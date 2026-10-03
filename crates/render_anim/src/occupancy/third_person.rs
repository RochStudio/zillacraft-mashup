use asset_world::ClipCollision;
use net::PresentedSnapshot;
use playerstate_iw4::{
    CG_CAMERA_PULLBACK_BOX_HALF, CG_CAMERA_PULLBACK_CLIPMASK, CG_THIRD_PERSON_RANGE_DEFAULT,
    KillCamMode, OffsetThirdPersonViewInputs, ThirdPersonViewInputs, is_third_person_view,
    offset_third_person_view,
};
use sim::ClientId;

use render_scene::WorldCameraPose;

pub const CG_THIRD_PERSON_ANGLE_MP: f32 = 356.0;

pub fn presented_is_third_person(
    presented: &PresentedSnapshot,
    local: ClientId,
    in_killcam: bool,
) -> bool {
    let Some(ps) = presented.player(local) else {
        return false;
    };
    if in_killcam && ps.kill_cam_entity != playerstate_iw4::ENTITYNUM_NONE {
        return true;
    }
    if remote_missile_camera(presented, local, 0).is_some() {
        return true;
    }
    is_third_person_view(ThirdPersonViewInputs {
        pm_type: ps.pm_type,
        other_flags: ps.other_flags,
        link_flags: ps.link_flags,
        cg_third_person: frame::third_person() != frame::ThirdPerson::Off,
        in_killcam,
        killcam_mode: KillCamMode::Mode0,
    })
}

pub fn remote_missile_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
    at_time: i32,
) -> Option<WorldCameraPose> {
    let link = presented
        .snapshot()?
        .meta
        .for_client(local)?
        .remote_missile
        .filter(|link| link.unlink_at_ms.is_none())?;
    let missile = presented
        .presented_projectiles()
        .iter()
        .find(|p| p.authoritative_id() == Some(link.projectile))?;
    Some(WorldCameraPose {
        origin: missile.origin_at(at_time),
        angles: link.angles,
    })
}

pub fn death_watch_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
    clip: Option<&ClipCollision>,
) -> Option<WorldCameraPose> {
    third_person_camera(presented, local, clip, CG_THIRD_PERSON_ANGLE_MP)
}

/// `CG_OffsetThirdPersonView`: the camera `cg_thirdPersonRange` back from the player's eye,
/// swung `angle` degrees round from behind them (180 puts it in front, looking back), pulled
/// in short of walls in the map's brushes and the Minecraft world's blocks alike.
pub fn third_person_camera(
    presented: &PresentedSnapshot,
    local: ClientId,
    clip: Option<&ClipCollision>,
    angle: f32,
) -> Option<WorldCameraPose> {
    let ps = presented.player(local)?;
    if clip.is_none() && !sim::voxel::active() {
        return None;
    }
    let offset = presented.view_offset();
    let yaw = presented
        .snapshot()?
        .meta
        .for_client(local)
        .map(|meta| meta.look_at_killer_yaw as f32)
        .unwrap_or(ps.viewangles[1]);

    let half = CG_CAMERA_PULLBACK_BOX_HALF;
    let trace = |start: [f32; 3], end: [f32; 3]| {
        let brushes = clip.map_or(1.0, |clip| {
            clip.sweep_box(start, end, [-half, -half, -half], [half, half, half], CG_CAMERA_PULLBACK_CLIPMASK)
                .fraction
        });
        brushes.min(sim::voxel::camera_fraction(start, end, half))
    };
    let view = offset_third_person_view(
        OffsetThirdPersonViewInputs {
            origin: [
                ps.origin[0] + offset[0],
                ps.origin[1] + offset[1],
                ps.origin[2] + offset[2],
            ],
            view_height_current: ps.view_height_current,
            viewangles: ps.viewangles,
            pm_type: ps.pm_type,
            look_at_killer_yaw: yaw,

            corpse_j_mainroot: None,
            other_flags: ps.other_flags,
            delta_time: ps.delta_time,
            cg_third_person_angle: angle,
            cg_third_person_range: CG_THIRD_PERSON_RANGE_DEFAULT,
        },
        trace,
    );
    // Turned round to face the player, the camera looks at their head.
    let angles = if (angle - 180.0).abs() < 90.0 {
        let eye = [ps.origin[0] + offset[0], ps.origin[1] + offset[1], ps.origin[2] + offset[2] + ps.view_height_current];
        let d = [eye[0] - view.origin[0], eye[1] - view.origin[1], eye[2] - view.origin[2]];
        let pitch = -d[2].atan2(d[0].hypot(d[1])).to_degrees();
        [pitch, d[1].atan2(d[0]).to_degrees(), 0.0]
    } else {
        view.angles
    };
    Some(WorldCameraPose {
        origin: view.origin,
        angles,
    })
}
