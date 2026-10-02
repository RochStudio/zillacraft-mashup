//! The apes' moves (`KongCombat`, `KongAttack` and `KongSize` in the mod): which move Kong
//! picks, the timing of each, and the geometry that decides who it hits. Kong and King Kong use
//! exactly the same moves, designed for a 24-block ape: each multiplies every distance, radius
//! and height by its [`Ape::scale`]. Timings, and so the warning before every move, are the same
//! for both, and so is the walking speed (a sprinting player can always outrun either).
//!
//! Distances are blocks at the design size, times ticks since the move started.

use crate::anim::Move;

/// The apes' moves (and Kong's model) are designed for an ape this tall.
pub const DESIGN_HEIGHT: f64 = 24.0;

/// How big an ape is, and what follows from it.
#[derive(Clone, Copy, Debug)]
pub struct Ape {
    /// Size relative to the design size: every distance below is multiplied by this.
    pub scale: f64,
    /// Multiplier on every move's damage.
    pub damage_scale: f32,
    /// Where a boulder lifted overhead leaves its hands, in blocks (forward, up) from its centre
    /// and feet.
    pub hands: (f64, f64),
}

impl Ape {
    /// A distance designed for the 24-block ape, at this size.
    pub fn s(&self, distance: f64) -> f64 {
        distance * self.scale
    }
}

/// A player this close gets a chest-beating roar as a warning; one this close gets fought.
pub const WARN_RADIUS: f64 = 40.0;
pub const ENGAGE_RADIUS: f64 = 24.0;
/// It roars a warning at most this often.
pub const WARNING_MEMORY_TICKS: u32 = 600;
/// The tick of the roar on which a pending phase change takes effect (the second chest beat).
pub const ENRAGE_TICK: u32 = 26;
/// The chest beats of a roar.
pub const CHEST_BEATS: [u32; 4] = [12, 18, 24, 30];
pub const SWIPE_DAMAGE: f32 = 12.0;
pub const SLAM_DAMAGE: f32 = 16.0;
pub const LAND_DAMAGE: f32 = 14.0;
pub const BOULDER_DAMAGE: f32 = 12.0;
/// Every move hits this much harder in phase two.
pub const ENRAGED_BONUS: f32 = 2.0;
/// Chase speed, blocks per tick (4 blocks/s): slower than a walking player. The same for every size.
pub const CHASE_SPEED: f64 = 0.2;
/// Phase two (5 blocks/s): faster than walking, still slower than sprinting.
pub const ENRAGED_CHASE_SPEED: f64 = 0.25;
pub const WANDER_SPEED: f64 = 0.1;
/// It stops walking once its target is this close: under its fists already.
pub const CLOSE_ENOUGH: f64 = 8.0;
/// The rest between two moves.
pub const REST_TICKS: u32 = 14;
pub const REST_TICKS_ENRAGED: u32 = 6;
/// How fast it rises onto higher ground and drops to lower, in blocks per tick.
pub const CLIMB_PER_TICK: f64 = 1.0;
pub const DROP_PER_TICK: f64 = 1.5;
/// How far above its feet it looks for ground (it steps up onto anything lower), and below.
pub const SCAN_ABOVE: f64 = 12.0;
pub const SCAN_BELOW: f64 = 40.0;
pub const LEAP_GRAVITY: f64 = 0.1;
/// Distance walked between two heavy footsteps.
pub const STRIDE_LENGTH: f64 = 6.0;
/// It sees this far at most.
pub const SIGHT: f64 = 160.0;

