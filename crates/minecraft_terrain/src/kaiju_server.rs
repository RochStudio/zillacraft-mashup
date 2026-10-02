//! ZillaCraft's Godzilla on the server: his brain (the `kaiju` crate) driving a body that walks
//! the level the way a Minecraft mob does, crushing what he wades through, blasting the ground
//! with his atomic breath, and hitting players through the same path as any mob.

use kaiju::anim::Move;
use kaiju::brain::{Arena, Body, Brain, Cause, Effect, Seen, Walk};
use kaiju::godzilla;
use minecraftoss_core::BlockPos;
use minecraftoss_entities::world::{MobSound, PlayerHit, PlayerHitKind};
use minecraftoss_world::level::explosion::{BlockInteraction, Explosion};
use minecraftoss_world::level::{update, Level};

pub const GODZILLA: &str = "zillacraft:godzilla";
/// IDs well clear of the entity world's.
const FIRST_ID: u64 = 1 << 40;
/// Minecraft's ground friction for a mob walking on stone or dirt, and gravity per tick.
const GROUND_DRAG: f64 = 0.6 * 0.91;
const GRAVITY: f64 = 0.08;
/// He climbs rises this tall without stopping; anything taller he has to crush first.
const STEP_HEIGHT: i32 = 6;
const DEATH_TICKS: u32 = 60;
/// After a player respawns, kaiju neither see nor hit it for this long (5 s), so it isn't
/// killed again where it lands.
const SPAWN_GRACE: u32 = 100;
/// Having killed its target, or with a player respawning near it, a kaiju walks this far off
/// (past its follow range), for up to this long (a minute, at its pace), unless shot.
const RETREAT_DISTANCE: f64 = 110.0;
const RETREAT_TICKS: u32 = 1200;
/// MW2 players have 100 health and no armor: kaiju blows are scaled for them (the mod's
/// numbers are for Minecraft players in diamond armor), and a blast hurts at most this much.
const PLAYER_SHARE_MELEE: f32 = 0.5;
const PLAYER_SHARE_BREATH: f32 = 0.3;
const PLAYER_BLAST_MAX: f32 = 14.0;
/// For this long after a blow, a player only takes what a harder one adds: Minecraft's hurt
/// cooldown (`LivingEntity.hurt`), which MW2 players lack, so the breath's blasts every other
/// tick don't stack.
const HURT_COOLDOWN: u32 = 10;

/// What the client draws of a kaiju this tick.
#[derive(Clone, Debug)]
pub struct KaijuView {
    pub id: u64,
    pub feet: [f64; 3],
    pub yaw: f32,
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub walk_position: f32,
    pub walk_speed: f32,
    pub movement: Move,
    pub move_ticks: u32,
    pub move_direction: i32,
    pub breath_ticks: u32,
    pub breath_amount: f32,
    pub health: f32,
    pub max_health: f32,
    pub death_ticks: u32,
    pub hurt_ticks: u32,
    pub beam: Option<([f64; 3], [f64; 3])>,
    pub charge: Option<([f64; 3], f32)>,
    pub shockwave: Option<([f64; 3], f64)>,
    pub warnings: Vec<[f64; 3]>,
}

struct Kaiju {
    id: u64,
    body: Body,
    brain: Brain,
    health: f32,
    velocity: [f64; 3],
    on_ground: bool,
    walk_position: f32,
    walk_speed: f32,
    death_ticks: u32,
    hurt_ticks: u32,
    shown: Shown,
}

/// What a kaiju's moves showed this tick: the breath's beam and charge, a stomp's shockwave,
/// and the warnings before a move.
#[derive(Default)]
struct Shown {
    beam: Option<([f64; 3], [f64; 3])>,
    charge: Option<([f64; 3], f32)>,
    shockwave: Option<([f64; 3], f64)>,
    warnings: Vec<[f64; 3]>,
}

/// What a tick of the kaiju did, for the server's output.
#[derive(Default)]
pub struct KaijuTick {
    pub player_hits: Vec<PlayerHit>,
    pub sounds: Vec<MobSound>,
    /// Where each one that died fell, for its drops.
    pub deaths: Vec<[f64; 3]>,
}

/// A player as the kaiju see it.
#[derive(Clone, Copy, Debug)]
pub struct KaijuPlayer {
    pub id: u64,
    pub feet: [f64; 3],
    pub alive: bool,
    /// Fair game on sight: not in creative mode (they go for one of those only once it hurts
    /// them).
    pub attackable: bool,
}

