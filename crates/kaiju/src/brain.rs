//! Godzilla's behaviour, one tick at a time, independent of the engine that hosts him.
//!
//! The host owns his body (position, physics, health) and the world; each tick it hands the
//! brain what he can see, and the brain turns him, says where to walk, and lists what happens:
//! hits, sounds, blasts, blocks to crush. No pathfinding: no path is ever that wide. He walks
//! straight at things, turning slowly, crushing whatever is in the way.

use crate::anim::Move;
use crate::combat::{GROUND_REACH_DY, Tail};
use crate::godzilla::{self, breath};

/// Something he can see: a player, a mob, anything alive.
#[derive(Clone, Copy, Debug)]
pub struct Seen {
    pub id: u64,
    pub feet: [f64; 3],
    pub width: f64,
    pub height: f64,
    pub on_ground: bool,
    pub is_player: bool,
    /// Whether he has line of sight to it.
    pub visible: bool,
    /// Whether he picks it out on sight: not a player in creative mode, whom he only goes for
    /// once it hurts him.
    pub targetable: bool,
}

impl Seen {
    pub fn eyes(&self) -> [f64; 3] {
        [self.feet[0], self.feet[1] + self.height * 0.85, self.feet[2]]
    }
}

/// His body as the host simulates it.
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
}

/// What happened this tick, for the host to carry out.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Hurt `target`, push it along `push` (blocks per tick, already scaled), and maybe set it alight.
    Hit { target: u64, damage: f32, push: [f64; 3], lift: f64, burn_seconds: f32, cause: Cause },
    Sound { name: &'static str, volume: f32, pitch: f32 },
    /// An explosion that breaks blocks and hurts what it reaches (never him).
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
    AtomicBreath,
}

/// Where the host should walk him this tick: towards `to` at `speed` times his walking speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walk {
    pub to: [f64; 2],
    pub speed: f64,
}

#[derive(Debug)]
pub struct Brain {
    pub movement: Move,
    pub move_ticks: u32,
    pub move_direction: i32,
    move_yaw: f32,
    tail_cooldown: u32,
    stomp_cooldown: u32,
    recovery: u32,
    hit: Vec<u64>,
    foot: [f64; 3],
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
    /// The target hurt him: he keeps after it even if it isn't fair game on sight.
    provoked: bool,
    rng: u64,
}