/// Choose a swipe when the target is at most this far away (horizontal, centre to centre).
pub const SWIPE_RANGE: f64 = 15.0;
/// The swipe connects if the target's near edge is within this distance of the ape's centre.
pub const SWIPE_REACH: f64 = 18.0;
/// Half the swipe's arc, degrees (150 in all). Nothing behind the ape is hit.
pub const SWIPE_HALF_ARC: f64 = 75.0;
/// Anything this close is between its feet and always inside the sweep.
pub const SWIPE_POINT_BLANK: f64 = 5.0;
pub const SLAM_RANGE: f64 = 24.0;
/// The fists land this far in front of its centre; the shockwave starts there.
pub const SLAM_FORWARD_OFFSET: f64 = 9.5;
pub const SLAM_RADIUS: f64 = 18.0;
pub const SLAM_RADIUS_ENRAGED: f64 = 22.0;
pub const LAND_RADIUS: f64 = 12.0;
pub const LAND_RADIUS_ENRAGED: f64 = 15.0;
pub const SHOCKWAVE_START_RADIUS: f64 = 3.0;
/// How fast the shockwave ring travels outwards, blocks per tick.
pub const SHOCKWAVE_SPEED: f64 = 2.5;
/// Ground attacks (swipe, slam, landing) travel along the ground: anything more than this far
/// above or below its feet is out of their reach. The same limit decides when it throws instead.
pub const GROUND_REACH_DY: f64 = 3.0;
pub const LEAP_MIN: f64 = 24.0;
pub const LEAP_MAX: f64 = 48.0;
pub const BOULDER_MIN: f64 = 20.0;
pub const BOULDER_MAX: f64 = 80.0;
pub const BOULDER_BLAST_RADIUS: f64 = 6.0;
/// Moves are sized for player-width targets (half-width 0.3): wider ones are judged by their near edge.
pub const PLAYER_HALF_WIDTH: f64 = 0.3;
/// A thrown boulder's hitbox, blocks across, and its gravity, at the design size: an ape's
/// boulder is scaled with it, and falls faster the bigger it is, so its arc keeps its shape.
pub const BOULDER_SIZE: f64 = 4.0;
pub const BOULDER_GRAVITY: f64 = 0.05;
/// A boulder that hasn't landed after this long shatters where it is.
pub const BOULDER_MAX_LIFE: u32 = 160;
/// The boulder shows in its hands from when it tears it up until it lets go.
pub const BOULDER_PICKUP_TICK: f32 = 10.0;

/// Shortest time a leap spends in the air, and a thrown boulder before it can land.
pub const MIN_LEAP_FLIGHT_TICKS: u32 = 18;
pub const MIN_BOULDER_FLIGHT_TICKS: u32 = 20;

/// An ape's move: how long it lasts, the first tick it can hurt (or lets go of a boulder, or
/// leaps), and how long before it makes it again.
#[derive(Clone, Copy, Debug)]
pub struct Timing {
    pub duration: u32,
    pub impact: u32,
    pub cooldown: u32,
}

/// `KongAttack`: a chest-beating roar (a warning, and the start of phase two); a backhand across
/// 150 degrees in front of it; a two-fisted ground pound whose shockwave rolls out along the
/// ground; a boulder ripped up and lobbed at where its target stood; a leap onto where its target
/// stood; and the touchdown after a leap, a smaller shockwave and a long recovery.
pub fn timing(movement: Move) -> Timing {
    let (duration, impact, cooldown) = match movement {
        Move::Roar => (50, 0, 0),
        Move::Swipe => (30, 14, 36),
        Move::Slam => (56, 24, 120),
        Move::Boulder => (44, 30, 90),
        Move::Leap => (80, 16, 160),
        Move::Land => (26, 1, 0),
        _ => (0, 0, 0),
    };
    Timing { duration, impact, cooldown }
}

/// A boulder an ape has thrown (`KongBoulder`): it flies a fixed arc to the spot its target stood
/// on, marked on the ground the whole way, crashes through foliage, and shatters on the ground
/// or on whoever it hits, hurting everything in its blast that isn't behind solid cover. It
/// never breaks blocks.
#[derive(Clone, Debug)]
pub struct Boulder {
    pub id: u32,
    /// The bottom of its box, at its middle.
    pub at: [f64; 3],
    pub velocity: [f64; 3],
    /// Where it comes down.
    pub landing: [f64; 3],
    /// Its thrower's scale: its size, blast and fall follow it.
    pub scale: f64,
    /// What it does where it lands.
    pub damage: f32,
    /// Ticks in the air.
    pub life: u32,
}

impl Boulder {
    pub fn size(&self) -> f64 {
        BOULDER_SIZE * self.scale
    }

    pub fn blast_radius(&self) -> f64 {
        BOULDER_BLAST_RADIUS * self.scale
    }