#[derive(Default)]
pub struct KaijuWorld {
    list: Vec<Kaiju>,
    next_id: u64,
    /// Each player's liveness last tick, to see deaths and respawns.
    players_alive: std::collections::HashMap<u64, bool>,
    /// Players just respawned: ticks of grace left.
    grace: std::collections::HashMap<u64, u32>,
    /// Players just hurt: ticks of hurt cooldown left, and the blow that started it.
    cooldown: std::collections::HashMap<u64, (u32, f32)>,
}

impl KaijuWorld {
    /// A new kaiju; one summoned at a player (`aggro`) comes for it from any distance, unless
    /// it is in creative mode.
    pub fn spawn(&mut self, feet: [f64; 3], yaw: f32, aggro: Option<u64>) -> u64 {
        let id = FIRST_ID + self.next_id;
        self.next_id += 1;
        let mut brain = Brain::new(id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ feet[0].to_bits());
        if let Some(player) = aggro {
            brain.aggro(player);
        }
        self.list.push(Kaiju {
            id,
            body: Body { feet, yaw, head_yaw: yaw, ..Body::default() },
            brain,
            health: godzilla::MAX_HEALTH,
            velocity: [0.0; 3],
            on_ground: false,
            walk_position: 0.0,
            walk_speed: 0.0,
            death_ticks: 0,
            hurt_ticks: 0,
            shown: Shown::default(),
        });
        id
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// `kaiju clear`: every kaiju goes.
    pub fn clear(&mut self) -> usize {
        let count = self.list.len();
        self.list.clear();
        count
    }

    /// A hit from `attacker` for `damage` Minecraft health, before his armor.
    pub fn hurt(&mut self, id: u64, damage: f32, attacker: u64) {
        let Some(k) = self.list.iter_mut().find(|k| k.id == id && k.health > 0.0) else {
            return;
        };
        // `CombatRules.getDamageAfterAbsorb` with his 12 armor and no toughness.
        let armor = godzilla::ARMOR;
        let soaked = (armor / 5.0).max(armor - damage / 2.0).min(20.0);
        k.health -= damage * (1.0 - soaked / 25.0);
        k.hurt_ticks = 10;
        k.brain.provoked_by(attacker);
    }

    pub fn views(&self) -> Vec<KaijuView> {
        self.list
            .iter()
            .map(|k| KaijuView {
                id: k.id,
                feet: k.body.feet,
                yaw: k.body.yaw,
                head_yaw: k.body.head_yaw - k.body.yaw,
                head_pitch: k.body.head_pitch,
                walk_position: k.walk_position,
                walk_speed: k.walk_speed,
                movement: k.brain.movement,
                move_ticks: k.brain.move_ticks,
                move_direction: k.brain.move_direction,
                breath_ticks: k.brain.breath_ticks,
                breath_amount: k.brain.breath_amount,
                health: k.health.max(0.0),
                max_health: godzilla::MAX_HEALTH,
                death_ticks: k.death_ticks,
                hurt_ticks: k.hurt_ticks,
                beam: k.shown.beam,
                charge: k.shown.charge,
                shockwave: k.shown.shockwave,
                warnings: k.shown.warnings.clone(),
            })
            .collect()
    }

    /// Hit boxes of every living kaiju, in blocks: (id, box).
    pub fn boxes(views: &[KaijuView]) -> Vec<(u64, [f64; 6])> {
        let mut out = Vec::new();
        for v in views.iter().filter(|v| v.health > 0.0) {
            let tail_angle = match v.movement {
                Move::TailSwipe => godzilla::TAIL.angle(f64::from(v.move_ticks), v.move_direction),
                _ => 0.0,
            };
            let half = godzilla::WIDTH / 2.0;
            out.push((v.id, [v.feet[0] - half, v.feet[1], v.feet[2] - half, v.feet[0] + half, v.feet[1] + godzilla::HEIGHT, v.feet[2] + half]));
            for b in kaiju::hitboxes::world_boxes(&kaiju::hitboxes::GODZILLA, v.feet, v.yaw, &godzilla::TAIL, tail_angle) {
                out.push((v.id, [b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]]));
            }
        }
        out
    }

