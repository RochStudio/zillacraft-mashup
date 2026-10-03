//! How an ape behaves (`Kong` in the mod). It ignores players who keep their distance, roars a
//! chest-beating warning at anyone inside its warning radius, and fights whoever comes inside
//! its engage radius or hurts it. It picks its moves by distance and level (`ape::choose`):
//! every move is telegraphed with an animation, a sound and red marks on the ground before it
//! can hurt, and the ground moves sweep along the ground. Below half health it roars once more
//! and is enraged for good. It never crushes anything: it wades through the world.
//!
//! The host walks it (`Walk` speeds are shares of its chasing speed) and keeps its feet on the
//! ground ([`ground_under`]); the airborne part of a leap is scripted here.

use super::{Arena, Body, Brain, Cause, Effect, Seen, Walk, approach_degrees, distance, horizontal_from, normalize, scale};
use crate::anim::Move;
use crate::ape::{self, Ape, Boulder, Situation};
use crate::hitboxes::Aabb;
use crate::species::Species;

/// A target out of sight for this long is given up (`TargetGoal`'s unseen memory).
const UNSEEN_MEMORY_TICKS: u32 = 200;
/// How often it looks round its territory for players (`TerritoryGoal`).
const SCAN_INTERVAL: u32 = 10;
/// Idle, it looks at players this close (`LookAtPlayerGoal`), at its design size.
const LOOK_RANGE: f64 = 48.0;
/// How far a boulder's path may pass from someone and still hit them (`ProjectileUtil`).
const PROJECTILE_MARGIN: f64 = 0.3;

/// What an ape keeps track of besides what every kaiju does.
#[derive(Debug, Default)]
pub(super) struct ApeState {
    /// Ticks it has lived (`tickCount`).
    age: u32,
    pub(super) enraged: bool,
    /// Hurt below half health: it roars and is enraged as soon as it is free to.
    pub(super) pending_enrage: bool,
    /// The tick each move is ready again, in `slot` order.
    ready_at: [u32; 5],
    /// It rests until this tick after a move.
    next_move: u32,
    previous: Move,
    /// Who the current move is aimed at.
    attack_target: Option<u64>,
    /// Where a slam's or a landing's shockwave starts.
    impact: [f64; 3],
    leap_target: [f64; 3],
    leap_velocity: [f64; 3],
    leap_flight: u32,
    /// Ticks in the air on a leap (0 on the ground).
    airborne: u32,
    /// Where its target stood a little before a throw, to lead the second boulder.
    tracked: Option<[f64; 3]>,
    last_warning: Option<u32>,
    next_scan: u32,
    /// Ticks its target has been out of sight.
    unseen: u32,
    /// Ambling about: where to, and for how much longer.
    wander: Option<([f64; 2], u32)>,
    next_boulder: u32,
}

/// Each move's place in `ApeState::ready_at`.
fn slot(movement: Move) -> Option<usize> {
    match movement {
        Move::Swipe => Some(0),
        Move::Slam => Some(1),
        Move::Boulder => Some(2),
        Move::Leap => Some(3),
        Move::Roar => Some(4),
        _ => None,
    }
}

impl Brain {
    /// Whether an ape is enraged: glowing eyes, a red boss bar, harder blows.
    pub fn enraged(&self) -> bool {
        self.ape.enraged
    }

    /// Whether an ape is in the air on a leap (the host leaves its height alone).
    pub fn airborne(&self) -> bool {
        self.movement == Move::Leap && self.ape.airborne > 0
    }

    pub(super) fn tick_ape(&mut self, ape: Ape, body: &mut Body, seen: &[Seen], arena: &impl Arena, walk_out: &mut Option<Walk>, effects: &mut Vec<Effect>) {
        self.ape.age += 1;
        let target = if let Some((to, left)) = self.retreat {
            let arrived = (to[0] - body.feet[0]).hypot(to[1] - body.feet[2]) <= 6.0;
            self.retreat = (left > 0 && !arrived).then_some((to, left - 1));
            None
        } else {
            self.ape_target(ape, body, seen, arena, effects)
        };

        if self.movement == Move::None {
            if let Some(t) = target {
                self.fight(ape, body, &t, arena, walk_out, effects);
            } else if let Some((to, _)) = self.retreat {
                *walk_out = Some(Walk { to, speed: 1.0 });
            } else {
                self.amble(ape, body, seen, walk_out);
            }
        }
        if self.ape.pending_enrage && self.movement == Move::None {
            self.start_ape_move(ape, Move::Roar, body, target.as_ref(), arena, effects);
        }
        if self.movement != Move::None {
            *walk_out = None;
            self.tick_ape_move(ape, body, seen, arena, effects);
        }
    }