    pub fn gravity(&self) -> f64 {
        BOULDER_GRAVITY * self.scale
    }
}

/// Everything the move picker needs to know about the fight.
#[derive(Clone, Copy, Debug)]
pub struct Situation {
    pub distance: f64,
    pub dy: f64,
    pub can_see: bool,
    pub enraged: bool,
    pub swipe_ready: bool,
    pub slam_ready: bool,
    pub boulder_ready: bool,
    pub leap_ready: bool,
    pub previous: Move,
}

/// Picks its next move, or [`Move::None`] to keep closing in. `roll` is uniform in [0, 1).
pub fn choose(s: &Situation, roll: f64) -> Move {
    if !s.can_see {
        return Move::None;
    }
    let same_level = s.dy.abs() <= GROUND_REACH_DY;
    let d = s.distance;
    let mut options: Vec<(Move, u32)> = Vec::with_capacity(3);
    if same_level && d <= SWIPE_RANGE {
        if s.swipe_ready {
            options.push((Move::Swipe, 5));
        }
        if s.slam_ready {
            options.push((Move::Slam, if s.enraged { 3 } else { 2 }));
        }
    } else if same_level && d <= SLAM_RANGE {
        if s.slam_ready {
            options.push((Move::Slam, 3));
        }
    } else if same_level && (LEAP_MIN..=LEAP_MAX).contains(&d) {
        if s.leap_ready {
            options.push((Move::Leap, if s.enraged { 4 } else { 3 }));
        }
        if s.boulder_ready {
            options.push((Move::Boulder, 2));
        }
    } else if (!same_level || d > LEAP_MAX) && d <= BOULDER_MAX && s.boulder_ready {
        // Up on a ledge or down in a pit, out of reach of the ground attacks, or too far to leap: it throws.
        options.push((Move::Boulder, 1));
    }
    // It doesn't repeat a heavy move back to back when there is something else to do.
    if options.len() > 1 && s.previous != Move::Swipe {
        options.retain(|&(m, _)| m != s.previous);
    }
    let total: u32 = options.iter().map(|&(_, w)| w).sum();
    let mut pick = roll.clamp(0.0, 0.999_999) * f64::from(total);
    for &(m, weight) in &options {
        pick -= f64::from(weight);
        if pick < 0.0 {
            return m;
        }
    }
    options.last().map_or(Move::None, |&(m, _)| m)
}

/// The distance the move picker works with: centre to centre for a player-sized target, and for
/// a wider one the distance to its near edge, as if a player stood there.
pub fn effective_distance(centre_distance: f64, half_width: f64) -> f64 {
    (centre_distance - (half_width - PLAYER_HALF_WIDTH).max(0.0)).max(0.0)
}

/// Whether a target `(dx, dz)` from the ape is inside its swipe, facing `yaw` degrees (Minecraft
/// yaw: 0 faces +Z, 90 faces -X), judged by its near edge `half_width` out.
pub fn in_swipe_arc(yaw: f32, dx: f64, dz: f64, half_width: f64) -> bool {
    let dist = dx.hypot(dz);
    if dist - half_width > SWIPE_REACH {
        return false;
    }
    if dist < SWIPE_POINT_BLANK {
        return true;
    }
    let yaw = f64::from(yaw).to_radians();
    let cos = ((dx * -yaw.sin() + dz * yaw.cos()) / dist).clamp(-1.0, 1.0);
    let width_allowance = half_width.atan2(dist).to_degrees();
    cos.acos().to_degrees() <= SWIPE_HALF_ARC + width_allowance
}

pub fn slam_radius(enraged: bool) -> f64 {
    if enraged { SLAM_RADIUS_ENRAGED } else { SLAM_RADIUS }
}

pub fn land_radius(enraged: bool) -> f64 {
    if enraged { LAND_RADIUS_ENRAGED } else { LAND_RADIUS }
}

/// The shockwave ring's radius `ticks` after impact (tick 0 is the impact itself).
pub fn shockwave_radius(ticks: u32) -> f64 {
    SHOCKWAVE_START_RADIUS + f64::from(ticks) * SHOCKWAVE_SPEED
}