    /// One server tick.
    pub fn tick(&mut self, level: &mut Level<'static>, players: &[KaijuPlayer], griefing: bool) -> KaijuTick {
        let mut out = KaijuTick::default();
        // Deaths and respawns since last tick (not a player's first tick seen, alive or not).
        let mut fallen = Vec::new();
        for p in players {
            match self.players_alive.insert(p.id, p.alive) {
                Some(false) if p.alive => {
                    self.grace.insert(p.id, SPAWN_GRACE);
                }
                Some(true) if !p.alive => fallen.push((p.id, p.feet)),
                _ => {}
            }
        }
        self.grace.retain(|_, left| {
            *left = left.saturating_sub(1);
            *left > 0
        });
        self.cooldown.retain(|_, (left, _)| {
            *left = left.saturating_sub(1);
            *left > 0
        });
        let grace = &self.grace;
        let seen: Vec<Seen> = players
            .iter()
            .filter(|p| p.alive && !grace.contains_key(&p.id))
            .map(|p| Seen {
                id: p.id,
                feet: p.feet,
                width: 0.6,
                height: 1.8,
                on_ground: true,
                is_player: true,
                visible: true,
                targetable: p.attackable,
            })
            .collect();
        // Where players are respawning: followed through their grace, as their spawn moves
        // them onto the Minecraft spawn.
        let respawning: Vec<[f64; 3]> = players.iter().filter(|p| p.alive && grace.contains_key(&p.id)).map(|p| p.feet).collect();
        for k in &mut self.list {
            // Its target fell, or a player is respawning by it: it walks off, rather than wait
            // where they respawn.
            let fell = fallen.iter().find(|(id, _)| k.brain.target == Some(*id)).map(|&(_, at)| at);
            let near = |at: &[f64; 3]| (at[0] - k.body.feet[0]).hypot(at[2] - k.body.feet[2]) <= godzilla::FOLLOW_RANGE;
            let from = fell.or_else(|| respawning.iter().copied().find(near));
            if let Some(at) = from.filter(|_| k.health > 0.0) {
                let (dx, dz) = (k.body.feet[0] - at[0], k.body.feet[2] - at[2]);
                let length = dx.hypot(dz);
                let (dx, dz) = if length > 1.0 { (dx / length, dz / length) } else {
                    let yaw = f64::from(k.body.yaw).to_radians();
                    (yaw.sin(), -yaw.cos())
                };
                let to = [k.body.feet[0] + dx * RETREAT_DISTANCE, k.body.feet[2] + dz * RETREAT_DISTANCE];
                if !k.brain.retreating() {
                    out.sounds.push(sound("roar", k.body.feet, 8.0, k.brain.voice_pitch()));
                }
                k.brain.retreat(to, RETREAT_TICKS);
            }
            k.hurt_ticks = k.hurt_ticks.saturating_sub(1);
            if k.health <= 0.0 {
                if k.death_ticks == 0 {
                    out.sounds.push(sound("death", k.body.feet, 6.0, 1.0));
                }
                k.death_ticks += 1;
                if k.death_ticks == DEATH_TICKS {
                    out.deaths.push(k.body.feet);
                }
                k.shown = Shown::default();
                continue;
            }
            let seen: Vec<Seen> = seen
                .iter()
                .map(|s| Seen { visible: has_line_of_sight(level, eye_of(&k.body), s.eyes()), ..*s })
                .collect();
            let mut walk = None;
            let mut effects = Vec::new();
            {
                let arena = LevelArena { level };
                k.brain.tick(&mut k.body, &seen, &arena, &mut walk, &mut effects);
            }
            Self::step(k, level, walk);
            k.shown = Shown::default();
            for effect in effects {
                apply(k, effect, level, griefing, &mut out, &seen);
            }
            if k.brain.movement == Move::None && k.brain.breath_ticks == 0 && k.brain.chance(240) {
                out.sounds.push(sound("ambient", k.body.feet, 4.0, k.brain.voice_pitch()));
            }
        }
        self.list.retain(|k| k.death_ticks < DEATH_TICKS);
        let cooldown = &mut self.cooldown;
        out.player_hits.retain_mut(|hit| match cooldown.get_mut(&hit.player_id) {
            Some((_, last)) if hit.damage <= *last => false,
            Some((_, last)) => {
                (hit.damage, *last) = (hit.damage - *last, hit.damage);
                true
            }
            None => {
                cooldown.insert(hit.player_id, (HURT_COOLDOWN, hit.damage));
                true
            }
        });
        out
    }

