//! A kaiju's behaviour, one tick at a time, independent of the engine that hosts it.
//!
//! The host owns its body (position, physics, health) and the world; each tick it hands the
//! brain what it can see, and the brain turns it, says where to walk, and lists what happens:
//! hits, sounds, blasts, blocks to crush. No pathfinding: no path is ever that wide. It walks
//! straight at things, turning slowly, crushing whatever is in the way. What kind of kaiju it is
//! (its size, moves and voice) comes from its [`Species`]; the apes behave in their own way
//! (`brain/ape.rs`).

mod ape;

pub use ape::ground_under;

use crate::anim::Move;
use crate::combat::{GROUND_REACH_DY, Tail};
use crate::godzilla::breath;
use crate::species::Species;

/// Standing its ground, it turns on the spot to face what it fights once that is this far off
/// its nose (radians).
const TRACK_ANGLE: f64 = 20.0 * std::f64::consts::PI / 180.0;

/// Something it can see: a player, a mob, anything alive.
#[derive(Clone, Copy, Debug)]
pub struct Seen {
    pub id: u64,
    pub feet: [f64; 3],
    pub width: f64,
    pub height: f64,
    pub on_ground: bool,
    pub is_player: bool,
    /// Whether it has line of sight to it.
    pub visible: bool,
    /// Whether it picks it out on sight: not a player in creative mode, whom it only goes for
    /// once it hurts it.
    pub targetable: bool,
}

impl Seen {
    pub fn eyes(&self) -> [f64; 3] {
        [self.feet[0], self.feet[1] + self.height * 0.85, self.feet[2]]
    }
}

/// Its body as the host simulates it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Body {
    pub feet: [f64; 3],
    /// Minecraft yaw, degrees: 0 faces +Z, 90 faces -X.
    pub yaw: f32,
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub moving: bool,
    pub blocked: bool,
}

/// World questions the brain asks.
pub trait Arena {
    /// Where a line from `from` to `to` first hits a solid block, if it does.
    fn clip_blocks(&self, from: [f64; 3], to: [f64; 3]) -> Option<[f64; 3]>;
    /// The top of the ground near `near_y` at (x, z).
    fn surface_y(&self, x: f64, z: f64, near_y: f64) -> f64;
    /// As an ape sees the world, where trees and foliage are something to wade through, see past
    /// and throw through (`KongTerrain`): where a line first hits the ground (anything solid but
    /// foliage)...
    fn clip_ground(&self, from: [f64; 3], to: [f64; 3]) -> Option<[f64; 3]>;
    /// ...and the top of the highest ground at (x, z), from `above` blocks over `near_y` down to
    /// `below` blocks under it.
    fn ground_at(&self, x: f64, z: f64, near_y: f64, above: i32, below: i32) -> Option<f64>;
}

/// What happened this tick, for the host to carry out.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Hurt `target`, push it along `push` (blocks per tick, already scaled), and maybe set it alight.
    Hit { target: u64, damage: f32, push: [f64; 3], lift: f64, burn_seconds: f32, cause: Cause },
    /// `zillacraft:entity.<group>.<file>`.
    Sound { group: &'static str, file: &'static str, volume: f32, pitch: f32 },
    /// An explosion that breaks blocks and hurts what it reaches (never a kaiju).
    Blast { at: [f64; 3], power: f32 },
    /// Break crushable blocks in this box, up to `budget`, no harder than `max_hardness`.
    Crush { min: [f64; 3], max: [f64; 3], budget: u32, max_hardness: f32 },
    /// Red warning dust on the ground.
    Warning { at: [f64; 3] },
    /// The atomic breath's beam this tick, from his mouth to where it ends.
    Beam { from: [f64; 3], to: [f64; 3] },
    /// Blue fire gathering at his mouth while he charges.
    Charge { at: [f64; 3], strength: f32 },
    /// The stomp's shockwave ring this tick.
    Shockwave { at: [f64; 3], radius: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Claws,
    Tail,
    Stomp,
    Bite,
    AtomicBreath,
    /// The apes': the backhand, a slam's or a landing's shockwave, a thrown boulder.
    Swipe,
    Shockwave,
    Boulder,
}

/// Where the host should walk it this tick: towards `to` at `speed` times its walking speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walk {
    pub to: [f64; 2],
    pub speed: f64,
}

#[derive(Debug)]
pub struct Brain {
    pub species: &'static Species,
    pub movement: Move,
    pub move_ticks: u32,
    pub move_direction: i32,
    move_yaw: f32,
    tail_cooldown: u32,
    stomp_cooldown: u32,
    bite_cooldown: u32,
    roar_cooldown: u32,
    recovery: u32,
    hit: Vec<u64>,
    foot: [f64; 3],
    /// What it last roared at (it roars when it first sets eyes on something).
    roared_at: Option<u64>,
    /// Ticks since he started the breath (0 when he isn't breathing).
    pub breath_ticks: u32,
    breath_cooldown: u32,
    /// 0..1, the jaw opening for the breath.
    pub breath_amount: f32,
    pub breath_amount_previous: f32,
    /// Who the beam has scorched lately, and on which tick of the breath.
    breath_hits: Vec<(u64, u32)>,
    claw_cooldown: u32,
    wander: Option<([f64; 2], u32)>,
    /// Walking away after a kill: where to, and for how many more ticks.
    retreat: Option<([f64; 2], u32)>,
    pub target: Option<u64>,
    /// The target hurt it: it keeps after it even if it isn't fair game on sight.
    provoked: bool,
    rng: u64,
    /// An ape's own state.
    ape: ape::ApeState,
    /// Boulders an ape has thrown, still in the air.
    pub boulders: Vec<crate::ape::Boulder>,
}