/// Ticks after impact until the ring has reached `max_radius`.
pub fn shockwave_ticks(max_radius: f64) -> u32 {
    ((max_radius - SHOCKWAVE_START_RADIUS) / SHOCKWAVE_SPEED).ceil() as u32
}

/// Whether a target is close enough for the ring to reach it at all.
pub fn in_shockwave_range(distance: f64, half_width: f64, max_radius: f64) -> bool {
    distance - half_width <= max_radius
}

/// Whether the ring has reached a target by `ticks` after impact.
pub fn shockwave_reached(distance: f64, half_width: f64, ticks: u32) -> bool {
    distance - half_width <= shockwave_radius(ticks)
}

/// Damage falls off from the full amount at the centre to half at the ring's edge.
pub fn shockwave_damage(distance: f64, max_radius: f64, centre_damage: f32) -> f32 {
    let t = (distance / max_radius).clamp(0.0, 1.0);
    (f64::from(centre_damage) * (1.0 - 0.5 * t)) as f32
}

/// A boulder's blast: full where it lands, down to 40% at the edge.
pub fn boulder_damage(distance: f64, centre_damage: f32) -> f32 {
    let t = (distance / BOULDER_BLAST_RADIUS).clamp(0.0, 1.0);
    (f64::from(centre_damage) * (1.0 - 0.6 * t)) as f32
}

/// The launch velocity that carries a body from `from` to `to` in exactly `ticks` ticks, with
/// Minecraft's integration (move, then fall by `gravity`) and no drag.
pub fn ballistic_velocity(from: [f64; 3], to: [f64; 3], ticks: u32, gravity: f64) -> [f64; 3] {
    let t = f64::from(ticks.max(1));
    [(to[0] - from[0]) / t, (to[1] - from[1] + gravity * t * (t - 1.0) / 2.0) / t, (to[2] - from[2]) / t]
}

pub fn boulder_flight_ticks(horizontal_distance: f64) -> u32 {
    ((12.0 + horizontal_distance * 0.5).round() as u32).clamp(MIN_BOULDER_FLIGHT_TICKS, 50)
}