    /// Walks him a tick: Minecraft's mob ground movement, up rises of up to six blocks, falling
    /// when there's nothing under him. He doesn't float: he wades along the bottom.
    fn step(k: &mut Kaiju, level: &Level<'static>, walk: Option<Walk>) {
        let mut accel = [0.0, 0.0];
        if let Some(walk) = walk {
            Brain::turn_towards(&mut k.body, walk.to);
            let yaw = f64::from(k.body.yaw).to_radians();
            // `Mob.setSpeed` feeds both the input and the friction-influenced speed.
            let speed = godzilla::SPEED * walk.speed;
            accel = [-yaw.sin() * speed * speed, yaw.cos() * speed * speed];
        }
        k.velocity[0] = (k.velocity[0] + accel[0]) * GROUND_DRAG;
        k.velocity[2] = (k.velocity[2] + accel[1]) * GROUND_DRAG;
        let next = [k.body.feet[0] + k.velocity[0], k.body.feet[2] + k.velocity[2]];
        let here = k.body.feet[1].floor() as i32;
        let ground = ground_top(level, next[0], next[1], here);
        k.body.blocked = false;
        match ground {
            Some(top) if top - here > STEP_HEIGHT => {
                // A cliff: he stands against it while the rampage crushes it.
                k.velocity[0] = 0.0;
                k.velocity[2] = 0.0;
                k.body.blocked = true;
            }
            _ => {
                k.body.feet[0] = next[0];
                k.body.feet[2] = next[1];
            }
        }
        let ground = ground_top(level, k.body.feet[0], k.body.feet[2], here).map(f64::from);
        match ground {
            Some(top) if top >= k.body.feet[1] - 0.001 => {
                k.body.feet[1] = top;
                k.velocity[1] = 0.0;
                k.on_ground = true;
            }
            _ => {
                k.velocity[1] = (k.velocity[1] - GRAVITY) * 0.98;
                let fallen = k.body.feet[1] + k.velocity[1];
                match ground {
                    Some(top) if fallen <= top => {
                        k.body.feet[1] = top;
                        k.velocity[1] = 0.0;
                        k.on_ground = true;
                    }
                    _ => {
                        k.body.feet[1] = fallen.max(f64::from(level.min_y()));
                        k.on_ground = false;
                    }
                }
            }
        }
        // `WalkAnimationState`: the distance walked drives the stride.
        let moved = k.velocity[0].hypot(k.velocity[2]) as f32;
        k.body.moving = moved > 0.01;
        k.walk_speed += ((moved * 4.0).min(1.0) - k.walk_speed) * 0.4;
        k.walk_position += k.walk_speed;
    }
}

fn apply(k: &mut Kaiju, effect: Effect, level: &mut Level<'static>, griefing: bool, out: &mut KaijuTick, seen: &[Seen]) {
    match effect {
        Effect::Hit { target, damage, push, lift, cause, .. } => {
            let damage = damage * if cause == Cause::AtomicBreath { PLAYER_SHARE_BREATH } else { PLAYER_SHARE_MELEE };
            let kind = match cause {
                Cause::Claws => PlayerHitKind::Melee {
                    attacker: glam_dvec(k.body.feet),
                    hunger_ticks: 0,
                    lift: lift as f32,
                },
                _ => PlayerHitKind::Explosion { knockback: glam_dvec([push[0], lift, push[2]]) },
            };
            if seen.iter().any(|s| s.id == target && s.is_player) {
                out.player_hits.push(PlayerHit { player_id: target, damage, kind, source: Some(k.id) });
            }
        }
        Effect::Sound { name, volume, pitch } => {
            let (group, file) = name.split_once('/').unwrap_or(("godzilla", name));
            out.sounds.push(MobSound {
                event: format!("zillacraft:entity.{group}.{file}"),
                position: glam_dvec(k.body.feet),
                volume,
                pitch,
                category: "hostile",
            });
        }
        Effect::Blast { at, power } => {
            let interaction = if griefing { BlockInteraction::DestroyWithDecay } else { BlockInteraction::Keep };
            level.explode(Explosion { center: at, radius: power, fire: true, interaction, source: None });
            // The level's blast leaves players to the entity world: hurt them here, as `ServerExplosion` does.
            for s in seen.iter().filter(|s| s.is_player) {
                if let Some((damage, push)) = blast_damage(at, power, s) {
                    let damage = damage.min(PLAYER_BLAST_MAX);
                    out.player_hits.push(PlayerHit { player_id: s.id, damage, kind: PlayerHitKind::Explosion { knockback: glam_dvec(push) }, source: Some(k.id) });
                }
            }
        }
        Effect::Crush { min, max, budget, max_hardness } => {
            if griefing {
                crush(level, min, max, budget, max_hardness);
            }
        }
        Effect::Warning { at } => k.shown.warnings.push(at),
        Effect::Beam { from, to } => k.shown.beam = Some((from, to)),
        Effect::Charge { at, strength } => k.shown.charge = Some((at, strength)),
        Effect::Shockwave { at, radius } => k.shown.shockwave = Some((at, radius)),
    }
}

