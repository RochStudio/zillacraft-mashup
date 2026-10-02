//! Creative flight, as Minecraft flies: double-tap jump to take off or drop, jump to rise,
//! crouch or prone to sink and sprint to go faster, level with the ground whichever way you
//! look.
//! Walls, floors and ceilings still stop you, and touching down ends the flight.
//!
//! Only commands carrying [`buttons::CREATIVE`] may fly. Whether you are flying is
//! [`pm_flags::FLYING`], and the double tap is timed against `jump_time`, so a replay of the
//! same commands flies the same way.

use playerstate_iw4::{PlayerState, UserCmd, buttons, pm_flags};

use crate::{CollisionBackend, MoveBounds, Pml, jump, slide_move};

/// Two jump presses this close together (`LocalPlayer`'s seven-tick `jumpTriggerTime`).
pub const DOUBLE_TAP_MS: i32 = 350;
/// Flying speed over the player's walking speed (Minecraft flies at 10.9 blocks a second),
/// doubled while sprinting, and the speed of rising and sinking.
const SPEED_SCALE: f32 = 2.0;
const SPRINT_SCALE: f32 = 2.0;
const VERTICAL_SCALE: f32 = 1.4;
/// How quickly the velocity closes on what the keys ask (about half a second to get going or
/// glide to a stop): friction on the difference, as on the ground never less than at this
/// speed, so the velocity's rounding to whole units each frame can't hold a slow drift.
const FRICTION: f32 = 6.0;
const STOP_SPEED: f32 = 100.0;

#[must_use]
pub fn flying(ps: &PlayerState) -> bool {
    ps.pm_flags & pm_flags::FLYING != 0
}

/// Without creative mode there is no flight.
pub fn check_creative(ps: &mut PlayerState, cmd: &UserCmd) {
    if cmd.buttons & buttons::CREATIVE == 0 {
        ps.pm_flags &= !pm_flags::FLYING;
    }
}

/// Starts or ends a flight on a double tap of jump, once the ground trace has said whether
/// the player stands. The tap from the ground is a jump (which stamps `jump_time` itself), so
/// the second tap, in the air, takes off.
pub fn check_toggle(ps: &mut PlayerState, pml: &Pml, cmd: &UserCmd, old_buttons: u32) {
    let pressed = cmd.buttons & buttons::JUMP != 0 && old_buttons & buttons::JUMP == 0;
    if cmd.buttons & buttons::CREATIVE == 0 || !pressed || (!flying(ps) && pml.walking != 0) {
        return;
    }
    if cmd.server_time.wrapping_sub(ps.jump_time) < DOUBLE_TAP_MS {
        ps.pm_flags ^= pm_flags::FLYING;
        jump::clear_state(ps);
        // A third tap starts a new double tap.
        ps.jump_time = cmd.server_time.wrapping_sub(DOUBLE_TAP_MS);
    } else {
        ps.jump_time = cmd.server_time;
    }
}

/// Ends the flight once standing on something, unless rising off it.
pub fn check_landing(ps: &mut PlayerState, pml: &Pml, cmd: &UserCmd) {
    if flying(ps) && pml.walking != 0 && cmd.buttons & buttons::JUMP == 0 {
        ps.pm_flags &= !pm_flags::FLYING;
    }
}

/// One step of flight: the velocity closes on what the keys ask, then slides through the world
/// without gravity.
pub fn fly_move<C: CollisionBackend>(ps: &mut PlayerState, pml: &Pml, cmd: &UserCmd, bounds: MoveBounds, collision: &C) {
    let mut forward = [pml.forward[0], pml.forward[1], 0.0];
    let mut right = [pml.right[0], pml.right[1], 0.0];
    normalize(&mut forward);
    normalize(&mut right);
    let (f, r) = (f32::from(cmd.forwardmove), f32::from(cmd.rightmove));
    let mut flat = [forward[0] * f + right[0] * r, forward[1] * f + right[1] * r, 0.0];
    let flat_speed = if normalize(&mut flat) > 0.0 {
        let fast = if cmd.buttons & buttons::SPRINT != 0 { SPRINT_SCALE } else { 1.0 };
        ps.speed as f32 * SPEED_SCALE * fast * f.abs().max(r.abs()) / 127.0
    } else {
        0.0
    };
    // Crouch or prone (C or Ctrl) sinks.
    let sink = cmd.buttons & (buttons::CROUCH | buttons::PRONE) != 0;
    let rise = i8::from(cmd.buttons & buttons::JUMP != 0) - i8::from(sink);
    let wish = [
        flat[0] * flat_speed,
        flat[1] * flat_speed,
        f32::from(rise) * ps.speed as f32 * VERTICAL_SCALE,
    ];

    let off = [ps.velocity[0] - wish[0], ps.velocity[1] - wish[1], ps.velocity[2] - wish[2]];
    let gap = length(off);
    let left = if gap > 0.0 { (gap - gap.max(STOP_SPEED) * FRICTION * pml.frametime).max(0.0) / gap } else { 0.0 };
    ps.velocity = [wish[0] + off[0] * left, wish[1] + off[1] * left, wish[2] + off[2] * left];

    slide_move(ps, pml, collision, bounds.mins, bounds.maxs, bounds.tracemask, None);
}

fn length(v: [f32; 3]) -> f32 {
    libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2])
}