    /// Its target: the one it has while that stays in range and in sight now and then;
    /// otherwise a player who comes inside its engage radius, after a warning roar at anyone
    /// inside its warning radius.
    fn ape_target(&mut self, ape: Ape, body: &mut Body, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) -> Option<Seen> {
        if let Some(id) = self.target {
            let range = self.species.follow_range;
            let kept = seen.iter().find(|s| s.id == id && (s.targetable || self.provoked) && distance(body.feet, s.feet) <= range);
            if let Some(t) = kept {
                self.ape.unseen = if t.visible { 0 } else { self.ape.unseen + 1 };
                if self.ape.unseen <= UNSEEN_MEMORY_TICKS {
                    return Some(*t);
                }
            }
            self.target = None;
            self.provoked = false;
        }
        if self.ape.age < self.ape.next_scan {
            return None;
        }
        self.ape.next_scan = self.ape.age + SCAN_INTERVAL;
        let nearest = seen
            .iter()
            .filter(|s| s.is_player && s.targetable && s.visible && distance(body.feet, s.feet) <= ape.s(ape::WARN_RADIUS))
            .min_by(|a, b| distance(body.feet, a.feet).total_cmp(&distance(body.feet, b.feet)))
            .copied()?;
        if self.movement == Move::None && !self.warned_lately() {
            self.ape.last_warning = Some(self.ape.age);
            self.start_ape_move(ape, Move::Roar, body, Some(&nearest), arena, effects);
        }
        let engaged = horizontal_distance(body.feet, nearest.feet) <= ape.s(ape::ENGAGE_RADIUS);
        engaged.then(|| {
            self.target = Some(nearest.id);
            self.ape.unseen = 0;
            nearest
        })
    }

    /// Whether it roared a warning in the last half minute.
    fn warned_lately(&self) -> bool {
        self.ape.last_warning.is_some_and(|at| self.ape.age - at <= ape::WARNING_MEMORY_TICKS)
    }

    /// Closing in and picking moves (`FightGoal`). The move picker works at the design size: a
    /// bigger ape judges distances in proportion to its size.
    fn fight(&mut self, ape: Ape, body: &mut Body, target: &Seen, arena: &impl Arena, walk_out: &mut Option<Walk>, effects: &mut Vec<Effect>) {
        self.look_at(body, target.eyes());
        // It takes nobody on without a warning first (`TerritoryGoal.start`), even whoever it was
        // summoned at or who shot it.
        if !self.warned_lately() {
            self.ape.last_warning = Some(self.ape.age);
            self.start_ape_move(ape, Move::Roar, body, Some(target), arena, effects);
            return;
        }
        if self.ape.age >= self.ape.next_move {
            let ready = |m: Move| slot(m).is_none_or(|i| self.ape.age >= self.ape.ready_at[i]);
            let situation = Situation {
                distance: ape::effective_distance(horizontal_distance(body.feet, target.feet) / ape.scale, target.width / 2.0 / ape.scale),
                dy: (target.feet[1] - body.feet[1]) / ape.scale,
                can_see: target.visible,
                enraged: self.ape.enraged,
                swipe_ready: ready(Move::Swipe),
                slam_ready: ready(Move::Slam),
                boulder_ready: ready(Move::Boulder),
                leap_ready: ready(Move::Leap),
                previous: self.ape.previous,
            };
            let roll = self.unit();
            let next = ape::choose(&situation, roll);
            if next != Move::None {
                self.start_ape_move(ape, next, body, Some(target), arena, effects);
                return;
            }
        }
        if horizontal_distance(body.feet, target.feet) > ape.s(ape::CLOSE_ENOUGH) {
            let speed = if self.ape.enraged { ape::ENRAGED_CHASE_SPEED } else { ape::CHASE_SPEED };
            *walk_out = Some(Walk { to: [target.feet[0], target.feet[2]], speed: speed / ape::CHASE_SPEED });
        }
    }