impl Brain {
    pub fn new(seed: u64, species: &'static Species) -> Self {
        Self {
            species,
            movement: Move::None,
            move_ticks: 0,
            move_direction: 1,
            move_yaw: 0.0,
            tail_cooldown: 0,
            stomp_cooldown: 0,
            bite_cooldown: 0,
            roar_cooldown: 0,
            recovery: 0,
            hit: Vec::new(),
            foot: [0.0; 3],
            roared_at: None,
            breath_ticks: 0,
            breath_cooldown: breath::FIRST_COOLDOWN,
            breath_amount: 0.0,
            breath_amount_previous: 0.0,
            breath_hits: Vec::new(),
            claw_cooldown: 0,
            wander: None,
            retreat: None,
            target: None,
            provoked: false,
            rng: seed | 1,
            ape: ape::ApeState::default(),
            boulders: Vec::new(),
        }
    }

    fn random(&mut self) -> u64 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u32) -> u32 {
        (self.random() % u64::from(n.max(1))) as u32
    }

    fn unit(&mut self) -> f64 {
        (self.random() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// One chance in `n`, as `Random.nextInt(n) == 0`.
    pub fn chance(&mut self, n: u32) -> bool {
        self.below(n) == 0
    }

    pub fn voice_pitch(&mut self) -> f32 {
        0.95 + self.unit() as f32 * 0.1
    }

    /// Whether the breath is under way (charging or firing).
    pub fn breathing(&self) -> bool {
        self.breath_ticks > 0
    }

    /// One tick. `seen` is everything alive around it (it picks its own target), `walk_out`
    /// receives where to walk, and `effects` what happens.
    pub fn tick(&mut self, body: &mut Body, seen: &[Seen], arena: &impl Arena, walk_out: &mut Option<Walk>, effects: &mut Vec<Effect>) {
        *walk_out = None;
        if let Some(ape) = self.species.ape {
            self.tick_ape(ape, body, seen, arena, walk_out, effects);
            return;
        }
        self.claw_cooldown = self.claw_cooldown.saturating_sub(1);
        self.tail_cooldown = self.tail_cooldown.saturating_sub(1);
        self.stomp_cooldown = self.stomp_cooldown.saturating_sub(1);
        self.bite_cooldown = self.bite_cooldown.saturating_sub(1);
        self.roar_cooldown = self.roar_cooldown.saturating_sub(1);
        if self.movement == Move::None {
            self.recovery = self.recovery.saturating_sub(1);
        }
        if self.breath_ticks == 0 {
            self.breath_cooldown = self.breath_cooldown.saturating_sub(1);
        }
        self.breath_amount_previous = self.breath_amount;
        self.breath_amount = (self.breath_amount + if self.breath_ticks > 0 { 0.12 } else { -0.12 }).clamp(0.0, 1.0);

        let target = if let Some((to, left)) = self.retreat {
            let arrived = (to[0] - body.feet[0]).hypot(to[1] - body.feet[2]) <= 6.0;
            self.retreat = (left > 0 && !arrived).then_some((to, left - 1));
            None
        } else {
            self.pick_target(body, seen)
        };

        if self.movement != Move::None {
            // Planted: it doesn't turn during a move.
            body.yaw = self.move_yaw;
            body.head_yaw = self.move_yaw;
            self.tick_move(body, seen, arena, effects);
        } else if self.breath_ticks > 0 {
            self.tick_breath(body, target, seen, arena, effects);
        } else if let Some(t) = target {
            if !self.start_move(body, &t, effects) && !self.start_breath(body, &t, effects) {
                self.charge(body, &t, walk_out, effects);
            }
        } else if let Some((to, _)) = self.retreat {
            *walk_out = Some(Walk { to, speed: 1.0 });
        } else {
            self.wander_tick(body, walk_out);
        }

        // The rampage: it pulverizes what it wades through, across its whole height.
        let tail_sweeping = self.movement == Move::TailSwipe && self.species.tail.is_some_and(|tail| tail.sweeping(self.move_ticks));
        if body.moving || body.blocked || tail_sweeping {
            self.crush(body, effects);
        }
    }

    fn pick_target(&mut self, body: &Body, seen: &[Seen]) -> Option<Seen> {
        // Keep the current target while it's in range (one not fair game on sight only if it
        // hurt it); otherwise the nearest visible player.
        let range = self.species.follow_range;
        if let Some(id) = self.target
            && let Some(t) = seen.iter().find(|s| s.id == id)
            && distance(body.feet, t.feet) <= range * 1.5
            && (t.targetable || self.provoked)
        {
            return Some(*t);
        }
        let nearest = seen
            .iter()
            .filter(|s| s.visible && s.is_player && s.targetable && distance(body.feet, s.feet) <= range)
            .min_by(|a, b| distance(body.feet, a.feet).total_cmp(&distance(body.feet, b.feet)))
            .copied();
        self.target = nearest.map(|t| t.id);
        self.provoked = false;
        nearest
    }

    /// Something that hurt it becomes its target, even while it's walking away.
    pub fn provoked_by(&mut self, attacker: u64) {
        self.retreat = None;
        self.target = Some(attacker);
        self.provoked = true;
    }

    /// It was hurt down to `health_share` of its health: an ape below half is enraged as soon as
    /// it is free to roar.
    pub fn wounded(&mut self, health_share: f32) {
        if self.species.ape.is_some() && !self.ape.enraged && health_share <= 0.5 {
            self.ape.pending_enrage = true;
        }
    }

    /// It comes for `player` (whoever summoned it), if it is fair game.
    pub fn aggro(&mut self, player: u64) {
        self.target = Some(player);
    }

    /// Its target is dead: it lets it be and lumbers off towards `to` for up to `ticks`.
    pub fn retreat(&mut self, to: [f64; 2], ticks: u32) {
        self.target = None;
        if self.breath_ticks > 0 {
            self.end_breath();
        }
        self.retreat = Some((to, ticks));
    }

    pub fn retreating(&self) -> bool {
        self.retreat.is_some()
    }

    // -- walking ---------------------------------------------------------------------------------

    fn charge(&mut self, body: &mut Body, target: &Seen, walk_out: &mut Option<Walk>, effects: &mut Vec<Effect>) {
        self.look_at(body, target.eyes());
        let to = [target.feet[0], target.feet[2]];
        if self.holds_ground(body, target) {
            // Close enough: its moves do the rest, as it turns on the spot to keep facing its
            // target while that circles round it.
            let (forward, side) = to_local(body, target.feet, body.yaw);
            if side.atan2(forward).abs() > TRACK_ANGLE {
                self.turn_towards(body, to);
            }
        } else if self.turns_to_face(body, target) {
            self.turn_towards(body, to);
        } else {
            *walk_out = Some(Walk { to, speed: 1.0 });
        }
        if self.claw_cooldown == 0 && self.in_reach(body, target) {
            effects.push(Effect::Hit {
                target: target.id,
                damage: self.species.claw_damage,
                push: scale(horizontal_from(body.feet, target.feet), self.species.claw_knockback),
                lift: 0.4,
                burn_seconds: 0.0,
                cause: Cause::Claws,
            });
            self.claw_cooldown = 20;
        }
    }

    /// Whether it stops walking at its target: one with a bite stops once that is in front of
    /// its jaws, and lets the bite do the rest.
    fn holds_ground(&self, body: &Body, target: &Seen) -> bool {
        let Some(bite) = self.species.bite else { return false };
        let (forward, side) = to_local(body, target.feet, body.yaw);
        bite.worth_it(forward, side, target.width / 2.0, target.feet[1] - body.feet[1])
    }

    /// Whether, rather than step up to (and onto) its target, it turns on the spot to bring it
    /// in front of its jaws: it is within reach of them, but off to one side.
    fn turns_to_face(&self, body: &Body, target: &Seen) -> bool {
        let Some(bite) = self.species.bite else { return false };
        let (forward, side) = to_local(body, target.feet, body.yaw);
        let (reach, half_width) = (forward.hypot(side), target.width / 2.0);
        reach - half_width <= bite.max_forward - 1.0
            && reach + half_width >= bite.min_forward + 0.5
            && (target.feet[1] - body.feet[1]).abs() <= bite.max_height - 1.0
    }

    fn wander_tick(&mut self, body: &mut Body, walk_out: &mut Option<Walk>) {
        if self.wander.is_none() && self.below(140) == 0 {
            let yaw = self.unit() * std::f64::consts::TAU;
            self.wander = Some(([body.feet[0] - yaw.sin() * 30.0, body.feet[2] + yaw.cos() * 30.0], 0));
        }
        if let Some((to, ticks)) = self.wander {
            let near = (to[0] - body.feet[0]).hypot(to[1] - body.feet[2]) <= 6.0;
            if ticks >= 220 || near {
                self.wander = None;
            } else {
                self.wander = Some((to, ticks + 1));
                *walk_out = Some(Walk { to, speed: 1.0 });
            }
        }
    }

    /// Turns it towards a point at its turn rate: ships, not zombies.
    pub fn turn_towards(&self, body: &mut Body, to: [f64; 2]) {
        let wanted = ((to[1] - body.feet[2]).atan2(to[0] - body.feet[0]).to_degrees() - 90.0) as f32;
        body.yaw = approach_degrees(body.yaw, wanted, self.species.turn_rate);
    }

    fn look_at(&self, body: &mut Body, eyes: [f64; 3]) {
        let eye_y = body.feet[1] + self.species.eye_height;
        let dx = eyes[0] - body.feet[0];
        let dz = eyes[2] - body.feet[2];
        let wanted_yaw = (dz.atan2(dx).to_degrees() - 90.0) as f32;
        body.head_yaw = approach_degrees(body.head_yaw, wanted_yaw, 10.0);
        let pitch = -((eyes[1] - eye_y).atan2(dx.hypot(dz)).to_degrees()) as f32;
        body.head_pitch += (pitch - body.head_pitch).clamp(-10.0, 10.0);
    }

    fn in_reach(&self, body: &Body, target: &Seen) -> bool {
        let s = self.species;
        let reach = s.width / 2.0 + s.claw_reach + target.width / 2.0;
        (target.feet[0] - body.feet[0]).abs() <= reach
            && (target.feet[2] - body.feet[2]).abs() <= reach
            && target.feet[1] < body.feet[1] + s.column_height + s.claw_reach
            && target.feet[1] + target.height > body.feet[1] - s.claw_reach
    }

    // -- close-range moves -----------------------------------------------------------------------

    /// The roar when it first sets eyes on something, the tail swipe for whatever is beside or
    /// behind it, the bite for whatever is in front of its jaws, and the stomp for whatever is
    /// close: whichever is ready and worth it, in that order.
    fn start_move(&mut self, body: &Body, target: &Seen, effects: &mut Vec<Effect>) -> bool {
        if self.recovery > 0 {
            return false;
        }
        let species = self.species;
        let (forward, side) = to_local(body, target.feet, body.yaw);
        let half_width = target.width / 2.0;
        let dy = target.feet[1] - body.feet[1];
        let top = target.feet[1] + target.height - body.feet[1];
        let swipe_at = species.tail.map(|tail| (tail, tail.bearing_from_pivot(forward, side)));
        let choice = if species.roar_ticks > 0 && self.roar_cooldown == 0 && self.roared_at != Some(target.id) && target.visible {
            Some((Move::Roar, 1))
        } else if let Some((_, bearing)) = swipe_at.filter(|&(tail, bearing)| {
            self.tail_cooldown == 0
                && dy >= -GROUND_REACH_DY
                && tail.worth_swiping(bearing, tail.distance_from_pivot(forward, side), half_width, dy, top)
        }) {
            Some((Move::TailSwipe, Tail::direction_for(bearing)))
        } else if self.bite_cooldown == 0 && species.bite.is_some_and(|bite| bite.worth_it(forward, side, half_width, dy)) {
            // It flings its catch to whichever side.
            Some((Move::Bite, if self.chance(2) { 1 } else { -1 }))
        } else if self.stomp_cooldown == 0
            && dy.abs() <= GROUND_REACH_DY
            && species.stomp.is_some_and(|stomp| stomp.worth_it(forward.hypot(side), half_width))
        {
            Some((Move::Stomp, if side >= 0.0 { 1 } else { -1 }))
        } else {
            None
        };
        let Some((movement, direction)) = choice else {
            return false;
        };
        self.movement = movement;
        self.move_direction = direction;
        self.move_ticks = 0;
        self.move_yaw = body.yaw;
        self.hit.clear();
        if let Some(stomp) = species.stomp {
            let (fx, fz) = to_world(body, stomp.foot_forward, f64::from(direction) * stomp.foot_side, self.move_yaw);
            self.foot = [fx, body.feet[1], fz];
        }
        let pitch = self.voice_pitch();
        if movement == Move::Roar {
            self.roared_at = Some(target.id);
            effects.push(Effect::Sound { group: species.voice, file: "roar", volume: species.move_volume * 1.5, pitch });
        } else {
            effects.push(Effect::Sound { group: species.voice, file: "growl", volume: species.move_volume, pitch });
        }
        true
    }

    fn tick_move(&mut self, body: &Body, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        self.move_ticks += 1;
        let species = self.species;
        let total = match self.movement {
            Move::TailSwipe => species.tail.map_or(0, |tail| tail.total()),
            Move::Stomp => species.stomp.map_or(0, |stomp| stomp.total),
            Move::Bite => species.bite.map_or(0, |bite| bite.total()),
            Move::Roar => species.roar_ticks,
            _ => 0,
        };
        match self.movement {
            Move::TailSwipe => self.tick_tail(body, seen, arena, effects),
            Move::Stomp => self.tick_stomp(body, seen, arena, effects),
            Move::Bite => self.tick_bite(body, seen, arena, effects),
            _ => {}
        }
        if self.move_ticks >= total {
            match self.movement {
                Move::TailSwipe => self.tail_cooldown = self.cooldown(species.tail_cooldown),
                Move::Stomp => self.stomp_cooldown = self.cooldown(species.stomp_cooldown),
                Move::Bite => self.bite_cooldown = self.cooldown(species.bite_cooldown),
                Move::Roar => self.roar_cooldown = species.roar_cooldown,
                _ => {}
            }
            // A roar is only a warning: it may strike straight after one.
            self.recovery = if self.movement == Move::Roar { 0 } else { species.move_recovery };
            self.movement = Move::None;
            self.move_ticks = 0;
        }
    }

    /// A move's cooldown: its base and up to its random extra.
    fn cooldown(&mut self, (base, extra): (u32, u32)) -> u32 {
        base + self.below(extra)
    }

    /// Warning dust on the ground at a point {forward, side} from its centre.
    fn warn(body: &Body, arena: &impl Arena, yaw: f32, forward: f64, side: f64) -> Effect {
        let (x, z) = to_world(body, forward, side, yaw);
        Effect::Warning { at: [x, arena.surface_y(x, z, body.feet[1]), z] }
    }

    fn tick_tail(&mut self, body: &Body, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        let species = self.species;
        let Some(tail) = species.tail else { return };
        let t = self.move_ticks;
        let yaw = self.move_yaw;
        if t < tail.windup {
            if t % 4 == 1 {
                // The far edge of the ground the tail will sweep.
                let reach = tail.reach();
                let points = 16.max((reach * 1.2) as u32);
                for i in 0..=points {
                    let bearing = -tail.arc + 2.0 * tail.arc * f64::from(i) / f64::from(points);
                    effects.push(Self::warn(body, arena, yaw, -tail.pivot_back - bearing.cos() * reach, bearing.sin() * reach));
                }
            }
            return;
        }
        if t == tail.windup + 1 {
            effects.push(Effect::Sound { group: "kaiju", file: "tail_swipe", volume: species.move_volume, pitch: 0.85 * species.move_pitch });
        }
        if !tail.sweeping(t) {
            return;
        }
        let yaw_rad = f64::from(yaw).to_radians();
        let forward_dir = [-yaw_rad.sin(), 0.0, yaw_rad.cos()];
        let left = [forward_dir[2], 0.0, -forward_dir[0]];
        for s in seen {
            if self.hit.contains(&s.id) {
                continue;
            }
            let (forward, side) = to_local(body, s.feet, yaw);
            let bearing = tail.bearing_from_pivot(forward, side);
            let distance = tail.distance_from_pivot(forward, side);
            let bottom = s.feet[1] - body.feet[1];
            let top = bottom + s.height;
            if tail.hits(t, self.move_direction, bearing, distance, s.width / 2.0, bottom, top) {
                self.hit.push(s.id);
                let d = f64::from(self.move_direction);
                let push = [
                    forward_dir[0] * d * bearing.sin() + left[0] * d * bearing.cos(),
                    0.0,
                    forward_dir[2] * d * bearing.sin() + left[2] * d * bearing.cos(),
                ];
                effects.push(Effect::Hit {
                    target: s.id,
                    damage: tail.damage,
                    push: scale(push, tail.knockback),
                    lift: tail.lift,
                    burn_seconds: 0.0,
                    cause: Cause::Tail,
                });
            }
        }
    }

    fn tick_stomp(&mut self, body: &Body, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        let species = self.species;
        let Some(stomp) = species.stomp else { return };
        let t = self.move_ticks;
        if t < stomp.windup {
            if t % 3 == 1 {
                // The ring the shockwave will reach, and a mark where the foot comes down.
                let points = 24.max((stomp.radius * 3.0) as u32);
                for i in 0..points {
                    let a = std::f64::consts::TAU * f64::from(i) / f64::from(points);
                    let (x, z) = (self.foot[0] + a.cos() * stomp.radius, self.foot[2] + a.sin() * stomp.radius);
                    effects.push(Effect::Warning { at: [x, arena.surface_y(x, z, body.feet[1]), z] });
                }
                effects.push(Effect::Warning { at: self.foot });
            }
            return;
        }
        let since = t - stomp.windup;
        if since == 0 {
            effects.push(Effect::Sound { group: "kaiju", file: "stomp", volume: species.move_volume * 1.5, pitch: 0.9 * species.move_pitch });
        }
        if since > stomp.wave_ticks() {
            return;
        }
        effects.push(Effect::Shockwave { at: self.foot, radius: stomp.wave_radius(since) });
        for s in seen {
            if self.hit.contains(&s.id) {
                continue;
            }
            let (dx, dz) = (s.feet[0] - self.foot[0], s.feet[2] - self.foot[2]);
            let distance = dx.hypot(dz);
            if !stomp.reaches(distance, s.width / 2.0, since) {
                continue;
            }
            // Each thing is judged once, as the wave reaches it: only what stands on the ground at its level.
            self.hit.push(s.id);
            if s.on_ground && (s.feet[1] - self.foot[1]).abs() <= GROUND_REACH_DY {
                let away = if distance > 1e-3 { [dx / distance, 0.0, dz / distance] } else { [0.0; 3] };
                effects.push(Effect::Hit {
                    target: s.id,
                    damage: stomp.damage_at((distance - s.width / 2.0).max(0.0)),
                    push: scale(away, stomp.knockback),
                    lift: stomp.lift,
                    burn_seconds: 0.0,
                    cause: Cause::Stomp,
                });
            }
        }
    }

    /// The bite: it rears back while red dust marks the ground its jaws will close over, then
    /// lunges and snaps, flinging what it catches aside (and a little onwards).
    fn tick_bite(&mut self, body: &Body, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        let species = self.species;
        let Some(bite) = species.bite else { return };
        let t = self.move_ticks;
        let yaw = self.move_yaw;
        if t < bite.windup {
            if t % 3 == 1 {
                let (near, far, half) = (bite.min_forward, bite.max_forward, bite.half_width);
                let mut forward = near;
                while forward <= far + 1e-6 {
                    effects.push(Self::warn(body, arena, yaw, forward, -half));
                    effects.push(Self::warn(body, arena, yaw, forward, half));
                    forward += 1.5;
                }
                let mut side = -half;
                while side <= half + 1e-6 {
                    effects.push(Self::warn(body, arena, yaw, near, side));
                    effects.push(Self::warn(body, arena, yaw, far, side));
                    side += 1.5;
                }
            }
            return;
        }
        if t == bite.windup + 1 {
            effects.push(Effect::Sound { group: "kaiju", file: "bite", volume: species.move_volume, pitch: 0.9 * species.move_pitch });
        }
        if !bite.snapping(t) {
            return;
        }
        let yaw_rad = f64::from(yaw).to_radians();
        let forward_dir = [-yaw_rad.sin(), 0.0, yaw_rad.cos()];
        let left = [forward_dir[2], 0.0, -forward_dir[0]];
        let d = f64::from(self.move_direction);
        let fling = normalize([left[0] * d + forward_dir[0] * 0.35, 0.0, left[2] * d + forward_dir[2] * 0.35]);
        for s in seen {
            if self.hit.contains(&s.id) {
                continue;
            }
            let (forward, side) = to_local(body, s.feet, yaw);
            let bottom = s.feet[1] - body.feet[1];
            if bite.hits(t, forward, side, s.width / 2.0, bottom, bottom + s.height) {
                self.hit.push(s.id);
                effects.push(Effect::Hit {
                    target: s.id,
                    damage: bite.damage,
                    push: scale(fling, bite.fling),
                    lift: bite.fling_lift,
                    burn_seconds: 0.0,
                    cause: Cause::Bite,
                });
            }
        }
    }

    // -- atomic breath ---------------------------------------------------------------------------

    fn mouth(body: &Body) -> [f64; 3] {
        let yaw = f64::from(body.head_yaw).to_radians();
        [body.feet[0] - yaw.sin() * breath::MOUTH_FORWARD, body.feet[1] + breath::MOUTH_UP, body.feet[2] + yaw.cos() * breath::MOUTH_FORWARD]
    }

    fn start_breath(&mut self, body: &Body, target: &Seen, effects: &mut Vec<Effect>) -> bool {
        if !self.species.breathes || self.breath_cooldown > 0 || !target.visible {
            return false;
        }
        let d = distance(body.feet, target.feet);
        let off_nose = {
            let (forward, side) = to_local(body, target.feet, body.yaw);
            side.atan2(forward).abs()
        };
        if !((d > breath::MIN_DIST || target.height >= breath::GIANT_HEIGHT) && d < breath::MAX_RANGE && off_nose <= breath::MAX_OFF_NOSE) {
            return false;
        }
        self.breath_ticks = 1;
        let pitch = self.voice_pitch();
        effects.push(Effect::Sound { group: "godzilla", file: "growl", volume: 6.0, pitch });
        effects.push(Effect::Sound { group: "godzilla", file: "breath_charge", volume: 6.0, pitch: 1.0 });
        true
    }

    fn tick_breath(&mut self, body: &mut Body, target: Option<Seen>, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        let Some(target) = target.filter(|t| distance(body.feet, t.feet) < breath::MAX_RANGE * 1.22) else {
            self.end_breath();
            return;
        };
        self.breath_ticks += 1;
        if self.breath_ticks > breath::TOTAL_TICKS {
            self.end_breath();
            return;
        }
        self.look_at(body, target.eyes());
        self.turn_towards(body, [target.feet[0], target.feet[2]]);
        let mouth = Self::mouth(body);
        let aim = normalize(sub(target.eyes(), mouth));
        if self.breath_ticks < breath::WINDUP_TICKS {
            effects.push(Effect::Charge { at: mouth, strength: self.breath_ticks as f32 / breath::WINDUP_TICKS as f32 });
            return;
        }
        if self.breath_ticks == breath::WINDUP_TICKS {
            effects.push(Effect::Sound { group: "godzilla", file: "breath_fire", volume: 8.0, pitch: 1.0 });
        }
        let far = add(mouth, scale(aim, breath::MAX_RANGE));
        let hit_block = arena.clip_blocks(mouth, far);
        let end = hit_block.unwrap_or(far);
        effects.push(Effect::Beam { from: mouth, to: end });

        // Scorch whatever the beam touches (each at most every half second), and throw it back
        // the way the beam goes, flat so it isn't driven into the ground.
        let away = normalize([aim[0], 0.0, aim[2]]);
        let radius = breath::BEAM_RADIUS.max(3.5);
        let now = self.breath_ticks;
        self.breath_hits.retain(|&(_, at)| now - at < 10);
        for s in seen {
            if self.breath_hits.iter().any(|&(id, _)| id == s.id) || !segment_hits_body(mouth, end, s, radius) {
                continue;
            }
            self.breath_hits.push((s.id, now));
            effects.push(Effect::Hit {
                target: s.id,
                damage: breath::DAMAGE,
                push: scale(away, breath::KNOCKBACK),
                lift: breath::KNOCKBACK_LIFT,
                burn_seconds: breath::BURN_SECONDS,
                cause: Cause::AtomicBreath,
            });
        }
        // Where it strikes the ground the earth blows apart, over and over as it sweeps.
        if hit_block.is_some() && self.breath_ticks.is_multiple_of(breath::BLAST_INTERVAL) {
            effects.push(Effect::Blast { at: end, power: breath::BLAST_POWER });
        }
    }

    fn end_breath(&mut self) {
        self.breath_ticks = 0;
        self.breath_hits.clear();
        self.breath_cooldown = breath::COOLDOWN.0 + self.below(breath::COOLDOWN.1);
    }

    // -- rampage ---------------------------------------------------------------------------------

    fn crush(&self, body: &Body, effects: &mut Vec<Effect>) {
        let s = self.species;
        let half = s.width / 2.0 + 0.5;
        // Never below its feet, or it digs itself a pit; up its full height so trees never poke through.
        let feet_y = (body.feet[1] + 0.001).floor();
        effects.push(Effect::Crush {
            min: [body.feet[0] - half, feet_y, body.feet[2] - half],
            max: [body.feet[0] + half, body.feet[1] + s.column_height + 0.5, body.feet[2] + half],
            budget: s.crush_budget,
            max_hardness: s.crush_hardness,
        });
        // Its legs and the tail carve through terrain too, never below its ground plane.
        let Some(tail) = s.tail else { return };
        let tail_angle = if self.movement == Move::TailSwipe { tail.angle(f64::from(self.move_ticks), self.move_direction) } else { 0.0 };
        for (i, seg) in tail.segments.iter().enumerate() {
            let (mut forward, mut side) = (seg.forward, seg.side);
            if tail_angle != 0.0 {
                (forward, side) = tail.swing_point(forward, side, tail_angle);
            }
            let (x, z) = to_world(body, forward, side, body.yaw);
            let half = seg.width / 2.0 + 0.25;
            let y = body.feet[1] + seg.up;
            effects.push(Effect::Crush {
                min: [x - half, (y - 0.25).max(feet_y), z - half],
                max: [x + half, (y + seg.height + 0.25).max(feet_y), z + half],
                budget: s.crush_budget / (i as u32 + 2),
                max_hardness: s.crush_hardness,
            });
        }
    }
}

// -- geometry --------------------------------------------------------------------------------------

/// A point as (forward, side +left) from its centre, for a body facing `yaw` degrees.
pub fn to_local(body: &Body, point: [f64; 3], yaw: f32) -> (f64, f64) {
    let r = f64::from(yaw).to_radians();
    let (fx, fz) = (-r.sin(), r.cos());
    let (dx, dz) = (point[0] - body.feet[0], point[2] - body.feet[2]);
    (dx * fx + dz * fz, dx * fz - dz * fx)
}

/// The world (x, z) of a point (forward, side +left) from its centre, facing `yaw` degrees.
pub fn to_world(body: &Body, forward: f64, side: f64, yaw: f32) -> (f64, f64) {
    let r = f64::from(yaw).to_radians();
    let (fx, fz) = (-r.sin(), r.cos());
    (body.feet[0] + fx * forward + fz * side, body.feet[2] + fz * forward - fx * side)
}

fn segment_hits_body(from: [f64; 3], to: [f64; 3], s: &Seen, radius: f64) -> bool {
    let half = s.width / 2.0 + radius;
    let b = crate::hitboxes::Aabb {
        min: [s.feet[0] - half, s.feet[1] - radius, s.feet[2] - half],
        max: [s.feet[0] + half, s.feet[1] + s.height + radius, s.feet[2] + half],
    };
    b.contains(from) || b.clip(from, to).is_some()
}

pub fn approach_degrees(current: f32, wanted: f32, step: f32) -> f32 {
    let mut delta = (wanted - current) % 360.0;
    if delta > 180.0 {
        delta -= 360.0;
    } else if delta < -180.0 {
        delta += 360.0;
    }
    current + delta.clamp(-step, step)
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn horizontal_from(from: [f64; 3], to: [f64; 3]) -> [f64; 3] {
    normalize([to[0] - from[0], 0.0, to[2] - from[2]])
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn normalize(a: [f64; 3]) -> [f64; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if l < 1e-9 { [0.0; 3] } else { scale(a, 1.0 / l) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Open, flat ground at the kaiju's feet.
    struct Flat;

    impl Arena for Flat {
        fn clip_blocks(&self, _: [f64; 3], _: [f64; 3]) -> Option<[f64; 3]> {
            None
        }

        fn surface_y(&self, _: f64, _: f64, near_y: f64) -> f64 {
            near_y
        }

        fn clip_ground(&self, from: [f64; 3], to: [f64; 3]) -> Option<[f64; 3]> {
            // The ground is the plane y = 0.
            (from[1] >= 0.0 && to[1] < 0.0).then(|| {
                let t = from[1] / (from[1] - to[1]);
                [from[0] + (to[0] - from[0]) * t, 0.0, from[2] + (to[2] - from[2]) * t]
            })
        }

        fn ground_at(&self, _: f64, _: f64, near_y: f64, above: i32, below: i32) -> Option<f64> {
            (near_y - f64::from(below) <= 0.0 && near_y + f64::from(above) >= 0.0).then_some(0.0)
        }
    }

    /// The moves a kaiju starts and the hits it lands over `ticks`, a player standing `ahead`
    /// blocks in front of it, and whether it walked on the last tick.
    fn fight(species: &'static Species, ahead: f64, ticks: u32) -> (Vec<Move>, Vec<Cause>, bool) {
        let mut brain = Brain::new(1, species);
        // Facing +Z, the player straight ahead.
        let mut body = Body::default();
        let seen = [Seen { id: 7, feet: [0.0, 0.0, ahead], width: 0.6, height: 1.8, on_ground: true, is_player: true, visible: true, targetable: true }];
        let (mut walk, mut effects, mut moves, mut hits) = (None, Vec::new(), Vec::new(), Vec::new());
        for _ in 0..ticks {
            let before = brain.movement;
            effects.clear();
            brain.tick(&mut body, &seen, &Flat, &mut walk, &mut effects);
            if brain.movement != before && brain.movement != Move::None {
                moves.push(brain.movement);
            }
            hits.extend(effects.iter().filter_map(|e| match e {
                Effect::Hit { cause, .. } => Some(*cause),
                _ => None,
            }));
        }
        (moves, hits, walk.is_some())
    }

    #[test]
    fn zilla_roars_at_first_sight_then_bites_what_is_before_its_jaws() {
        let (moves, hits, walked) = fight(&crate::zila::SPECIES, 9.0, 200);
        assert_eq!(moves.first(), Some(&Move::Roar), "{moves:?}");
        assert_eq!(moves.iter().filter(|m| **m == Move::Roar).count(), 1, "it roars once at a target: {moves:?}");
        assert!(moves.contains(&Move::Bite), "{moves:?}");
        assert!(hits.contains(&Cause::Bite), "{hits:?}");
        // Already before its jaws: it holds its ground rather than walk onto it.
        assert!(!walked);
    }

    /// An ape and a player `ahead` blocks in front of it (and `up` above it) over `ticks`: the
    /// moves it makes, the hits it lands, and its brain.
    fn ape_fight(species: &'static Species, ahead: f64, up: f64, aggro: bool, ticks: u32, wounded: bool) -> (Vec<Move>, Vec<Cause>, Brain) {
        let mut brain = Brain::new(3, species);
        if aggro {
            brain.aggro(7);
        }
        if wounded {
            brain.wounded(0.4);
        }
        let mut body = Body::default();
        let seen = [Seen { id: 7, feet: [0.0, up, ahead], width: 0.6, height: 1.8, on_ground: true, is_player: true, visible: true, targetable: true }];
        let (mut walk, mut effects, mut moves, mut hits) = (None, Vec::new(), Vec::new(), Vec::new());
        for _ in 0..ticks {
            let before = (brain.movement, brain.move_ticks);
            effects.clear();
            brain.tick(&mut body, &seen, &Flat, &mut walk, &mut effects);
            brain.tick_boulders(&seen, &Flat, &mut effects);
            if brain.movement != Move::None && (brain.movement != before.0 || brain.move_ticks < before.1) {
                moves.push(brain.movement);
            }
            hits.extend(effects.iter().filter_map(|e| match e {
                Effect::Hit { cause, .. } => Some(*cause),
                _ => None,
            }));
            // It stays on the ground, as its host keeps it.
            if !brain.airborne() {
                body.feet[1] = 0.0;
            }
        }
        (moves, hits, brain)
    }

    #[test]
    fn kong_warns_whoever_comes_near_and_fights_whoever_comes_close() {
        // Inside his warning radius, outside his engage radius: a roar, and he lets them be.
        let (moves, hits, brain) = ape_fight(&crate::kong::KONG, 20.0, 0.0, false, 300, false);
        assert_eq!(moves, [Move::Roar], "{moves:?}");
        assert!(hits.is_empty() && brain.target.is_none(), "{hits:?}");
        // Close: a roar, then he swipes and slams.
        let (moves, hits, _) = ape_fight(&crate::kong::KONG, 6.0, 0.0, false, 400, false);
        assert_eq!(moves.first(), Some(&Move::Roar), "{moves:?}");
        assert!(moves.iter().any(|m| matches!(m, Move::Swipe | Move::Slam)), "{moves:?}");
        assert!(hits.iter().any(|c| matches!(c, Cause::Swipe | Cause::Shockwave)), "{hits:?}");
    }

    #[test]
    fn king_kong_leaps_and_throws_at_what_is_out_of_reach() {
        // Far off on the same level (and summoned at it): leaps and boulders.
        let (moves, hits, _) = ape_fight(&crate::kong::KING_KONG, 65.0, 0.0, true, 600, false);
        assert!(moves.iter().any(|m| matches!(m, Move::Leap | Move::Boulder)), "{moves:?}");
        assert!(!hits.is_empty(), "{moves:?}");
        // Up on a ledge: only boulders, and they land on it.
        let (moves, hits, _) = ape_fight(&crate::kong::KING_KONG, 20.0, 9.0, true, 400, false);
        assert!(moves.contains(&Move::Boulder) && !moves.iter().any(|m| matches!(m, Move::Swipe | Move::Slam | Move::Leap)), "{moves:?}");
        assert!(hits.contains(&Cause::Boulder), "{hits:?}");
    }

    #[test]
    fn an_ape_below_half_health_roars_and_is_enraged() {
        let (moves, _, brain) = ape_fight(&crate::kong::KONG, 60.0, 0.0, false, 60, true);
        assert_eq!(moves, [Move::Roar], "{moves:?}");
        assert!(brain.enraged());
    }

    #[test]
    fn godzilla_neither_bites_nor_roars() {
        let (moves, hits, _) = fight(&crate::godzilla::SPECIES, 9.0, 200);
        assert!(!moves.contains(&Move::Bite) && !moves.contains(&Move::Roar), "{moves:?}");
        assert!(!hits.contains(&Cause::Bite), "{hits:?}");
        assert!(moves.contains(&Move::Stomp), "close in front of him, he stomps: {moves:?}");
    }
}