fn normalize(v: &mut [f32; 3]) -> f32 {
    let length = length(*v);
    if length > 0.0 {
        v.iter_mut().for_each(|c| *c /= length);
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GroundTraceInput;
    use trace_iw4::Trace;

    /// Open air everywhere.
    struct Open;

    impl CollisionBackend for Open {
        fn trace(&self, input: GroundTraceInput) -> Trace {
            Trace { fraction: 1.0, endpos: input.end, ..Trace::default() }
        }
    }

    const FRAME_MS: i32 = 16;
    const TAP: u32 = buttons::CREATIVE | buttons::JUMP;

    fn pml(walking: bool) -> Pml {
        Pml {
            forward: [1.0, 0.0, 0.0],
            right: [0.0, -1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            frametime: FRAME_MS as f32 * 0.001,
            msec: FRAME_MS,
            walking: u32::from(walking),
            ground_plane: 0,
            almost_ground_plane: 0,
            ground_trace: [0; 11],
            previous_origin: [0.0; 3],
            previous_velocity: [0.0; 3],
            holdrand: 0,
            jump_animations: [None; 4],
            mantle_movetype: None,
            landing_animation: false,
        }
    }

    fn cmd(server_time: i32, held: u32) -> UserCmd {
        UserCmd { server_time, buttons: held, ..UserCmd::default() }
    }

    fn player() -> PlayerState {
        PlayerState { speed: 190, ..PlayerState::ZERO }
    }

    #[test]
    fn a_tap_on_the_ground_is_left_to_the_jump() {
        let mut ps = player();
        check_toggle(&mut ps, &pml(true), &cmd(1000, TAP), buttons::CREATIVE);
        assert!(!flying(&ps));
        assert_eq!(ps.jump_time, 0);
    }

    #[test]
    fn a_double_tap_takes_off_and_another_drops() {
        let mut ps = player();
        // The first tap jumped off the ground at 1000.
        ps.jump_time = 1000;
        check_toggle(&mut ps, &pml(false), &cmd(1200, TAP), buttons::CREATIVE);
        assert!(flying(&ps));
        // Holding jump is no tap.
        check_toggle(&mut ps, &pml(false), &cmd(1216, TAP), TAP);
        assert!(flying(&ps));
        // Two taps too far apart, then one in time.
        check_toggle(&mut ps, &pml(false), &cmd(2000, TAP), buttons::CREATIVE);
        check_toggle(&mut ps, &pml(false), &cmd(2400, TAP), buttons::CREATIVE);
        assert!(flying(&ps));
        check_toggle(&mut ps, &pml(false), &cmd(2600, TAP), buttons::CREATIVE);
        assert!(!flying(&ps));
    }

    #[test]
    fn no_flight_without_creative() {
        let mut ps = player();
        ps.jump_time = 1000;
        check_toggle(&mut ps, &pml(false), &cmd(1200, buttons::JUMP), 0);
        assert!(!flying(&ps));
        ps.pm_flags |= pm_flags::FLYING;
        check_creative(&mut ps, &cmd(1300, 0));
        assert!(!flying(&ps));
    }

    #[test]
    fn touching_down_ends_a_flight_unless_rising() {
        let mut ps = player();
        ps.pm_flags |= pm_flags::FLYING;
        check_landing(&mut ps, &pml(true), &cmd(1000, TAP));
        assert!(flying(&ps));
        check_landing(&mut ps, &pml(true), &cmd(1016, buttons::CREATIVE));
        assert!(!flying(&ps));
    }

    /// Flies for `frames` frames, holding forward as given and `held`, rounding the velocity
    /// to whole units after each as a movement step does.
    fn fly(ps: &mut PlayerState, frames: i32, forwardmove: i8, held: u32) {
        let bounds = MoveBounds { mins: [-15.0, -15.0, 0.0], maxs: [15.0, 15.0, 70.0], tracemask: 0 };
        for frame in 0..frames {
            let cmd = UserCmd { forwardmove, ..cmd(frame * FRAME_MS, held | buttons::CREATIVE) };
            fly_move(ps, &pml(false), &cmd, bounds, &Open);
            crate::snap_vector(&mut ps.velocity);
        }
    }

    #[test]
    fn flight_keeps_its_speeds_and_holds_still() {
        let mut ps = player();
        fly(&mut ps, 120, 127, 0);
        assert_eq!(ps.velocity, [380.0, 0.0, 0.0]);
        fly(&mut ps, 120, 127, buttons::SPRINT);
        assert_eq!(ps.velocity, [760.0, 0.0, 0.0]);
        fly(&mut ps, 120, 0, buttons::JUMP);
        assert_eq!(ps.velocity, [0.0, 0.0, 266.0]);
        fly(&mut ps, 120, 0, buttons::CROUCH);
        assert_eq!(ps.velocity, [0.0, 0.0, -266.0]);
        fly(&mut ps, 120, 0, buttons::PRONE);
        assert_eq!(ps.velocity, [0.0, 0.0, -266.0]);
        // Let go: it glides to a stop, and stays there whatever the rounding.
        fly(&mut ps, 120, 0, 0);
        assert_eq!(ps.velocity, [0.0; 3]);
        let at = ps.origin;
        fly(&mut ps, 120, 0, 0);
        assert_eq!(ps.origin, at);
    }
}