    /// Idle: every so often it ambles to a spot 16 to 40 blocks off (further for a bigger ape),
    /// and it watches players nearby (`WanderGoal`, `LookAtPlayerGoal`).
    fn amble(&mut self, ape: Ape, body: &mut Body, seen: &[Seen], walk_out: &mut Option<Walk>) {
        let watched = seen
            .iter()
            .filter(|s| s.is_player && s.visible && distance(body.feet, s.feet) <= ape.s(LOOK_RANGE))
            .min_by(|a, b| distance(body.feet, a.feet).total_cmp(&distance(body.feet, b.feet)));
        if let Some(s) = watched {
            self.look_at(body, s.eyes());
        }
        if self.ape.wander.is_none() && self.chance(160) {
            let angle = self.unit() * std::f64::consts::TAU;
            let far = ape.s(16.0 + self.unit() * 24.0);
            let to = [body.feet[0] + angle.cos() * far, body.feet[2] + angle.sin() * far];
            self.ape.wander = Some((to, (600.0 * ape.scale) as u32));
        }
        if let Some((to, left)) = self.ape.wander {
            let arrived = (to[0] - body.feet[0]).hypot(to[1] - body.feet[2]) <= ape.s(3.0);
            self.ape.wander = (left > 0 && !arrived).then_some((to, left - 1));
            if self.ape.wander.is_some() {
                *walk_out = Some(Walk { to, speed: ape::WANDER_SPEED / ape::CHASE_SPEED });
            }
        }
    }

    // -- moves -----------------------------------------------------------------------------------

    /// Starts a move aimed at `target` (or straight ahead), facing it: a roar, a grunt or a cry
    /// as it begins, and where a slam lands or a leap comes down is fixed now.
    fn start_ape_move(&mut self, ape: Ape, movement: Move, body: &mut Body, target: Option<&Seen>, arena: &impl Arena, effects: &mut Vec<Effect>) {
        if self.movement != Move::None {
            return;
        }
        self.movement = movement;
        self.move_ticks = 0;
        self.ape.attack_target = target.map(|t| t.id);
        self.ape.previous = movement;
        if let Some(i) = slot(movement) {
            self.ape.ready_at[i] = self.ape.age + ape::timing(movement).cooldown;
        }
        self.hit.clear();
        self.ape.tracked = None;
        self.ape.wander = None;
        self.move_yaw = target.map_or(body.yaw, |t| yaw_towards(body.feet, t.feet));
        face(body, self.move_yaw);
        match movement {
            Move::Roar => self.ape_sound(ape, "roar", 8.0, 0.85, effects),
            Move::Swipe => self.ape_sound(ape, "grunt", 5.0, 0.95, effects),
            Move::Slam => {
                self.ape_sound(ape, "grunt", 7.0, 0.6, effects);
                self.ape.impact = self.slam_centre(ape, body);
            }
            Move::Boulder => self.ape_sound(ape, "grunt", 6.0, 0.75, effects),
            Move::Leap => {
                self.ape_sound(ape, "leap", 7.0, 0.85, effects);
                let wanted = target.map_or_else(|| ahead(body, self.move_yaw, ape.s(ape::LEAP_MIN + 6.0), 0.0), |t| t.feet);
                self.ape.leap_target = self.leap_target_for(ape, body, arena, wanted);
            }
            _ => {}
        }
    }

    /// One of its sounds: louder (heard further off) and deeper the bigger it is.
    fn ape_sound(&self, ape: Ape, file: &'static str, volume: f32, pitch: f32, effects: &mut Vec<Effect>) {
        let species = self.species;
        effects.push(Effect::Sound { group: species.voice, file, volume: volume * ape.scale as f32, pitch: pitch * species.move_pitch });
    }