impl Brain {
    pub fn new(seed: u64) -> Self {
        Self {
            movement: Move::None,
            move_ticks: 0,
            move_direction: 1,
            move_yaw: 0.0,
            tail_cooldown: 0,
            stomp_cooldown: 0,
            recovery: 0,
            hit: Vec::new(),
            foot: [0.0; 3],
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

    /// One tick. `seen` is everything alive around him (he picks his own target), `walk_out`
    /// receives where to walk, and `effects` what happens.
    pub fn tick(&mut self, body: &mut Body, seen: &[Seen], arena: &impl Arena, walk_out: &mut Option<Walk>, effects: &mut Vec<Effect>) {
        *walk_out = None;
        self.claw_cooldown = self.claw_cooldown.saturating_sub(1);
        self.tail_cooldown = self.tail_cooldown.saturating_sub(1);
        self.stomp_cooldown = self.stomp_cooldown.saturating_sub(1);
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
            // Planted: he doesn't turn during a move.
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

        // The rampage: he pulverizes what he wades through, across his whole height.
        let tail_sweeping = self.movement == Move::TailSwipe && godzilla::TAIL.sweeping(self.move_ticks);
        if body.moving || body.blocked || tail_sweeping {
            self.crush(body, effects);
        }
    }

    fn pick_target(&mut self, body: &Body, seen: &[Seen]) -> Option<Seen> {
        // Keep the current target while it's in range (one not fair game on sight only if it
        // hurt him); otherwise the nearest visible player.
        if let Some(id) = self.target
            && let Some(t) = seen.iter().find(|s| s.id == id)
            && distance(body.feet, t.feet) <= godzilla::FOLLOW_RANGE * 1.5
            && (t.targetable || self.provoked)
        {
            return Some(*t);
        }
        let nearest = seen
            .iter()
            .filter(|s| s.visible && s.is_player && s.targetable && distance(body.feet, s.feet) <= godzilla::FOLLOW_RANGE)
            .min_by(|a, b| distance(body.feet, a.feet).total_cmp(&distance(body.feet, b.feet)))
            .copied();
        self.target = nearest.map(|t| t.id);
        self.provoked = false;
        nearest
    }

    /// Something that hurt him becomes his target, even while he's walking away.
    pub fn provoked_by(&mut self, attacker: u64) {
        self.retreat = None;
        self.target = Some(attacker);
        self.provoked = true;
    }

    /// He comes for `player` (whoever summoned him), if it is fair game.
    pub fn aggro(&mut self, player: u64) {
        self.target = Some(player);
    }

    /// His target is dead: he lets it be and lumbers off towards `to` for up to `ticks`.
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
        *walk_out = Some(Walk { to: [target.feet[0], target.feet[2]], speed: 1.0 });
        if self.claw_cooldown == 0 && in_reach(body, target) {
            effects.push(Effect::Hit {
                target: target.id,
                damage: godzilla::CLAW_DAMAGE,
                push: scale(horizontal_from(body.feet, target.feet), 3.0 * 0.5),
                lift: 0.4,
                burn_seconds: 0.0,
                cause: Cause::Claws,
            });
            self.claw_cooldown = 20;
        }
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

    /// Turns him towards a point at his turn rate: ships, not zombies.
    pub fn turn_towards(body: &mut Body, to: [f64; 2]) {
        let wanted = ((to[1] - body.feet[2]).atan2(to[0] - body.feet[0]).to_degrees() - 90.0) as f32;
        body.yaw = approach_degrees(body.yaw, wanted, godzilla::TURN_RATE);
    }

    fn look_at(&self, body: &mut Body, eyes: [f64; 3]) {
        let mouth_y = body.feet[1] + breath::MOUTH_UP;
        let dx = eyes[0] - body.feet[0];
        let dz = eyes[2] - body.feet[2];
        let wanted_yaw = (dz.atan2(dx).to_degrees() - 90.0) as f32;
        body.head_yaw = approach_degrees(body.head_yaw, wanted_yaw, 10.0);
        let pitch = -((eyes[1] - mouth_y).atan2(dx.hypot(dz)).to_degrees()) as f32;
        body.head_pitch += (pitch - body.head_pitch).clamp(-10.0, 10.0);
    }

    // -- close-range moves -----------------------------------------------------------------------

    fn start_move(&mut self, body: &Body, target: &Seen, effects: &mut Vec<Effect>) -> bool {
        if self.recovery > 0 {
            return false;
        }
        let (forward, side) = to_local(body, target.feet, body.yaw);
        let half_width = target.width / 2.0;
        let dy = target.feet[1] - body.feet[1];
        let top = target.feet[1] + target.height - body.feet[1];
        let tail = godzilla::TAIL;
        let bearing = tail.bearing_from_pivot(forward, side);
        let choice = if self.tail_cooldown == 0
            && dy >= -GROUND_REACH_DY
            && tail.worth_swiping(bearing, tail.distance_from_pivot(forward, side), half_width, dy, top)
        {
            Some((Move::TailSwipe, Tail::direction_for(bearing)))
        } else if self.stomp_cooldown == 0 && dy.abs() <= GROUND_REACH_DY && godzilla::STOMP.worth_it(forward.hypot(side), half_width) {
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
        let stomp = godzilla::STOMP;
        let (fx, fz) = to_world(body, stomp.foot_forward, f64::from(direction) * stomp.foot_side, self.move_yaw);
        self.foot = [fx, body.feet[1], fz];
        let pitch = self.voice_pitch();
        effects.push(Effect::Sound { name: "godzilla/growl", volume: 6.0, pitch });
        true
    }

    fn tick_move(&mut self, body: &Body, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        self.move_ticks += 1;
        let total = match self.movement {
            Move::TailSwipe => godzilla::TAIL.total(),
            Move::Stomp => godzilla::STOMP.total,
            Move::None => 0,
        };
        match self.movement {
            Move::TailSwipe => self.tick_tail(body, seen, arena, effects),
            Move::Stomp => self.tick_stomp(body, seen, arena, effects),
            Move::None => {}
        }
        if self.move_ticks >= total {
            match self.movement {
                Move::TailSwipe => self.tail_cooldown = godzilla::TAIL_COOLDOWN.0 + self.below(godzilla::TAIL_COOLDOWN.1),
                Move::Stomp => self.stomp_cooldown = godzilla::STOMP_COOLDOWN.0 + self.below(godzilla::STOMP_COOLDOWN.1),
                Move::None => {}
            }
            self.movement = Move::None;
            self.move_ticks = 0;
            self.recovery = godzilla::MOVE_RECOVERY;
        }
    }

    fn tick_tail(&mut self, body: &Body, seen: &[Seen], arena: &impl Arena, effects: &mut Vec<Effect>) {
        let tail = godzilla::TAIL;
        let t = self.move_ticks;
        let yaw = self.move_yaw;
        if t < tail.windup {
            if t % 4 == 1 {
                // The far edge of the ground the tail will sweep.
                let reach = tail.reach();
                let points = 16.max((reach * 1.2) as u32);
                for i in 0..=points {
                    let bearing = -tail.arc + 2.0 * tail.arc * f64::from(i) / f64::from(points);
                    let (x, z) = to_world(body, -tail.pivot_back - bearing.cos() * reach, bearing.sin() * reach, yaw);
                    effects.push(Effect::Warning { at: [x, arena.surface_y(x, z, body.feet[1]), z] });
                }
            }
            return;
        }
        if t == tail.windup + 1 {
            effects.push(Effect::Sound { name: "kaiju/tail_swipe", volume: 6.0, pitch: 0.85 });
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
        let stomp = godzilla::STOMP;
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
            effects.push(Effect::Sound { name: "kaiju/stomp", volume: 9.0, pitch: 0.9 });
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
            // Each thing is judged once, as the wave reaches it: only what stands on the ground at his level.
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

    // -- atomic breath ---------------------------------------------------------------------------

    fn mouth(body: &Body) -> [f64; 3] {
        let yaw = f64::from(body.head_yaw).to_radians();
        [body.feet[0] - yaw.sin() * breath::MOUTH_FORWARD, body.feet[1] + breath::MOUTH_UP, body.feet[2] + yaw.cos() * breath::MOUTH_FORWARD]
    }

    fn start_breath(&mut self, body: &Body, target: &Seen, effects: &mut Vec<Effect>) -> bool {
        if self.breath_cooldown > 0 || !target.visible {
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
        effects.push(Effect::Sound { name: "godzilla/growl", volume: 6.0, pitch });
        effects.push(Effect::Sound { name: "godzilla/breath_charge", volume: 6.0, pitch: 1.0 });
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
        Self::turn_towards(body, [target.feet[0], target.feet[2]]);
        let mouth = Self::mouth(body);
        let aim = normalize(sub(target.eyes(), mouth));
        if self.breath_ticks < breath::WINDUP_TICKS {
            effects.push(Effect::Charge { at: mouth, strength: self.breath_ticks as f32 / breath::WINDUP_TICKS as f32 });
            return;
        }
        if self.breath_ticks == breath::WINDUP_TICKS {
            effects.push(Effect::Sound { name: "godzilla/breath_fire", volume: 8.0, pitch: 1.0 });
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
        let half = godzilla::WIDTH / 2.0 + 0.5;
        // Never below his feet, or he digs himself a pit; up his full height so trees never poke through.
        let feet_y = (body.feet[1] + 0.001).floor();
        effects.push(Effect::Crush {
            min: [body.feet[0] - half, feet_y, body.feet[2] - half],
            max: [body.feet[0] + half, body.feet[1] + godzilla::HEIGHT + 0.5, body.feet[2] + half],
            budget: godzilla::CRUSH_BUDGET,
            max_hardness: godzilla::CRUSH_HARDNESS,
        });
        // His legs and the tail carve through terrain too, never below his ground plane.
        let tail_angle = if self.movement == Move::TailSwipe {
            godzilla::TAIL.angle(f64::from(self.move_ticks), self.move_direction)
        } else {
            0.0
        };
        for (i, seg) in godzilla::TAIL.segments.iter().enumerate() {
            let (mut forward, mut side) = (seg.forward, seg.side);
            if tail_angle != 0.0 {
                (forward, side) = godzilla::TAIL.swing_point(forward, side, tail_angle);
            }
            let (x, z) = to_world(body, forward, side, body.yaw);
            let half = seg.width / 2.0 + 0.25;
            let y = body.feet[1] + seg.up;
            effects.push(Effect::Crush {
                min: [x - half, (y - 0.25).max(feet_y), z - half],
                max: [x + half, (y + seg.height + 0.25).max(feet_y), z + half],
                budget: godzilla::CRUSH_BUDGET / (i as u32 + 2),
                max_hardness: godzilla::CRUSH_HARDNESS,
            });
        }
    }
}

// -- geometry --------------------------------------------------------------------------------------

/// A point as (forward, side +left) from his centre, for a body facing `yaw` degrees.
pub fn to_local(body: &Body, point: [f64; 3], yaw: f32) -> (f64, f64) {
    let r = f64::from(yaw).to_radians();
    let (fx, fz) = (-r.sin(), r.cos());
    let (dx, dz) = (point[0] - body.feet[0], point[2] - body.feet[2]);
    (dx * fx + dz * fz, dx * fz - dz * fx)
}

/// The world (x, z) of a point (forward, side +left) from his centre, facing `yaw` degrees.
pub fn to_world(body: &Body, forward: f64, side: f64, yaw: f32) -> (f64, f64) {
    let r = f64::from(yaw).to_radians();
    let (fx, fz) = (-r.sin(), r.cos());
    (body.feet[0] + fx * forward + fz * side, body.feet[2] + fz * forward - fx * side)
}

fn in_reach(body: &Body, target: &Seen) -> bool {
    let reach = godzilla::WIDTH / 2.0 + godzilla::CLAW_REACH + target.width / 2.0;
    (target.feet[0] - body.feet[0]).abs() <= reach
        && (target.feet[2] - body.feet[2]).abs() <= reach
        && target.feet[1] < body.feet[1] + godzilla::HEIGHT + godzilla::CLAW_REACH
        && target.feet[1] + target.height > body.feet[1] - godzilla::CLAW_REACH
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
