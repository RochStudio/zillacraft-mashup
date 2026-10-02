//! ZillaCraft's kaiju on the server (Godzilla, Zilla, Kong and King Kong): each one's brain (the
//! `kaiju` crate) driving a body that walks the level the way a Minecraft mob does, crushing what
//! it wades through, Godzilla blasting the ground with his atomic breath, and hitting players
//! through the same path as any mob. The apes walk their own way: through everything, over the
//! ground, throwing boulders.

use kaiju::Species;
use kaiju::anim::Move;
use kaiju::ape;
use kaiju::brain::{Arena, Body, Brain, Cause, Effect, Seen, Walk};
use minecraftoss_core::BlockPos;
use minecraftoss_core::tags::TagId;
use minecraftoss_entities::world::{MobSound, PlayerHit, PlayerHitKind};
use minecraftoss_world::level::explosion::{BlockInteraction, Explosion};
use minecraftoss_world::level::{update, Level};

/// IDs well clear of the entity world's.
const FIRST_ID: u64 = 1 << 40;
/// Minecraft's ground friction for a mob walking on stone or dirt, and gravity per tick.
const GROUND_DRAG: f64 = 0.6 * 0.91;
const GRAVITY: f64 = 0.08;
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
    pub species: &'static Species,
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
    /// An ape below half health: glowing eyes, a red boss bar.
    pub enraged: bool,
    /// Boulders an ape has thrown, still in the air.
    pub boulders: Vec<ape::Boulder>,
    pub beam: Option<([f64; 3], [f64; 3])>,
    pub charge: Option<([f64; 3], f32)>,
    /// A shockwave's ring this tick, on the ground.
    pub shockwave: Vec<[f64; 3]>,
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
    /// A hit is waiting for his hurt sound, and ticks until another may have one.
    hurt_sound: bool,
    hurt_sound_ticks: u32,
    /// How far an ape has walked since its last heavy footstep, and which foot that was.
    stride: f64,
    shown: Shown,
}

/// What a kaiju's moves showed this tick: the breath's beam and charge, a stomp's shockwave,
/// and the warnings before a move.
#[derive(Default)]
struct Shown {
    beam: Option<([f64; 3], [f64; 3])>,
    charge: Option<([f64; 3], f32)>,
    shockwave: Vec<[f64; 3]>,
    warnings: Vec<[f64; 3]>,
}