    fn tick_ape_move(&mut self, ape: Ape, body: &mut Body, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        self.move_ticks += 1;
        // Whom the move is aimed at, or failing that whom it is after.
        let find = |id: Option<u64>| id.and_then(|id| seen.iter().find(|s| s.id == id)).copied();
        let target = find(self.ape.attack_target).or_else(|| find(self.target));
        match self.movement {
            Move::Roar => self.tick_roar(ape, effects),
            Move::Swipe => self.tick_swipe(ape, body, target, seen, arena, effects),
            Move::Slam => self.tick_slam(ape, body, target, seen, arena, effects),
            Move::Boulder => self.tick_throw(ape, body, target, arena, effects),
            Move::Leap => self.tick_leap(ape, body, seen, arena, effects),
            Move::Land => self.tick_land(ape, seen, effects),
            _ => {}
        }
        face(body, self.move_yaw);
        if self.movement != Move::None && self.move_ticks >= ape::timing(self.movement).duration {
            self.movement = Move::None;
            self.move_ticks = 0;
            self.ape.attack_target = None;
            self.ape.airborne = 0;
            self.ape.next_move = self.ape.age + if self.ape.enraged { ape::REST_TICKS_ENRAGED } else { ape::REST_TICKS };
        }
    }

    /// Rears up and roars, then beats its chest four times; a pending rage takes hold on the
    /// second beat.
    fn tick_roar(&mut self, ape: Ape, effects: &mut Vec<Effect>) {
        if ape::CHEST_BEATS.contains(&self.move_ticks) {
            let pitch = 0.75 + self.unit() as f32 * 0.15;
            self.ape_sound(ape, "chest_beat", 6.0, pitch, effects);
        }
        if self.move_ticks == ape::ENRAGE_TICK && self.ape.pending_enrage {
            self.ape.pending_enrage = false;
            self.ape.enraged = true;
        }
    }

    /// A low backhand along the ground across 150 degrees in front of it: red marks along the
    /// edge of its reach while it draws back, then everything on the ground in the arc is hit.
    fn tick_swipe(&mut self, ape: Ape, body: &Body, target: Option<Seen>, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        let impact = ape::timing(Move::Swipe).impact;
        let t = self.move_ticks;
        if t < impact - 4 {
            self.track(body, target, 8.0);
        }
        if t < impact {
            if t % 3 == 1 {
                self.swipe_warning(ape, body, arena, effects);
            }
            return;
        }
        if t != impact {
            return;
        }
        let pitch = 0.7 + self.unit() as f32 * 0.15;
        self.ape_sound(ape, "swipe", 6.0, pitch, effects);
        let damage = self.move_damage(ape, ape::SWIPE_DAMAGE);
        let reach_dy = ape.s(ape::GROUND_REACH_DY);
        for s in seen {
            let (dx, dz) = (s.feet[0] - body.feet[0], s.feet[2] - body.feet[2]);
            // The arm sweeps along the ground: anything in the air goes over it.
            if (s.feet[1] - body.feet[1]).abs() > reach_dy
                || !ape::in_swipe_arc(self.move_yaw, dx / ape.scale, dz / ape.scale, s.width / 2.0 / ape.scale)
                || !s.on_ground
                || !s.visible
            {
                continue;
            }
            let away = away_from(body.feet, s.feet, facing(self.move_yaw));
            effects.push(Effect::Hit { target: s.id, damage, push: scale(away, 2.0), lift: 0.4, burn_seconds: 0.0, cause: Cause::Swipe });
        }
    }

    /// Red marks along the edge of the swipe's reach, across its whole arc, and down its sides.
    fn swipe_warning(&self, ape: Ape, body: &Body, arena: &impl Arena, effects: &mut Vec<Effect>) {
        let radius = ape.s(ape::SWIPE_REACH - 1.0);
        let points = (36.0 * ape.scale) as u32;
        let half_arc = ape::SWIPE_HALF_ARC as f32;
        for i in 0..=points {
            let yaw = self.move_yaw - half_arc + 2.0 * half_arc * i as f32 / points as f32;
            effects.push(mark(ape, arena, ahead(body, yaw, radius, 0.0)));
        }
        for yaw in [self.move_yaw - half_arc, self.move_yaw + half_arc] {
            let mut r = ape.s(ape::SWIPE_POINT_BLANK);
            while r < radius {
                effects.push(mark(ape, arena, ahead(body, yaw, r, 0.0)));
                r += 2.0;
            }
        }
    }