/// `ServerExplosion.hurtEntities` for a player in the open: damage and push.
fn blast_damage(at: [f64; 3], power: f32, s: &Seen) -> Option<(f32, [f64; 3])> {
    let reach = f64::from(power) * 2.0;
    let d = [s.feet[0] - at[0], s.feet[1] + s.height / 2.0 - at[1], s.feet[2] - at[2]];
    let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if distance > reach {
        return None;
    }
    let impact = 1.0 - distance / reach;
    let damage = ((impact * impact + impact) / 2.0 * 7.0 * reach + 1.0) as f32;
    let dir = if distance > 1e-6 { [d[0] / distance, d[1] / distance, d[2] / distance] } else { [0.0, 1.0, 0.0] };
    Some((damage, [dir[0] * impact, dir[1] * impact, dir[2] * impact]))
}

fn crush(level: &mut Level<'static>, min: [f64; 3], max: [f64; 3], mut budget: u32, max_hardness: f32) {
    let lo = [min[0].floor() as i32, min[1].floor() as i32, min[2].floor() as i32];
    let hi = [max[0].floor() as i32, max[1].floor() as i32, max[2].floor() as i32];
    for y in lo[1]..=hi[1] {
        for z in lo[2]..=hi[2] {
            for x in lo[0]..=hi[0] {
                if budget == 0 {
                    return;
                }
                let pos = BlockPos::new(x, y, z);
                let state = level.block(pos);
                let blocks = &level.registries().blocks;
                if blocks.is_air(state) || blocks.has_fluid(state) {
                    continue;
                }
                let hardness = blocks.state(state).destroy_speed;
                if (0.0..=max_hardness).contains(&hardness) && level.destroy_block(pos, update::LIMIT) {
                    budget -= 1;
                }
            }
        }
    }
}

/// The top of the highest solid block under (x, z) from `near` + the step height down to 64
/// blocks below, if there is one.
fn ground_top(level: &Level<'static>, x: f64, z: f64, near: i32) -> Option<i32> {
    let (bx, bz) = (x.floor() as i32, z.floor() as i32);
    let floor = level.min_y();
    let mut y = near + STEP_HEIGHT + 1;
    while y >= (near - 64).max(floor) {
        if solid(level, BlockPos::new(bx, y, bz)) {
            return Some(y + 1);
        }
        y -= 1;
    }
    None
}

fn solid(level: &Level<'static>, pos: BlockPos) -> bool {
    let state = level.block(pos);
    let blocks = &level.registries().blocks;
    !blocks.is_air(state) && !blocks.has_fluid(state) && blocks.state(state).collision_full_block
}

fn eye_of(body: &Body) -> [f64; 3] {
    [body.feet[0], body.feet[1] + godzilla::breath::MOUTH_UP, body.feet[2]]
}

/// Kaiju see through foliage, not walls (the mod's rule): leaves don't block sight.
fn has_line_of_sight(level: &Level<'static>, from: [f64; 3], to: [f64; 3]) -> bool {
    first_solid(level, from, to, true).is_none()
}

fn leaves(level: &Level<'static>, pos: BlockPos) -> bool {
    let blocks = &level.registries().blocks;
    blocks.block(blocks.state(level.block(pos)).block).is_a("LeavesBlock")
}

/// The first solid block a line passes through (a voxel walk), as the point where it enters.
fn first_solid(level: &Level<'static>, from: [f64; 3], to: [f64; 3], through_leaves: bool) -> Option<[f64; 3]> {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if length < 1e-9 {
        return None;
    }
    let steps = (length * 2.0).ceil() as usize;
    let mut last = [i32::MIN; 3];
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let p = [from[0] + d[0] * t, from[1] + d[1] * t, from[2] + d[2] * t];
        let cell = [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32];
        if cell == last {
            continue;
        }
        last = cell;
        let pos = BlockPos::new(cell[0], cell[1], cell[2]);
        if solid(level, pos) && !(through_leaves && leaves(level, pos)) {
            return Some(p);
        }
    }
    None
}

struct LevelArena<'a> {
    level: &'a Level<'static>,
}

impl Arena for LevelArena<'_> {
    fn clip_blocks(&self, from: [f64; 3], to: [f64; 3]) -> Option<[f64; 3]> {
        first_solid(self.level, from, to, false)
    }

    fn surface_y(&self, x: f64, z: f64, near_y: f64) -> f64 {
        ground_top(self.level, x, z, near_y.floor() as i32).map_or(near_y, f64::from)
    }
}

fn sound(file: &str, at: [f64; 3], volume: f32, pitch: f32) -> MobSound {
    MobSound {
        event: format!("zillacraft:entity.godzilla.{file}"),
        position: glam_dvec(at),
        volume,
        pitch,
        category: "hostile",
    }
}

fn glam_dvec(p: [f64; 3]) -> glam::DVec3 {
    glam::DVec3::new(p[0], p[1], p[2])
}