/// What a tick of the kaiju did, for the server's output.
#[derive(Default)]
pub struct KaijuTick {
    pub player_hits: Vec<PlayerHit>,
    pub sounds: Vec<MobSound>,
    /// Where each one that died fell, and what it was, for its drops.
    pub deaths: Vec<([f64; 3], &'static Species)>,
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
    /// A new kaiju of `species`; one summoned at a player (`aggro`) comes for it from any
    /// distance, unless it is in creative mode.
    pub fn spawn(&mut self, species: &'static Species, feet: [f64; 3], yaw: f32, aggro: Option<u64>) -> u64 {
        let id = FIRST_ID + self.next_id;
        self.next_id += 1;
        let mut brain = Brain::new(id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ feet[0].to_bits(), species);
        if let Some(player) = aggro {
            brain.aggro(player);
        }
        self.list.push(Kaiju {
            id,
            body: Body { feet, yaw, head_yaw: yaw, ..Body::default() },
            brain,
            health: species.max_health,
            velocity: [0.0; 3],
            on_ground: false,
            walk_position: 0.0,
            walk_speed: 0.0,
            death_ticks: 0,
            hurt_ticks: 0,
            hurt_sound: false,
            hurt_sound_ticks: 0,
            stride: 0.0,
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

    /// A hit from `attacker` for `damage` Minecraft health, before its armor.
    pub fn hurt(&mut self, id: u64, damage: f32, attacker: u64) {
        let Some(k) = self.list.iter_mut().find(|k| k.id == id && k.health > 0.0) else {
            return;
        };
        // `CombatRules.getDamageAfterAbsorb` with its armor and toughness.
        let (armor, toughness) = (k.brain.species.armor, k.brain.species.toughness);
        let soaked = (armor / 5.0).max(armor - damage / (2.0 + toughness / 4.0)).min(20.0);
        k.health -= damage * (1.0 - soaked / 25.0);
        k.hurt_ticks = 10;
        k.brain.wounded(k.health / k.brain.species.max_health);
        // Its hurt sound at most every half second, as vanilla's hurt cooldown allows one.
        if k.hurt_sound_ticks == 0 {
            k.hurt_sound = true;
            k.hurt_sound_ticks = HURT_COOLDOWN;
        }
        k.brain.provoked_by(attacker);
    }

    pub fn views(&self) -> Vec<KaijuView> {
        self.list
            .iter()
            .map(|k| KaijuView {
                id: k.id,
                species: k.brain.species,
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
                max_health: k.brain.species.max_health,
                death_ticks: k.death_ticks,
                hurt_ticks: k.hurt_ticks,
                enraged: k.brain.enraged(),
                boulders: k.brain.boulders.clone(),
                beam: k.shown.beam,
                charge: k.shown.charge,
                shockwave: k.shown.shockwave.clone(),
                warnings: k.shown.warnings.clone(),
            })
            .collect()
    }

    /// Hit boxes of every living kaiju, in blocks: (id, box).
    pub fn boxes(views: &[KaijuView]) -> Vec<(u64, [f64; 6])> {
        let mut out = Vec::new();
        for v in views.iter().filter(|v| v.health > 0.0) {
            let s = v.species;
            let tail_angle = match (v.movement, s.tail) {
                (Move::TailSwipe, Some(tail)) => tail.angle(f64::from(v.move_ticks), v.move_direction),
                _ => 0.0,
            };
            let half = s.width / 2.0;
            out.push((v.id, [v.feet[0] - half, v.feet[1], v.feet[2] - half, v.feet[0] + half, v.feet[1] + s.column_height, v.feet[2] + half]));
            for b in kaiju::hitboxes::world_boxes(s.hitboxes, v.feet, v.yaw, s.tail.as_ref(), tail_angle) {
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
        let logs = level.registries().block_tags.id("minecraft:logs");
        for k in &mut self.list {
            // Its target fell, or a player is respawning by it: it walks off, rather than wait
            // where they respawn.
            let species = k.brain.species;
            let fell = fallen.iter().find(|(id, _)| k.brain.target == Some(*id)).map(|&(_, at)| at);
            let near = |at: &[f64; 3]| (at[0] - k.body.feet[0]).hypot(at[2] - k.body.feet[2]) <= species.follow_range;
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
                    let pitch = k.brain.voice_pitch();
                    out.sounds.push(sound(species.voice, "roar", k.body.feet, species.move_volume * 1.5, pitch));
                }
                k.brain.retreat(to, RETREAT_TICKS);
            }
            k.hurt_ticks = k.hurt_ticks.saturating_sub(1);
            k.hurt_sound_ticks = k.hurt_sound_ticks.saturating_sub(1);
            if std::mem::take(&mut k.hurt_sound) && k.health > 0.0 {
                let pitch = k.brain.voice_pitch() * species.voice_pitch;
                out.sounds.push(sound(species.voice, "hurt", k.body.feet, species.voice_volume * 1.25, pitch));
            }
            // An ape's boulders fly on whatever becomes of it.
            if !k.brain.boulders.is_empty() {
                let arena = LevelArena { level, logs };
                let mut effects = Vec::new();
                k.brain.tick_boulders(&seen, &arena, &mut effects);
                for effect in effects {
                    apply(k, effect, level, griefing, &mut out, &seen);
                }
            }
            if k.health <= 0.0 {
                if k.death_ticks == 0 {
                    out.sounds.push(sound(species.voice, "death", k.body.feet, species.voice_volume * 1.5, species.voice_pitch));
                }
                k.death_ticks += 1;
                if k.death_ticks == DEATH_TICKS {
                    out.deaths.push((k.body.feet, species));
                }
                k.shown = Shown::default();
                continue;
            }
            let arena = LevelArena { level, logs };
            let seen: Vec<Seen> = seen.iter().map(|s| Seen { visible: arena.sees(&k.body, species, s.eyes()), ..*s }).collect();
            let mut walk = None;
            let mut effects = Vec::new();
            k.brain.tick(&mut k.body, &seen, &arena, &mut walk, &mut effects);
            if species.ape.is_some() {
                Self::step_ape(k, &arena, walk, &mut out);
            } else {
                Self::step(k, level, walk);
            }
            k.shown = Shown::default();
            for effect in effects {
                apply(k, effect, level, griefing, &mut out, &seen);
            }
            if k.brain.movement == Move::None && k.brain.breath_ticks == 0 && k.brain.chance(240) {
                let pitch = k.brain.voice_pitch() * species.voice_pitch;
                out.sounds.push(sound(species.voice, "ambient", k.body.feet, species.voice_volume, pitch));
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

    /// Walks it a tick: Minecraft's mob ground movement, up rises of up to its step height,
    /// falling when there's nothing under it. It doesn't float: it wades along the bottom.
    fn step(k: &mut Kaiju, level: &Level<'static>, walk: Option<Walk>) {
        let species = k.brain.species;
        let mut accel = [0.0, 0.0];
        if let Some(walk) = walk {
            k.brain.turn_towards(&mut k.body, walk.to);
            let yaw = f64::from(k.body.yaw).to_radians();
            // `Mob.setSpeed` feeds both the input and the friction-influenced speed.
            let speed = species.speed * walk.speed;
            accel = [-yaw.sin() * speed * speed, yaw.cos() * speed * speed];
        }
        k.velocity[0] = (k.velocity[0] + accel[0]) * GROUND_DRAG;
        k.velocity[2] = (k.velocity[2] + accel[1]) * GROUND_DRAG;
        let next = [k.body.feet[0] + k.velocity[0], k.body.feet[2] + k.velocity[2]];
        let here = k.body.feet[1].floor() as i32;
        let ground = ground_top(level, next[0], next[1], here, species.step_height);
        k.body.blocked = false;
        match ground {
            Some(top) if top - here > species.step_height => {
                // A cliff: it stands against it while the rampage crushes it.
                k.velocity[0] = 0.0;
                k.velocity[2] = 0.0;
                k.body.blocked = true;
            }
            _ => {
                k.body.feet[0] = next[0];
                k.body.feet[2] = next[1];
            }
        }
        let ground = ground_top(level, k.body.feet[0], k.body.feet[2], here, species.step_height).map(f64::from);
        match ground {
            Some(top) if top >= k.body.feet[1] - 0.001 => {
                k.body.feet[1] = top;
                k.velocity[1] = 0.0;
                k.on_ground = true;
            }
            // More than a step down under its middle, or nothing: a hole (a crater, a shaft, a
            // cave mouth) or a drop. Its body rests on whatever is under the rest of it, as a
            // mob's box does, so only a hole as wide as it swallows it.
            // (Only standing on a block's top, not part way through a fall.)
            ground
                if (k.body.feet[1] - k.body.feet[1].round()).abs() < 1e-6
                    && ground.is_none_or(|top| k.body.feet[1] - top > f64::from(species.step_height))
                    && footprint_holds(level, k.body.feet, species.width) =>
            {
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

    /// Walks an ape a tick (`Kong.strideTowards` and `followTerrain`): it turns towards where it
    /// is going a few degrees a tick, slowing while it turns, and wades through whatever is in
    /// the way, rising onto higher ground and dropping to lower at a limited rate. Mid-leap, its
    /// brain flies it. A heavy footfall every few strides.
    fn step_ape(k: &mut Kaiju, arena: &LevelArena, walk: Option<Walk>, out: &mut KaijuTick) {
        let species = k.brain.species;
        let Some(ape) = species.ape else { return };
        let before = k.body.feet;
        if let Some(walk) = walk {
            k.brain.turn_towards(&mut k.body, walk.to);
            let wanted = (walk.to[1] - k.body.feet[2]).atan2(walk.to[0] - k.body.feet[0]).to_degrees() - 90.0;
            let alignment = (wanted - f64::from(k.body.yaw)).to_radians().cos().max(0.0);
            let yaw = f64::from(k.body.yaw).to_radians();
            let speed = species.speed * walk.speed * alignment;
            k.body.feet[0] -= yaw.sin() * speed;
            k.body.feet[2] += yaw.cos() * speed;
        }
        if !k.brain.airborne() {
            let y = k.body.feet[1];
            let ground = kaiju::brain::ground_under(arena, species, k.body.feet[0], k.body.feet[2], y);
            k.body.feet[1] = match ground {
                Some(top) if top > y => top.min(y + ape.s(ape::CLIMB_PER_TICK)),
                Some(top) => top.max(y - ape.s(ape::DROP_PER_TICK)),
                None => (y - ape.s(ape::DROP_PER_TICK)).max(f64::from(arena.level.min_y())),
            };
            k.on_ground = ground.is_some_and(|top| (k.body.feet[1] - top).abs() < 1e-3);
        } else {
            k.on_ground = false;
        }
        // `LivingEntity.calculateEntityAnimation`: the distance moved drives the stride.
        let moved = (k.body.feet[0] - before[0]).hypot(k.body.feet[2] - before[2]);
        k.body.moving = moved > 0.01;
        k.walk_speed += ((moved as f32 * 4.0).min(1.0) - k.walk_speed) * 0.4;
        k.walk_position += k.walk_speed;
        if k.on_ground {
            k.stride += moved;
            if k.stride >= ape.s(ape::STRIDE_LENGTH) {
                k.stride = 0.0;
                let pitch = 0.675 * k.brain.voice_pitch() * species.move_pitch;
                out.sounds.push(sound(species.voice, "step", k.body.feet, 3.0 * ape.scale as f32, pitch));
            }
        }
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
        Effect::Sound { group, file, volume, pitch } => out.sounds.push(sound(group, file, k.body.feet, volume, pitch)),
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
        Effect::Shockwave { at, radius } => {
            // Its ring rolls along the ground (`Kong.shockwaveRing`), not through the air at the
            // height it started from.
            let arena = LevelArena { level, logs: level.registries().block_tags.id("minecraft:logs") };
            let points = ((radius * 2.5) as u32).clamp(16, 120);
            k.shown.shockwave = (0..points)
                .map(|i| {
                    let angle = std::f64::consts::TAU * f64::from(i) / f64::from(points);
                    let (x, z) = (at[0] + angle.cos() * radius, at[2] + angle.sin() * radius);
                    [x, arena.ground_at(x, z, at[1], 6, 12).unwrap_or(at[1]), z]
                })
                .collect();
        }
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

/// The top of the highest solid block under (x, z) from `near` + `step_height` down to 64
/// blocks below, if there is one.
fn ground_top(level: &Level<'static>, x: f64, z: f64, near: i32, step_height: i32) -> Option<i32> {
    let (bx, bz) = (x.floor() as i32, z.floor() as i32);
    let floor = level.min_y();
    let mut y = near + step_height + 1;
    while y >= (near - 64).max(floor) {
        if solid(level, BlockPos::new(bx, y, bz)) {
            return Some(y + 1);
        }
        y -= 1;
    }
    None
}

/// Whether a kaiju standing at `feet` rests on a block anywhere under its footprint, `width`
/// across: sampled every block or two, a little in from its edge.
fn footprint_holds(level: &Level<'static>, feet: [f64; 3], width: f64) -> bool {
    let layer = feet[1].round() as i32 - 1;
    let half = (width / 2.0 - 0.3).max(0.0);
    let steps = (width / 2.0).ceil().max(1.0) as i32;
    let along = |n: i32| -half + 2.0 * half * f64::from(n) / f64::from(steps);
    (0..=steps).any(|i| {
        (0..=steps).any(|j| {
            let (x, z) = (feet[0] + along(i), feet[2] + along(j));
            solid(level, BlockPos::new(x.floor() as i32, layer, z.floor() as i32))
        })
    })
}

fn solid(level: &Level<'static>, pos: BlockPos) -> bool {
    let state = level.block(pos);
    let blocks = &level.registries().blocks;
    !blocks.is_air(state) && !blocks.has_fluid(state) && blocks.state(state).collision_full_block
}

fn eye_of(body: &Body, species: &Species) -> [f64; 3] {
    [body.feet[0], body.feet[1] + species.eye_height, body.feet[2]]
}

fn leaves(level: &Level<'static>, pos: BlockPos) -> bool {
    let blocks = &level.registries().blocks;
    blocks.block(blocks.state(level.block(pos)).block).is_a("LeavesBlock")
}

/// The first solid block a line passes through (a voxel walk), as the point where it enters,
/// passing through whatever `see_through` says.
fn first_solid(level: &Level<'static>, from: [f64; 3], to: [f64; 3], see_through: impl Fn(BlockPos) -> bool) -> Option<[f64; 3]> {
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
        if solid(level, pos) && !see_through(pos) {
            return Some(p);
        }
    }
    None
}

struct LevelArena<'a> {
    level: &'a Level<'static>,
    /// `#minecraft:logs`, which the apes see and wade through.
    logs: Option<TagId>,
}

impl LevelArena<'_> {
    /// Trees and foliage, to an ape (`KongTerrain.isFoliage`): logs, leaves and bamboo.
    fn foliage(&self, pos: BlockPos) -> bool {
        let state = self.level.block(pos);
        let registries = self.level.registries();
        let blocks = &registries.blocks;
        let block = blocks.block(blocks.state(state).block);
        block.is_a("LeavesBlock") || block.is_a("BambooStalkBlock") || self.logs.is_some_and(|logs| registries.block_in_tag(state, logs))
    }

    /// Whether a kaiju sees `to`: through foliage but not walls (the mod's rule). Leaves don't
    /// block a kaiju's sight; nor do tree trunks or bamboo an ape's, out to its sight's range.
    fn sees(&self, body: &Body, species: &Species, to: [f64; 3]) -> bool {
        let from = eye_of(body, species);
        match species.ape {
            Some(ape) => {
                let far = ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2) + (to[2] - from[2]).powi(2)).sqrt();
                far <= ape.s(ape::SIGHT) && self.clip_ground(from, to).is_none()
            }
            None => first_solid(self.level, from, to, |pos| leaves(self.level, pos)).is_none(),
        }
    }
}

impl Arena for LevelArena<'_> {
    fn clip_blocks(&self, from: [f64; 3], to: [f64; 3]) -> Option<[f64; 3]> {
        first_solid(self.level, from, to, |_| false)
    }

    fn surface_y(&self, x: f64, z: f64, near_y: f64) -> f64 {
        // Warnings are drawn on the ground a few blocks above or below the kaiju's feet.
        ground_top(self.level, x, z, near_y.floor() as i32, 6).map_or(near_y, f64::from)
    }

    fn clip_ground(&self, from: [f64; 3], to: [f64; 3]) -> Option<[f64; 3]> {
        first_solid(self.level, from, to, |pos| self.foliage(pos))
    }

    /// `KongTerrain.surfaceAt`: the top of the highest block with a collision shape that isn't foliage.
    fn ground_at(&self, x: f64, z: f64, near_y: f64, above: i32, below: i32) -> Option<f64> {
        let (bx, bz) = (x.floor() as i32, z.floor() as i32);
        let top = near_y.floor() as i32 + above;
        let bottom = (near_y.floor() as i32 - below).max(self.level.min_y());
        let blocks = &self.level.registries().blocks;
        (bottom..=top).rev().find_map(|y| {
            let pos = BlockPos::new(bx, y, bz);
            let state = self.level.block(pos);
            if blocks.is_air(state) || self.foliage(pos) {
                return None;
            }
            let shape_top = blocks.collision_boxes(state).iter().map(|b| b[4]).fold(f64::NEG_INFINITY, f64::max);
            shape_top.is_finite().then(|| f64::from(y) + shape_top)
        })
    }
}

/// `zillacraft:entity.<group>.<file>`: a kaiju's voice, or a sound all kaiju share.
fn sound(group: &str, file: &str, at: [f64; 3], volume: f32, pitch: f32) -> MobSound {
    MobSound {
        event: format!("zillacraft:entity.{group}.{file}"),
        position: glam_dvec(at),
        volume,
        pitch,
        category: "hostile",
    }
}

fn glam_dvec(p: [f64; 3]) -> glam::DVec3 {
    glam::DVec3::new(p[0], p[1], p[2])
}