    /// Both fists raised overhead, then slammed into the ground: a red ring marks how far the
    /// shockwave will roll, and it rolls out along the ground.
    fn tick_slam(&mut self, ape: Ape, body: &Body, target: Option<Seen>, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        let impact = ape::timing(Move::Slam).impact;
        let t = self.move_ticks;
        let radius = ape.s(ape::slam_radius(self.ape.enraged));
        if t <= 8 {
            self.track(body, target, 6.0);
            self.ape.impact = self.slam_centre(ape, body);
        }
        if t < impact {
            if t % 3 == 1 {
                warning_ring(ape, arena, self.ape.impact, radius, effects);
            }
            return;
        }
        let since = t - impact;
        if since == 0 {
            let pitch = 0.7 + self.unit() as f32 * 0.1;
            self.ape_sound(ape, "slam", 10.0, pitch, effects);
        }
        if since <= ape::shockwave_ticks(ape::slam_radius(self.ape.enraged)) {
            let damage = self.move_damage(ape, ape::SLAM_DAMAGE);
            self.shockwave(ape, since, radius, damage, seen, effects);
        }
    }

    /// Rips up a boulder, lifts it overhead and lobs it at the spot its target stood on (and,
    /// enraged, a second where it is heading).
    fn tick_throw(&mut self, ape: Ape, body: &Body, target: Option<Seen>, arena: &impl Arena, effects: &mut Vec<Effect>) {
        let release = ape::timing(Move::Boulder).impact;
        let t = self.move_ticks;
        if t < release - 2 {
            self.track(body, target, 6.0);
        }
        if t == 8 {
            self.ape_sound(ape, "rip", 6.0, 0.85, effects);
        }
        if t == release - 5 {
            self.ape.tracked = target.map(|s| s.feet);
        }
        if t != release {
            return;
        }
        self.ape_sound(ape, "throw", 6.0, 0.85, effects);
        let hands = ahead(body, self.move_yaw, ape.hands.0, ape.hands.1);
        let aim = match target {
            Some(s) => ground_below(ape, arena, s.feet),
            None => ground_below(ape, arena, ahead(body, self.move_yaw, ape.s(ape::BOULDER_MIN + 10.0), ape.s(6.0))),
        };
        self.launch(ape, hands, aim);
        if let Some(s) = target.filter(|_| self.ape.enraged) {
            // A second boulder where it is heading, or beside it if it stands still.
            let flight = f64::from(ape::boulder_flight_ticks(horizontal_distance(hands, aim) / ape.scale));
            let mut lead = self.ape.tracked.map_or([0.0; 3], |was| [(s.feet[0] - was[0]) * flight / 5.0, 0.0, (s.feet[2] - was[2]) * flight / 5.0]);
            let length = lead[0].hypot(lead[2]);
            if length < ape.s(5.0) {
                let side = if self.chance(2) { ape.s(9.0) } else { -ape.s(9.0) };
                let way = ahead(body, self.move_yaw + 90.0, side, 0.0);
                lead = [way[0] - body.feet[0], 0.0, way[2] - body.feet[2]];
            } else if length > ape.s(14.0) {
                lead = scale(lead, ape.s(14.0) / length);
            }
            let second = ground_below(ape, arena, [aim[0] + lead[0], aim[1] + ape.s(6.0), aim[2] + lead[2]]);
            self.launch(ape, hands, second);
        }
    }

    fn launch(&mut self, ape: Ape, from: [f64; 3], to: [f64; 3]) {
        self.ape.next_boulder += 1;
        let mut boulder = Boulder {
            id: self.ape.next_boulder,
            at: from,
            velocity: [0.0; 3],
            landing: to,
            scale: ape.scale,
            damage: self.move_damage(ape, ape::BOULDER_DAMAGE),
            life: 0,
        };
        let ticks = ape::boulder_flight_ticks(horizontal_distance(from, to) / ape.scale);
        boulder.velocity = ape::ballistic_velocity(from, to, ticks, boulder.gravity());
        self.boulders.push(boulder);
    }