pub fn leap_flight_ticks(horizontal_distance: f64) -> u32 {
    ((10.0 + horizontal_distance * 0.5).round() as u32).clamp(MIN_LEAP_FLIGHT_TICKS, 36)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn at(distance: f64, dy: f64) -> Situation {
        Situation {
            distance,
            dy,
            can_see: true,
            enraged: false,
            swipe_ready: true,
            slam_ready: true,
            boulder_ready: true,
            leap_ready: true,
            previous: Move::None,
        }
    }

    fn possible(s: Situation) -> BTreeSet<String> {
        (0..200).map(|i| format!("{:?}", choose(&s, f64::from(i) / 200.0))).collect()
    }

    fn set(moves: &[Move]) -> BTreeSet<String> {
        moves.iter().map(|m| format!("{m:?}")).collect()
    }

    #[test]
    fn ranges_fit_a_24_block_giant() {
        // Ordered bands with no gaps between melee, slam, leap and throwing range.
        const { assert!(SWIPE_RANGE < SLAM_RANGE && SLAM_RANGE <= LEAP_MIN && BOULDER_MIN <= LEAP_MIN && LEAP_MAX < BOULDER_MAX) };
        // The fists land inside the slam's own ring, and the swipe reaches past where it's chosen.
        const { assert!(SLAM_FORWARD_OFFSET < SLAM_RADIUS && SWIPE_REACH > SWIPE_RANGE) };
    }

    #[test]
    fn picks_moves_by_distance_and_level() {
        assert_eq!(possible(Situation { can_see: false, ..at(5.0, 0.0) }), set(&[Move::None]));
        assert_eq!(possible(at(SWIPE_RANGE - 3.0, 0.0)), set(&[Move::Swipe, Move::Slam]));
        assert_eq!(possible(at(2.0, 0.0)), set(&[Move::Swipe, Move::Slam]), "between its feet");
        let mid = (SWIPE_RANGE + SLAM_RANGE) / 2.0;
        assert_eq!(possible(at(mid, 0.0)), set(&[Move::Slam]));
        assert_eq!(possible(Situation { slam_ready: false, ..at(mid, 0.0) }), set(&[Move::None]));
        let far = (LEAP_MIN + LEAP_MAX) / 2.0;
        assert_eq!(possible(at(far, 0.0)), set(&[Move::Leap, Move::Boulder]));
        let ledge = GROUND_REACH_DY + 3.0;
        assert_eq!(possible(at(6.0, ledge)), set(&[Move::Boulder]));
        assert_eq!(possible(at(40.0, -ledge)), set(&[Move::Boulder]));
        assert_eq!(possible(at(6.0, GROUND_REACH_DY)), set(&[Move::Swipe, Move::Slam]), "just within ground reach");
        assert_eq!(possible(at(BOULDER_MAX + 5.0, 0.0)), set(&[Move::None]));
        assert_eq!(possible(at(LEAP_MAX + 10.0, 0.0)), set(&[Move::Boulder]));
    }

    #[test]
    fn heavy_moves_are_not_repeated_when_there_is_an_alternative() {
        let far = (LEAP_MIN + LEAP_MAX) / 2.0;
        assert_eq!(possible(Situation { previous: Move::Leap, ..at(far, 0.0) }), set(&[Move::Boulder]));
        assert_eq!(possible(Situation { previous: Move::Slam, ..at(5.0, 0.0) }), set(&[Move::Swipe]));
    }

    #[test]
    fn swipe_hits_in_front_but_not_behind() {
        // Facing +Z (yaw 0).
        assert!(in_swipe_arc(0.0, 0.0, SWIPE_REACH - 2.0, 0.3));
        assert!(in_swipe_arc(0.0, 10.0, 10.0, 0.3), "45 degrees to the side is inside the arc");
        assert!(!in_swipe_arc(0.0, 0.0, -12.0, 0.3), "directly behind is safe");
        assert!(!in_swipe_arc(0.0, 12.0, -5.0, 0.3), "behind the shoulder is safe");
        assert!(!in_swipe_arc(0.0, 0.0, SWIPE_REACH + 2.0, 0.3), "beyond reach is safe");
        assert!(in_swipe_arc(0.0, 1.5, -1.5, 0.3), "between its feet is always hit");
        // Yaw 90 faces -X.
        assert!(in_swipe_arc(90.0, -12.0, 0.0, 0.3) && !in_swipe_arc(90.0, 12.0, 0.0, 0.3));
    }

    #[test]
    fn shockwave_reaches_targets_in_distance_order() {
        let first = |distance: f64| (0..100).find(|&t| shockwave_reached(distance, 0.3, t));
        assert_eq!(first(1.0), Some(0));
        assert!(first(5.0) < first(SLAM_RADIUS - 1.0));
        assert!(shockwave_ticks(SLAM_RADIUS) >= first(SLAM_RADIUS - 1.0).unwrap());
        assert!(!in_shockwave_range(SLAM_RADIUS + 1.0, 0.3, SLAM_RADIUS));
        assert!(in_shockwave_range(SLAM_RADIUS + 1.0, 0.3, SLAM_RADIUS_ENRAGED));
        assert!((shockwave_damage(0.0, SLAM_RADIUS, 16.0) - 16.0).abs() < 1e-4);
        assert!((shockwave_damage(SLAM_RADIUS, SLAM_RADIUS, 16.0) - 8.0).abs() < 1e-4);
        assert!(boulder_damage(BOULDER_BLAST_RADIUS, 12.0) < boulder_damage(0.0, 12.0));
    }

    #[test]
    fn a_ballistic_launch_lands_on_its_mark() {
        let (from, to, ticks, gravity) = ([0.0, 30.0, 0.0], [40.0, 2.0, -15.0], 30, 0.1);
        let mut p = from;
        let mut v = ballistic_velocity(from, to, ticks, gravity);
        for _ in 0..ticks {
            p = [p[0] + v[0], p[1] + v[1], p[2] + v[2]];
            v[1] -= gravity;
        }
        assert!((0..3).all(|i| (p[i] - to[i]).abs() < 1e-9), "{p:?}");
    }
}