    /// Crouches while a red ring marks where it will come down, then leaps there on a fixed
    /// arc; it lands as soon as the arc meets the ground.
    fn tick_leap(&mut self, ape: Ape, body: &mut Body, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        let takeoff = ape::timing(Move::Leap).impact;
        let t = self.move_ticks;
        let radius = ape.s(ape::land_radius(self.ape.enraged));
        if t < takeoff {
            if t % 3 == 1 {
                warning_ring(ape, arena, self.ape.leap_target, radius, effects);
            }
            return;
        }
        let gravity = ape.s(ape::LEAP_GRAVITY);
        if t == takeoff {
            self.ape.leap_flight = ape::leap_flight_ticks(horizontal_distance(body.feet, self.ape.leap_target) / ape.scale);
            self.ape.leap_velocity = ape::ballistic_velocity(body.feet, self.ape.leap_target, self.ape.leap_flight, gravity);
            self.ape.airborne = 0;
            self.ape_sound(ape, "swipe", 6.0, 0.5, effects);
        } else if self.ape.airborne > 3 {
            let ground = ground_under(arena, self.species, body.feet[0], body.feet[2], body.feet[1]);
            let touched_down = ground.is_some_and(|g| body.feet[1] <= g + 0.01);
            if touched_down || self.ape.airborne > self.ape.leap_flight + 20 {
                self.land(ape, body, ground.unwrap_or(body.feet[1]), seen, effects);
                return;
            }
        }
        // The flight is scripted, so it comes down on the marked spot.
        self.ape.airborne += 1;
        let v = self.ape.leap_velocity;
        let fall = gravity * f64::from(self.ape.airborne - 1);
        body.feet = [body.feet[0] + v[0], body.feet[1] + v[1] - fall, body.feet[2] + v[2]];
        if self.ape.airborne.is_multiple_of(3) {
            warning_ring(ape, arena, self.ape.leap_target, radius, effects);
        }
    }

    fn land(&mut self, ape: Ape, body: &mut Body, ground: f64, seen: &[Seen], effects: &mut Vec<Effect>) {
        self.movement = Move::Land;
        self.move_ticks = 0;
        self.ape.airborne = 0;
        self.hit.clear();
        body.feet[1] = ground;
        self.ape.impact = body.feet;
        if let Some(t) = self.ape.attack_target.and_then(|id| seen.iter().find(|s| s.id == id)) {
            self.move_yaw = yaw_towards(body.feet, t.feet);
        }
        self.ape_sound(ape, "slam", 9.0, 0.8, effects);
    }

    /// The touchdown: a smaller shockwave, then a long recovery.
    fn tick_land(&mut self, ape: Ape, seen: &[Seen], effects: &mut Vec<Effect>) {
        let impact = ape::timing(Move::Land).impact;
        let t = self.move_ticks;
        if t >= impact && t - impact <= ape::shockwave_ticks(ape::land_radius(self.ape.enraged)) {
            let radius = ape.s(ape::land_radius(self.ape.enraged));
            let damage = self.move_damage(ape, ape::LAND_DAMAGE);
            self.shockwave(ape, t - impact, radius, damage, seen, effects);
        }
    }

    /// The ground shockwave ring, a tick on: each thing is judged once, as the ring reaches it.
    /// Standing on the ground near the impact's level it is hit; mid-jump, up on a ledge or down
    /// in a pit, the ring passes it by.
    fn shockwave(&mut self, ape: Ape, since: u32, max_radius: f64, centre_damage: f32, seen: &[Seen], effects: &mut Vec<Effect>) {
        let at = self.ape.impact;
        effects.push(Effect::Shockwave { at, radius: ape.s(ape::shockwave_radius(since)).min(max_radius) });
        let reach_dy = ape.s(ape::GROUND_REACH_DY);
        for s in seen {
            if self.hit.contains(&s.id) {
                continue;
            }
            let distance = horizontal_distance(at, s.feet) / ape.scale;
            let half_width = s.width / 2.0 / ape.scale;
            if !ape::in_shockwave_range(distance, half_width, max_radius / ape.scale) || !ape::shockwave_reached(distance, half_width, since) {
                continue;
            }
            self.hit.push(s.id);
            if (s.feet[1] - at[1]).abs() > reach_dy || !s.on_ground {
                continue;
            }
            let away = away_from(at, s.feet, facing(self.move_yaw));
            effects.push(Effect::Hit {
                target: s.id,
                damage: ape::shockwave_damage(distance, max_radius / ape.scale, centre_damage),
                push: scale(away, 1.3),
                lift: 0.4,
                burn_seconds: 0.0,
                cause: Cause::Shockwave,
            });
        }
    }

    // -- boulders --------------------------------------------------------------------------------

    /// Flies its boulders a tick on. Each comes down on its arc; one that meets the ground, or
    /// passes by someone, shatters there.
    pub fn tick_boulders(&mut self, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        let mut boulders = std::mem::take(&mut self.boulders);
        boulders.retain_mut(|b| {
            let v = b.velocity;
            let to = [b.at[0] + v[0], b.at[1] + v[1], b.at[2] + v[2]];
            let ground = arena.clip_ground(b.at, to);
            let end = ground.unwrap_or(to);
            // Whoever its path passes within a hair of, nearest first.
            let struck = seen
                .iter()
                .filter_map(|s| {
                    let m = PROJECTILE_MARGIN;
                    let half = s.width / 2.0 + m;
                    let around = Aabb { min: [s.feet[0] - half, s.feet[1] - m, s.feet[2] - half], max: [s.feet[0] + half, s.feet[1] + s.height + m, s.feet[2] + half] };
                    around.clip(b.at, end).map(|f| (f, s))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((f, s)) = struck {
                let at = [b.at[0] + (end[0] - b.at[0]) * f, b.at[1] + (end[1] - b.at[1]) * f, b.at[2] + (end[2] - b.at[2]) * f];
                self.shatter(b, at, Some(s.id), seen, arena, effects);
                return false;
            }
            if let Some(at) = ground {
                self.shatter(b, at, None, seen, arena, effects);
                return false;
            }
            b.at = to;
            b.velocity[1] -= b.gravity();
            b.life += 1;
            if b.life > ape::BOULDER_MAX_LIFE {
                let at = b.at;
                self.shatter(b, at, None, seen, arena, effects);
                return false;
            }
            true
        });
        self.boulders = boulders;
    }

    /// A boulder shatters: whoever it hit takes the full blow, and everything within its blast
    /// that isn't behind solid cover takes less the further off it is.
    fn shatter(&mut self, b: &Boulder, at: [f64; 3], direct: Option<u64>, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        let pitch = (0.7 + self.unit() as f32 * 0.2) / (b.scale as f32).sqrt();
        effects.push(Effect::Sound { group: "boulder", file: "impact", volume: 8.0 * b.scale as f32, pitch });
        effects.push(Effect::Shockwave { at, radius: b.blast_radius() });
        let origin = [at[0], at[1] + 0.5, at[2]];
        for s in seen {
            let hit_directly = direct == Some(s.id);
            let half = s.width / 2.0;
            let gap = |i: usize, low: f64, high: f64| (low - at[i]).max(at[i] - high).max(0.0);
            let distance = if hit_directly {
                0.0
            } else {
                let g = [gap(0, s.feet[0] - half, s.feet[0] + half), gap(1, s.feet[1], s.feet[1] + s.height), gap(2, s.feet[2] - half, s.feet[2] + half)];
                (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt()
            };
            let centre = [s.feet[0], s.feet[1] + s.height / 2.0, s.feet[2]];
            if !hit_directly && (distance > b.blast_radius() || arena.clip_ground(origin, centre).is_some()) {
                continue;
            }
            effects.push(Effect::Hit {
                target: s.id,
                damage: ape::boulder_damage(distance / b.scale, b.damage),
                push: scale(away_from(at, s.feet, [1.0, 0.0, 0.0]), 1.2),
                lift: 0.4,
                burn_seconds: 0.0,
                cause: Cause::Boulder,
            });
        }
    }

    // -- helpers ---------------------------------------------------------------------------------

    /// A move's damage: harder for a bigger ape, and enraged.
    fn move_damage(&self, ape: Ape, damage: f32) -> f32 {
        (damage + if self.ape.enraged { ape::ENRAGED_BONUS } else { 0.0 }) * ape.damage_scale
    }

    /// While it winds up, it keeps turning (a few degrees a tick at most) to face its target.
    fn track(&mut self, body: &Body, target: Option<Seen>, max_turn: f32) {
        if let Some(t) = target {
            self.move_yaw = approach_degrees(self.move_yaw, yaw_towards(body.feet, t.feet), max_turn);
        }
    }

    fn slam_centre(&self, ape: Ape, body: &Body) -> [f64; 3] {
        ahead(body, self.move_yaw, ape.s(ape::SLAM_FORWARD_OFFSET), 0.0)
    }

    /// Where a leap at `wanted` comes down: there, or as far as it leaps that way, on the ground.
    fn leap_target_for(&self, ape: Ape, body: &Body, arena: &impl Arena, wanted: [f64; 3]) -> [f64; 3] {
        let (dx, dz) = (wanted[0] - body.feet[0], wanted[2] - body.feet[2]);
        let far = dx.hypot(dz);
        let most = ape.s(ape::LEAP_MAX);
        let goal = if far > most {
            [body.feet[0] + dx * most / far, wanted[1] + ape.s(6.0), body.feet[2] + dz * most / far]
        } else {
            wanted
        };
        ground_below(ape, arena, goal)
    }

}

/// A ring of red marks on the ground: where a move is about to hit.
fn warning_ring(ape: Ape, arena: &impl Arena, centre: [f64; 3], radius: f64, effects: &mut Vec<Effect>) {
    let points = ((radius * 3.5) as u32).clamp(18, 180);
    for i in 0..points {
        let angle = std::f64::consts::TAU * f64::from(i) / f64::from(points);
        effects.push(mark(ape, arena, [centre[0] + angle.cos() * radius, centre[1], centre[2] + angle.sin() * radius]));
    }
    effects.push(mark(ape, arena, centre));
}

/// A red mark on the ground near `at` (foliage ignored).
fn mark(ape: Ape, arena: &impl Arena, at: [f64; 3]) -> Effect {
    let y = arena.ground_at(at[0], at[2], at[1], ape.s(4.0).ceil() as i32, ape.s(8.0).ceil() as i32).unwrap_or(at[1]);
    Effect::Warning { at: [at[0], y, at[2]] }
}

/// The ground an ape of `species` stands on at (x, z) (`KongTerrain.groundUnder`): the median
/// of the ground under its middle and four points round it, so broad terrain (hills, big
/// buildings) lifts it but a lone pillar or a hole doesn't. None if none of them found ground.
pub fn ground_under(arena: &impl Arena, species: &Species, x: f64, z: f64, near_y: f64) -> Option<f64> {
    let ape = species.ape?;
    let r = species.width * 0.35;
    let below = ape.s(ape::SCAN_BELOW).ceil() as i32;
    let mut samples = [(x, z), (x + r, z), (x - r, z), (x, z + r), (x, z - r)]
        .map(|(x, z)| arena.ground_at(x, z, near_y, species.step_height, below).unwrap_or(f64::NEG_INFINITY));
    samples.sort_by(f64::total_cmp);
    Some(samples[2]).filter(|y| y.is_finite())
}

/// The ground under a point, a few blocks up or down (foliage ignored), or the point itself.
fn ground_below(ape: Ape, arena: &impl Arena, at: [f64; 3]) -> [f64; 3] {
    let y = arena.ground_at(at[0], at[2], at[1], ape.s(1.0).ceil() as i32, ape.s(16.0).ceil() as i32);
    [at[0], y.unwrap_or(at[1]), at[2]]
}

/// A point `forward` ahead of it along `yaw` and `up` above its feet.
fn ahead(body: &Body, yaw: f32, forward: f64, up: f64) -> [f64; 3] {
    let r = f64::from(yaw).to_radians();
    [body.feet[0] - r.sin() * forward, body.feet[1] + up, body.feet[2] + r.cos() * forward]
}

/// The way `yaw` faces, flat.
fn facing(yaw: f32) -> [f64; 3] {
    let r = f64::from(yaw).to_radians();
    [-r.sin(), 0.0, r.cos()]
}

fn face(body: &mut Body, yaw: f32) {
    body.yaw = yaw;
    body.head_yaw = yaw;
}

fn yaw_towards(from: [f64; 3], to: [f64; 3]) -> f32 {
    ((to[2] - from[2]).atan2(to[0] - from[0]).to_degrees() - 90.0) as f32
}

fn horizontal_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (b[0] - a[0]).hypot(b[2] - a[2])
}

/// The horizontal way from `from` to `to`, or `fallback` where they meet.
fn away_from(from: [f64; 3], to: [f64; 3], fallback: [f64; 3]) -> [f64; 3] {
    if horizontal_distance(from, to) < 1e-2 { normalize(fallback) } else { horizontal_from(from, to) }
}
