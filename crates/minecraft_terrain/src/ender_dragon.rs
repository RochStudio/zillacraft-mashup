//! The Ender Dragon (`EnderDragon`, its phases, `DragonFireball` and the clouds of dragon's
//! breath), which the entity world doesn't have. Hatched from its spawn egg, it takes the spot
//! where it hatched as its fight's origin, as the End's dragon takes the exit portal, and does
//! as that one does: it flies its holding pattern round the spot along a graph of nodes, now and
//! then strafes a player with a fireball, lands to scan, roar and breathe fire, charges anyone
//! who keeps their distance, and smashes through whatever it flies into.
//!
//! The dragon faces the way vanilla's does: its yaw points behind it, and it flies along
//! (sin yaw, 0, -cos yaw).

use crate::kaiju_server::{KaijuTick, PLAYER_SHARE_MELEE, first_solid};
use kaiju::brain::Seen;
use minecraftoss_core::BlockPos;
use minecraftoss_core::chunk::HeightmapKind;
use minecraftoss_entities::world::{MobSound, PlayerHit, PlayerHitKind};
use minecraftoss_generator::feature::World;
use minecraftoss_world::level::Level;

/// Flight history samples the client needs: the model reads 24 back, and each is blended
/// with the one before it.
pub const HISTORY: usize = 26;
pub const MAX_HEALTH: f32 = 200.0;
/// `BossEvent.BossBarColor.PINK`.
pub const BOSS_COLOR: [f32; 3] = [0.93, 0.33, 0.83];
const RAD: f32 = std::f32::consts::PI / 180.0;
const TAU: f32 = std::f32::consts::TAU;
/// Its body parts (`EnderDragonPart`): head, neck, body, three of tail, two wings; width, height.
const PARTS: [(f64, f64); 8] = [(1.0, 1.0), (3.0, 3.0), (5.0, 3.0), (2.0, 2.0), (2.0, 2.0), (2.0, 2.0), (4.0, 2.0), (4.0, 2.0)];
const HEAD: usize = 0;
const NECK: usize = 1;
const BODY: usize = 2;
const WINGS: [usize; 2] = [6, 7];
const DEATH_TICKS: u32 = 200;
/// A fireball that never meets anything is gone after this long (vanilla's leaves loaded chunks).
const FIREBALL_LIFE: u32 = 300;

/// The id of a dragon's part `part`: vanilla gives part i the dragon's id + i + 1.
pub fn part_id(dragon: u64, part: usize) -> u64 {
    dragon + 1 + part as u64
}

/// Ids a dragon takes: its own and its parts'.
pub const IDS: u64 = 1 + PARTS.len() as u64;

/// What the client draws of a dragon this tick.
#[derive(Clone, Debug)]
pub struct DragonView {
    pub id: u64,
    pub position: [f64; 3],
    pub previous_position: [f64; 3],
    /// Wingbeat, in cycles (`flapTime`), and last tick's.
    pub flap: f32,
    pub flap_previous: f32,
    /// Its flight history (`DragonFlightHistory`), newest first: (Y, yaw in degrees).
    pub history: Vec<(f64, f32)>,
    /// Perched (`isSitting`): its neck curls down.
    pub sitting: bool,
    pub death_ticks: u32,
    pub hurt_ticks: u32,
    pub health: f32,
    pub max_health: f32,
    /// Its body parts' boxes (`EnderDragonPart`), for bullets.
    pub parts: Vec<[f64; 6]>,
    /// Its fireballs in flight, and the clouds of breath on the ground: (centre, radius).
    pub fireballs: Vec<(u32, [f64; 3])>,
    pub clouds: Vec<([f64; 3], f32)>,
}

/// A path along the node graph (`Path`): node positions, and the next to fly to.
#[derive(Clone, Debug, Default)]
struct Path {
    nodes: Vec<[i32; 3]>,
    next: usize,
}

impl Path {
    fn done(&self) -> bool {
        self.next >= self.nodes.len()
    }

    fn advance(&mut self) {
        self.next += 1;
    }

    fn next_node(&self) -> [i32; 3] {
        self.nodes[self.next]
    }
}

#[derive(Clone, Debug)]
enum Phase {
    HoldingPattern { path: Option<Path>, target: Option<[f64; 3]> },
    StrafePlayer { charge: u32, path: Option<Path>, target: Option<[f64; 3]>, attack: Option<u64> },
    LandingApproach { path: Option<Path>, target: Option<[f64; 3]> },
    Landing { target: Option<[f64; 3]> },
    Takeoff { first: bool, path: Option<Path>, target: Option<[f64; 3]> },
    SittingFlaming { ticks: u32 },
    SittingScanning { ticks: u32 },
    SittingAttacking { ticks: u32 },
    ChargingPlayer { target: [f64; 3], since: u32 },
    Dying { target: Option<[f64; 3]> },
}

impl Phase {
    /// Perched on the ground (`isSitting`): it doesn't fly, flaps slowly, deals no wing blows,
    /// and leaves after enough damage.
    fn sitting(&self) -> bool {
        matches!(self, Phase::SittingFlaming { .. } | Phase::SittingScanning { .. } | Phase::SittingAttacking { .. })
    }

    /// `getFlySpeed`: the steepest it climbs or dives.
    fn fly_speed(&self) -> f32 {
        match self {
            Phase::Landing { .. } => 1.5,
            Phase::ChargingPlayer { .. } | Phase::Dying { .. } => 3.0,
            _ => 0.6,
        }
    }

    fn target(&self) -> Option<[f64; 3]> {
        match self {
            Phase::HoldingPattern { target, .. }
            | Phase::StrafePlayer { target, .. }
            | Phase::LandingApproach { target, .. }
            | Phase::Landing { target }
            | Phase::Takeoff { target, .. }
            | Phase::Dying { target } => *target,
            Phase::ChargingPlayer { target, .. } => Some(*target),
            _ => None,
        }
    }

    fn same(&self, other: &Phase) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Clone, Debug)]
struct Fireball {
    id: u32,
    at: [f64; 3],
    velocity: [f64; 3],
    life: u32,
}

/// A cloud of dragon's breath (`AreaEffectCloud` with instant damage).
#[derive(Clone, Debug)]
struct Cloud {
    at: [f64; 3],
    radius: f32,
    growth: f32,
    duration: u32,
    age: u32,
    /// What each touch does (`HealOrHarmMobEffect`: 6 for a fireball's, 3 for the breath).
    damage: f32,
    /// Who it touched, and the tick it may again.
    touched: Vec<(u64, u32)>,
    /// The breath it breathes while perched, gone when that ends.
    breath: bool,
}

pub struct EnderDragon {
    id: u64,
    at: [f64; 3],
    previous: [f64; 3],
    velocity: [f64; 3],
    yaw: f32,
    /// `yRotA`: how fast it is turning.
    turning: f32,
    flap: f32,
    flap_previous: f32,
    /// `DragonFlightHistory`: (Y, yaw), newest first.
    history: std::collections::VecDeque<(f64, f32)>,
    health: f32,
    hurt_ticks: u32,
    /// A hit is waiting for its hurt sound.
    pending_hurt_sound: bool,
    death_ticks: u32,
    in_wall: bool,
    phase: Phase,
    /// The fight's origin: where it hatched.
    origin: [i32; 3],
    /// The node graph round the origin, built once (`findClosestNode`).
    nodes: Option<[[i32; 3]; 24]>,
    sitting_damage: f32,
    /// Kept across phases, as vanilla keeps each phase's instance.
    holding_clockwise: bool,
    strafe_clockwise: bool,
    flame_count: u32,
    parts: [[f64; 3]; 8],
    fireballs: Vec<Fireball>,
    clouds: Vec<Cloud>,
    next_fireball: u32,
    ambient_time: i32,
    growl_time: i32,
    tick_count: u32,
    rng: u64,
}

/// The nodes' places round the origin: an outer ring (unused without crystals), an inner ring
/// 15 blocks over the ground, and four in the middle 5 over (`findClosestNode`'s table).
const NODES: [[i32; 2]; 24] = [
    [60, 0], [51, 30], [29, 51], [0, 60], [-31, 51], [-52, 29], [-60, 0], [-52, -31], [-30, -52], [0, -60], [29, -52], [51, -30],
    [40, 0], [28, 28], [0, 40], [-29, 28], [-40, 0], [-29, -29], [0, -40], [28, -29], [20, 0], [0, 20], [-20, 0], [0, -20],
];
/// Which nodes each links to (`nodeAdjacency`).
const LINKS: [u32; 24] = [
    6146, 8197, 8202, 16404, 32808, 32848, 65696, 131392, 131712, 263424, 526848, 525313, 1581057, 3166214, 2138120, 6373424,
    4358208, 12910976, 9044480, 9706496, 15216640, 13688832, 11763712, 8257536,
];
/// With no crystals it keeps to the inner nodes.
const FIRST_NODE: usize = 12;

fn distance_sq(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-4 { [0.0; 3] } else { [v[0] / l, v[1] / l, v[2] / l] }
}

/// `Mth.wrapDegrees`.
fn wrap(degrees: f32) -> f32 {
    let mut r = degrees % 360.0;
    if r >= 180.0 {
        r -= 360.0;
    }
    if r < -180.0 {
        r += 360.0;
    }
    r
}

fn sound(event: &str, at: [f64; 3], volume: f32, pitch: f32) -> MobSound {
    MobSound { event: format!("minecraft:{event}"), position: glam::DVec3::from_array(at), volume, pitch, category: "hostile" }
}

impl EnderDragon {
    pub fn new(id: u64, at: [f64; 3], yaw: f32) -> Self {
        Self {
            id,
            at,
            previous: at,
            velocity: [0.0; 3],
            yaw,
            turning: 0.0,
            flap: 0.0,
            flap_previous: 0.0,
            history: std::collections::VecDeque::new(),
            health: MAX_HEALTH,
            hurt_ticks: 0,
            pending_hurt_sound: false,
            death_ticks: 0,
            in_wall: false,
            // As `EnderDragonFight.createNewDragon` starts the End's.
            phase: Phase::HoldingPattern { path: None, target: None },
            origin: [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32],
            nodes: None,
            sitting_damage: 0.0,
            holding_clockwise: false,
            strafe_clockwise: false,
            flame_count: 0,
            parts: [at; 8],
            fireballs: Vec::new(),
            clouds: Vec::new(),
            next_fireball: 0,
            ambient_time: 0,
            growl_time: 100,
            tick_count: 0,
            rng: (id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ at[0].to_bits() ^ at[2].to_bits().rotate_left(17)) | 1,
        }
    }

    fn random(&mut self) -> u64 {
        // xorshift64*
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        self.rng.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_float(&mut self) -> f32 {
        (self.random() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn next_int(&mut self, n: u32) -> u32 {
        (self.random() % u64::from(n.max(1))) as u32
    }

    /// Whether `id` is it or one of its parts.
    pub fn owns(&self, id: u64) -> bool {
        (self.id..self.id + IDS).contains(&id)
    }

    /// Gone once it has died (200 ticks after its last breath).
    pub fn gone(&self) -> bool {
        self.death_ticks >= DEATH_TICKS
    }

    fn part_box(&self, part: usize) -> [f64; 6] {
        let (w, h) = PARTS[part];
        let p = self.parts[part];
        [p[0] - w / 2.0, p[1], p[2] - w / 2.0, p[0] + w / 2.0, p[1] + h, p[2] + w / 2.0]
    }

    pub fn view(&self) -> DragonView {
        DragonView {
            id: self.id,
            position: self.at,
            previous_position: self.previous,
            flap: self.flap,
            flap_previous: self.flap_previous,
            history: self.history.iter().take(HISTORY).copied().collect(),
            sitting: self.phase.sitting(),
            death_ticks: self.death_ticks,
            hurt_ticks: self.hurt_ticks,
            health: self.health.max(0.0),
            max_health: MAX_HEALTH,
            parts: (0..PARTS.len()).map(|i| self.part_box(i)).collect(),
            fireballs: self.fireballs.iter().map(|f| (f.id, f.at)).collect(),
            clouds: self.clouds.iter().map(|c| (c.at, c.radius)).collect(),
        }
    }

    fn sample(&self, index: usize) -> (f64, f32) {
        self.history.get(index).or(self.history.back()).copied().unwrap_or((self.at[1], self.yaw))
    }

    // -- damage ----------------------------------------------------------------------------------

    /// A player's hit on its part `id` for `damage` Minecraft health (`EnderDragon.hurt` with a
    /// part): the head and neck take it all, the rest a quarter and one.
    pub fn hurt(&mut self, id: u64, damage: f32, _attacker: u64) {
        if matches!(self.phase, Phase::Dying { .. }) || self.health <= 0.0 {
            return;
        }
        let part = id.saturating_sub(self.id + 1) as usize;
        let damage = if part == HEAD || part == NECK || id == self.id { damage } else { damage / 4.0 + damage.min(1.0) };
        if damage < 0.01 {
            return;
        }
        let before = self.health;
        self.health -= damage;
        self.hurt_ticks = 10;
        self.ambient_time = -80;
        self.pending_hurt_sound = true;
        if self.phase.sitting() {
            self.sitting_damage += before - self.health.max(0.0);
            if self.sitting_damage > 0.25 * MAX_HEALTH {
                self.sitting_damage = 0.0;
                self.set_phase(Phase::Takeoff { first: true, path: None, target: None });
            }
        }
        // `handleKillingBlow`: in the air it flies home to die; perched, it dies where it is.
        if self.health <= 0.0 && !self.phase.sitting() {
            self.health = 1.0;
            self.set_phase(Phase::Dying { target: None });
        }
    }
}

/// A player's box, for the dragon's blows and clouds.
fn player_box(s: &Seen) -> [f64; 6] {
    let half = s.width / 2.0;
    [s.feet[0] - half, s.feet[1], s.feet[2] - half, s.feet[0] + half, s.feet[1] + s.height, s.feet[2] + half]
}

fn overlaps(a: [f64; 6], b: [f64; 6]) -> bool {
    (0..3).all(|i| a[i] < b[i + 3] && a[i + 3] > b[i])
}

fn inflate(b: [f64; 6], x: f64, y: f64, z: f64) -> [f64; 6] {
    [b[0] - x, b[1] - y, b[2] - z, b[3] + x, b[4] + y, b[5] + z]
}

/// Where a segment from `a` to `b` first enters box `bx`, as a fraction of the way.
fn clip(bx: [f64; 6], a: [f64; 3], b: [f64; 3]) -> Option<f64> {
    kaiju::hitboxes::Aabb { min: [bx[0], bx[1], bx[2]], max: [bx[3], bx[4], bx[5]] }.clip(a, b)
}

impl EnderDragon {
    fn voice_pitch(&mut self) -> f32 {
        (self.next_float() - self.next_float()) * 0.2 + 1.0
    }

    fn hit(&self, s: &Seen, damage: f32, from: [f64; 3], out: &mut KaijuTick) {
        let kind = PlayerHitKind::Melee { attacker: glam::DVec3::from_array(from), hunger_ticks: 0, lift: 0.2 };
        out.player_hits.push(PlayerHit { player_id: s.id, damage: damage * PLAYER_SHARE_MELEE, kind, source: Some(self.id) });
    }

    /// One server tick (`EnderDragon.aiStep`, or `tickDeath` once it has died).
    pub fn tick(&mut self, level: &mut Level<'static>, seen: &[Seen], griefing: bool, out: &mut KaijuTick) {
        self.tick_count += 1;
        self.previous = self.at;
        self.hurt_ticks = self.hurt_ticks.saturating_sub(1);
        if std::mem::take(&mut self.pending_hurt_sound) && self.health > 0.0 {
            let pitch = self.voice_pitch();
            out.sounds.push(sound("entity.ender_dragon.hurt", self.at, 5.0, pitch));
        }
        // What it threw keeps flying whatever becomes of it.
        self.tick_fireballs(level, seen, out);
        self.tick_clouds(seen, out);
        if self.health <= 0.0 {
            self.tick_death(out);
            return;
        }
        // `Mob.baseTick`'s ambient sound, `processFlappingMovement`, and the growl vanilla plays on
        // the client.
        let roll = self.next_int(1000) as i32;
        self.ambient_time += 1;
        if roll < self.ambient_time - 1 {
            self.ambient_time = -80;
            let pitch = self.voice_pitch();
            out.sounds.push(sound("entity.ender_dragon.ambient", self.at, 5.0, pitch));
        }
        if (self.flap_previous * TAU).cos() <= -0.3 && (self.flap * TAU).cos() >= -0.3 {
            let pitch = 0.8 + self.next_float() * 0.3;
            out.sounds.push(sound("entity.ender_dragon.flap", self.at, 5.0, pitch));
        }
        if !self.phase.sitting() {
            self.growl_time -= 1;
            if self.growl_time < 0 {
                let pitch = 0.8 + self.next_float() * 0.3;
                out.sounds.push(sound("entity.ender_dragon.growl", self.at, 2.5, pitch));
                self.growl_time = 200 + self.next_int(200) as i32;
            }
        }

        // The wingbeat: faster the slower it flies, slower climbing, slow when perched.
        self.flap_previous = self.flap;
        let horizontal = self.velocity[0].hypot(self.velocity[2]) as f32;
        let beat = 0.2 / (horizontal * 10.0 + 1.0) * 2f32.powf(self.velocity[1] as f32);
        self.flap += if self.phase.sitting() { 0.1 } else if self.in_wall { beat * 0.5 } else { beat };
        self.yaw = wrap(self.yaw);
        self.history.push_front((self.at[1], self.yaw));
        self.history.truncate(64);

        // Its phase, and a new phase one tick of its own at once.
        if let Some(next) = self.phase_tick(level, seen, out) {
            self.set_phase(next);
            if let Some(next) = self.phase_tick(level, seen, out) {
                self.set_phase(next);
            }
        }
        if let Some(target) = self.phase.target() {
            self.fly_towards(target);
        }
        self.tick_parts(level, seen, griefing, out);
    }

    /// Steering towards `target` (`aiStep`): it climbs or dives no steeper than its phase
    /// allows, turns a little each tick, speeds up when facing the target and slows when not.
    fn fly_towards(&mut self, target: [f64; 3]) {
        let (dx, dz) = (target[0] - self.at[0], target[2] - self.at[2]);
        let mut dy = target[1] - self.at[1];
        let distance_sq = dx * dx + dy * dy + dz * dz;
        let fly = f64::from(self.phase.fly_speed());
        let horizontal = dx.hypot(dz);
        if horizontal > 0.0 {
            dy = (dy / horizontal).clamp(-fly, fly);
        }
        self.velocity[1] += dy * 0.01;
        self.yaw = wrap(self.yaw);
        let to_target = normalize([dx, target[1] - self.at[1], dz]);
        let yaw = self.yaw * RAD;
        let forward = normalize([f64::from(yaw.sin()), self.velocity[1], f64::from(-yaw.cos())]);
        let dot = forward[0] * to_target[0] + forward[1] * to_target[1] + forward[2] * to_target[2];
        let align = ((dot as f32 + 0.5) / 1.5).max(0.0);
        if dx.abs() > 1e-5 || dz.abs() > 1e-5 {
            let turn = wrap(180.0 - dx.atan2(dz).to_degrees() as f32 - self.yaw).clamp(-50.0, 50.0);
            let speed = self.velocity[0].hypot(self.velocity[2]) as f32 + 1.0;
            let turn_speed = if matches!(self.phase, Phase::Landing { .. }) { speed.min(40.0) / speed } else { 0.7 / speed.min(40.0) / speed };
            self.turning = self.turning * 0.8 + turn * turn_speed;
            self.yaw += self.turning * 0.1;
        }
        let closeness = (2.0 / (distance_sq + 1.0)) as f32;
        let accel = f64::from(0.06 * (align * closeness + (1.0 - closeness)));
        let yaw = self.yaw * RAD;
        self.velocity[0] += accel * f64::from(yaw.sin());
        self.velocity[2] -= accel * f64::from(yaw.cos());
        let step = if self.in_wall { 0.8 } else { 1.0 };
        for i in 0..3 {
            self.at[i] += self.velocity[i] * step;
        }
        let along = normalize(self.velocity);
        let drag = 0.8 + 0.15 * (along[0] * forward[0] + along[1] * forward[1] + along[2] * forward[2] + 1.0) / 2.0;
        self.velocity = [self.velocity[0] * drag, self.velocity[1] * 0.91, self.velocity[2] * drag];
    }

    /// Its parts follow its body and flight history (`tickPart`); its wings bat aside and hurt
    /// what they touch, its jaws bite, and its head, neck and body smash through blocks.
    fn tick_parts(&mut self, level: &mut Level<'static>, seen: &[Seen], griefing: bool, out: &mut KaijuTick) {
        let (s0, s5, s10) = (self.sample(0), self.sample(5), self.sample(10));
        let pitch = (s5.0 - s10.0) as f32 * 10.0 * RAD;
        let (cp, sp) = (f64::from(pitch.cos()), f64::from(pitch.sin()));
        let yaw = self.yaw * RAD;
        let (sy, cy) = (f64::from(yaw.sin()), f64::from(yaw.cos()));
        let at = self.at;
        let place = |dx: f64, dy: f64, dz: f64| [at[0] + dx, at[1] + dy, at[2] + dz];
        self.parts[BODY] = place(sy * 0.5, 0.0, -cy * 0.5);
        self.parts[WINGS[0]] = place(cy * 4.5, 2.0, sy * 4.5);
        self.parts[WINGS[1]] = place(cy * -4.5, 2.0, sy * -4.5);
        if self.hurt_ticks == 0 {
            let body = self.part_box(BODY);
            let centre = [(body[0] + body[3]) / 2.0, body[1], (body[2] + body[5]) / 2.0];
            for wing in WINGS {
                let reach = inflate(self.part_box(wing), 4.0, 2.0, 4.0);
                let reach = [reach[0], reach[1] - 2.0, reach[2], reach[3], reach[4] - 2.0, reach[5]];
                for s in seen.iter().filter(|s| s.targetable && overlaps(reach, player_box(s))) {
                    if !self.phase.sitting() {
                        self.hit(s, 5.0, centre, out);
                    }
                }
            }
            // Its jaws, where its head and neck were last tick.
            for part in [HEAD, NECK] {
                let reach = inflate(self.part_box(part), 1.0, 1.0, 1.0);
                for s in seen.iter().filter(|s| s.targetable && overlaps(reach, player_box(s))) {
                    self.hit(s, 10.0, self.parts[HEAD], out);
                }
            }
        }
        let head_yaw = yaw - self.turning * 0.01;
        let (hs, hc) = (f64::from(head_yaw.sin()), f64::from(head_yaw.cos()));
        let head_y = if self.phase.sitting() { -1.0 } else { s5.0 - s0.0 };
        self.parts[HEAD] = place(hs * 6.5 * cp, head_y + sp * 6.5, -hc * 6.5 * cp);
        self.parts[NECK] = place(hs * 5.5 * cp, head_y + sp * 5.5, -hc * 5.5 * cp);
        for i in 0..3 {
            let si = self.sample(12 + i * 2);
            let a = yaw + wrap(si.1 - s5.1) * RAD;
            let (sa, ca) = (f64::from(a.sin()), f64::from(a.cos()));
            let k = (i as f64 + 1.0) * 2.0;
            self.parts[3 + i] = place(-(sy * 1.5 + sa * k) * cp, si.0 - s5.0 - (k + 1.5) * sp + 1.5, (cy * 1.5 + ca * k) * cp);
        }
        let walls = [HEAD, NECK, BODY].map(|part| self.check_walls(level, self.part_box(part), griefing));
        self.in_wall = walls.iter().any(|&w| w);
    }

    /// `checkWalls`: it smashes (without drops) whatever its box overlaps, unless mobs mayn't
    /// grief or the block is one it can't break, which stops it like a wall.
    fn check_walls(&self, level: &mut Level<'static>, b: [f64; 6], griefing: bool) -> bool {
        let tags = &level.registries().block_tags;
        let (immune, transparent) = (tags.id("minecraft:dragon_immune"), tags.id("minecraft:dragon_transparent"));
        let mut wall = false;
        for x in b[0].floor() as i32..=b[3].floor() as i32 {
            for y in b[1].floor() as i32..=b[4].floor() as i32 {
                for z in b[2].floor() as i32..=b[5].floor() as i32 {
                    let pos = BlockPos::new(x, y, z);
                    let state = level.block(pos);
                    let registries = level.registries();
                    let tagged = |tag: Option<minecraftoss_core::tags::TagId>| tag.is_some_and(|t| registries.block_in_tag(state, t));
                    if registries.blocks.is_air(state) || tagged(transparent) {
                        continue;
                    }
                    if !griefing || tagged(immune) {
                        wall = true;
                    } else {
                        level.remove_block(pos, false);
                    }
                }
            }
        }
        wall
    }

    /// `tickDeath`: it rises 20 blocks over ten seconds, letting go of its experience at the
    /// end, then it's gone.
    fn tick_death(&mut self, out: &mut KaijuTick) {
        self.death_ticks += 1;
        if self.death_ticks > 150 && self.death_ticks.is_multiple_of(5) {
            out.experience.push((self.at, 40));
        }
        if self.death_ticks == 1 {
            out.sounds.push(sound("entity.ender_dragon.death", self.at, 5.0, 1.0));
        }
        self.at[1] += 0.1;
        for part in &mut self.parts {
            part[1] += 0.1;
        }
        if self.death_ticks >= DEATH_TICKS {
            out.experience.push((self.at, 100));
        }
    }

    // -- phases ----------------------------------------------------------------------------------

    /// `setPhase`: the old phase ends (a perched breath goes with it), the new one begins.
    fn set_phase(&mut self, next: Phase) {
        if self.phase.same(&next) {
            return;
        }
        if matches!(self.phase, Phase::SittingFlaming { .. }) {
            self.clouds.retain(|c| !c.breath);
        }
        if matches!(next, Phase::SittingFlaming { .. }) {
            self.flame_count += 1;
        }
        self.phase = next;
    }

    /// The phase's server tick (`doServerTick`), and the phase it hands on to, if it does.
    fn phase_tick(&mut self, level: &mut Level<'static>, seen: &[Seen], out: &mut KaijuTick) -> Option<Phase> {
        let mut phase = std::mem::replace(&mut self.phase, Phase::Dying { target: None });
        let next = match &mut phase {
            Phase::HoldingPattern { path, target } => {
                let d = target.map_or(0.0, |t| distance_sq(t, self.at));
                if d < 100.0 || d > 22500.0 { self.holding_new_target(level, seen, path, target) } else { None }
            }
            Phase::StrafePlayer { charge, path, target, attack } => self.strafe_tick(level, seen, out, charge, path, target, *attack),
            Phase::LandingApproach { path, target } => {
                let d = target.map_or(0.0, |t| distance_sq(t, self.at));
                if d < 100.0 || d > 22500.0 { self.approach_new_target(level, seen, path, target) } else { None }
            }
            Phase::Landing { target } => {
                let podium = self.podium(level, HeightmapKind::MotionBlockingNoLeaves);
                let t = *target.get_or_insert([f64::from(podium[0]) + 0.5, f64::from(podium[1]), f64::from(podium[2]) + 0.5]);
                (distance_sq(t, self.at) < 1.0).then(|| {
                    self.flame_count = 0;
                    Phase::SittingScanning { ticks: 0 }
                })
            }
            Phase::Takeoff { first, path, target } => {
                if *first || path.is_none() {
                    *first = false;
                    self.takeoff_new_target(level, path, target);
                    None
                } else {
                    let podium = self.podium(level, HeightmapKind::MotionBlockingNoLeaves);
                    let centre = [f64::from(podium[0]) + 0.5, f64::from(podium[1]) + 0.5, f64::from(podium[2]) + 0.5];
                    (distance_sq(centre, self.at) >= 100.0).then_some(Phase::HoldingPattern { path: None, target: None })
                }
            }
            Phase::SittingScanning { ticks } => self.scan_tick(level, seen, ticks),
            Phase::SittingAttacking { ticks } => {
                // The roar: it growls on and on (vanilla every client tick), then breathes fire.
                if ticks.is_multiple_of(10) {
                    let pitch = 0.8 + self.next_float() * 0.3;
                    out.sounds.push(sound("entity.ender_dragon.growl", self.at, 2.5, pitch));
                }
                let before = *ticks;
                *ticks += 1;
                (before >= 40).then_some(Phase::SittingFlaming { ticks: 0 })
            }
            Phase::SittingFlaming { ticks } => {
                *ticks += 1;
                if *ticks >= 200 {
                    Some(if self.flame_count >= 4 { Phase::Takeoff { first: true, path: None, target: None } } else { Phase::SittingScanning { ticks: 0 } })
                } else {
                    if *ticks == 10 {
                        self.breathe(level);
                    }
                    None
                }
            }
            Phase::ChargingPlayer { target, since } => {
                let mut next = None;
                if *since > 0 {
                    let before = *since;
                    *since += 1;
                    if before >= 10 {
                        next = Some(Phase::HoldingPattern { path: None, target: None });
                    }
                }
                let d = distance_sq(*target, self.at);
                if next.is_none() && (d < 100.0 || d > 22500.0) {
                    *since += 1;
                }
                next
            }
            Phase::Dying { target } => {
                let podium = self.podium(level, HeightmapKind::MotionBlocking);
                let t = *target.get_or_insert([f64::from(podium[0]) + 0.5, f64::from(podium[1]), f64::from(podium[2]) + 0.5]);
                let d = distance_sq(t, self.at);
                self.health = if !(100.0..=22500.0).contains(&d) { 0.0 } else { 1.0 };
                None
            }
        };
        self.phase = phase;
        next
    }

    /// The top of the ground at the fight's origin (`getHeightmapPos` at the podium).
    fn podium(&self, level: &Level<'static>, kind: HeightmapKind) -> [i32; 3] {
        [self.origin[0], World::height_at(level, kind, self.origin[0], self.origin[2]), self.origin[2]]
    }

    /// The nearest targetable player to `from` that `keep` lets through.
    fn nearest(seen: &[Seen], from: [f64; 3], keep: impl Fn(&Seen) -> bool) -> Option<&Seen> {
        seen.iter()
            .filter(|s| s.targetable && keep(s))
            .min_by(|a, b| distance_sq(a.feet, from).total_cmp(&distance_sq(b.feet, from)))
    }

    /// Whether it sees `s` from its eye (`hasLineOfSight`), within 128 blocks.
    fn sees(&self, level: &Level<'static>, s: &Seen) -> bool {
        let eye = [self.at[0], self.at[1] + 6.8, self.at[2]];
        distance_sq(eye, s.eyes()) <= 128.0 * 128.0 && first_solid(level, eye, s.eyes(), |_| false).is_none()
    }

    /// HoldingPattern's `findNewTarget`: at the end of each leg it may land or go after a
    /// player; otherwise on round the inner ring, now and then turning back.
    fn holding_new_target(&mut self, level: &Level<'static>, seen: &[Seen], path: &mut Option<Path>, target: &mut Option<[f64; 3]>) -> Option<Phase> {
        if path.as_ref().is_some_and(Path::done) {
            if self.next_int(3) == 0 {
                return Some(Phase::LandingApproach { path: None, target: None });
            }
            let podium = self.podium(level, HeightmapKind::MotionBlockingNoLeaves);
            let centre = [f64::from(podium[0]) + 0.5, f64::from(podium[1]) + 0.5, f64::from(podium[2]) + 0.5];
            if let Some(p) = Self::nearest(seen, centre, |_| true) {
                let far = distance_sq(centre, p.feet) / 512.0;
                if self.next_int((far + 2.0) as u32) == 0 || self.next_int(2) == 0 {
                    return Some(self.strafe(level, p));
                }
            }
        }
        if path.as_ref().is_none_or(Path::done) {
            let clockwise = self.holding_clockwise;
            *path = self.lap(level, clockwise).map(|(p, cw)| {
                self.holding_clockwise = cw;
                p
            });
        }
        *target = self.navigate(path).or(*target);
        None
    }

    /// The next leg of a lap round the inner ring, from the node nearest it: a path, and which
    /// way round it went.
    fn lap(&mut self, level: &Level<'static>, mut clockwise: bool) -> Option<(Path, bool)> {
        let i = self.closest_node(level, self.at);
        let mut j = i as i32;
        if self.next_int(8) == 0 {
            clockwise = !clockwise;
            j += 6;
        }
        j += if clockwise { 1 } else { -1 };
        j = ((j - 12) & 7) + 12;
        let mut path = self.find_path(level, i, j as usize, None)?;
        path.advance();
        Some((path, clockwise))
    }

    /// `navigateToNextPathNode`: the next node, somewhere up to 20 blocks above it.
    fn navigate(&mut self, path: &mut Option<Path>) -> Option<[f64; 3]> {
        let path = path.as_mut().filter(|p| !p.done())?;
        let n = path.next_node();
        path.advance();
        let y = f64::from(n[1]) + f64::from(self.next_float() * 20.0);
        Some([f64::from(n[0]), y, f64::from(n[2])])
    }

    /// StrafePlayer begun at `p` (`setTarget`): a path to just above them.
    fn strafe(&mut self, level: &Level<'static>, p: &Seen) -> Phase {
        let i = self.closest_node(level, self.at);
        let j = self.closest_node(level, p.feet);
        let (bx, bz) = (p.feet[0].floor() as i32, p.feet[2].floor() as i32);
        let h = (f64::from(bx) - self.at[0]).hypot(f64::from(bz) - self.at[2]);
        let lift = (0.4 + h / 80.0 - 1.0).min(10.0);
        let mut path = self.find_path(level, i, j, Some([bx, (p.feet[1] + lift).floor() as i32, bz]));
        let mut target = None;
        if let Some(path) = path.as_mut() {
            path.advance();
        }
        if path.is_some() {
            target = self.navigate(&mut path);
        }
        Phase::StrafePlayer { charge: 0, path, target, attack: Some(p.id) }
    }

    /// StrafePlayer's tick: it dives at its target, and once it has had it in sight within 64
    /// blocks for a quarter second and faces it, it spits a fireball and goes back to circling.
    #[allow(clippy::too_many_arguments)]
    fn strafe_tick(
        &mut self,
        level: &Level<'static>,
        seen: &[Seen],
        out: &mut KaijuTick,
        charge: &mut u32,
        path: &mut Option<Path>,
        target: &mut Option<[f64; 3]>,
        attack: Option<u64>,
    ) -> Option<Phase> {
        let Some(p) = attack.and_then(|id| seen.iter().find(|s| s.id == id)) else {
            return Some(Phase::HoldingPattern { path: None, target: None });
        };
        if path.as_ref().is_some_and(Path::done) {
            let h = (p.feet[0] - self.at[0]).hypot(p.feet[2] - self.at[2]);
            *target = Some([p.feet[0], p.feet[1] + (0.4 + h / 80.0 - 1.0).min(10.0), p.feet[2]]);
        }
        let d = target.map_or(0.0, |t| distance_sq(t, self.at));
        if d < 100.0 || d > 22500.0 {
            if path.as_ref().is_none_or(Path::done) {
                let clockwise = self.strafe_clockwise;
                *path = self.lap(level, clockwise).map(|(p, cw)| {
                    self.strafe_clockwise = cw;
                    p
                });
            }
            *target = self.navigate(path).or(*target);
        }
        if distance_sq(p.feet, self.at) < 4096.0 && self.sees(level, p) {
            *charge += 1;
            let to = normalize([p.feet[0] - self.at[0], 0.0, p.feet[2] - self.at[2]]);
            let yaw = self.yaw * RAD;
            let facing = to[0] * f64::from(yaw.sin()) - to[2] * f64::from(yaw.cos());
            let angle = facing.clamp(-1.0, 1.0).acos().to_degrees() as f32 + 0.5;
            if *charge >= 5 && (0.0..10.0).contains(&angle) {
                let head = self.parts[HEAD];
                let from = [head[0] + f64::from(yaw.sin()), head[1] + 1.0, head[2] - f64::from(yaw.cos())];
                let aim = normalize([p.feet[0] - from[0], p.feet[1] + p.height / 2.0 - from[1], p.feet[2] - from[2]]);
                let pitch = self.voice_pitch();
                out.sounds.push(sound("entity.ender_dragon.shoot", self.at, 10.0, pitch));
                self.next_fireball += 1;
                self.fireballs.push(Fireball { id: self.next_fireball, at: from, velocity: aim.map(|c| c * 0.1), life: 0 });
                return Some(Phase::HoldingPattern { path: None, target: None });
            }
        } else if *charge > 0 {
            *charge -= 1;
        }
        None
    }

    /// LandingApproach's `findNewTarget`: round to the side of the origin away from the nearest
    /// player, then down onto it.
    fn approach_new_target(&mut self, level: &Level<'static>, seen: &[Seen], path: &mut Option<Path>, target: &mut Option<[f64; 3]>) -> Option<Phase> {
        if path.as_ref().is_none_or(Path::done) {
            let i = self.closest_node(level, self.at);
            let podium = self.podium(level, HeightmapKind::MotionBlockingNoLeaves);
            let centre = [f64::from(podium[0]) + 0.5, f64::from(podium[1]) + 0.5, f64::from(podium[2]) + 0.5];
            let o = [f64::from(self.origin[0]), f64::from(self.origin[2])];
            let j = match Self::nearest(seen, centre, |_| true) {
                Some(p) => {
                    let v = normalize([p.feet[0] - o[0], 0.0, p.feet[2] - o[1]]);
                    self.closest_node(level, [o[0] - v[0] * 40.0, 105.0, o[1] - v[2] * 40.0])
                }
                None => self.closest_node(level, [o[0] + 40.0, f64::from(podium[1]), o[1]]),
            };
            *path = self.find_path(level, i, j, Some(podium));
            if let Some(path) = path.as_mut() {
                path.advance();
            }
        }
        *target = self.navigate(path).or(*target);
        path.as_ref().is_some_and(Path::done).then_some(Phase::Landing { target: None })
    }

    /// Takeoff's `findNewTarget`: off the way its head looks, up to the inner ring.
    fn takeoff_new_target(&mut self, level: &Level<'static>, path: &mut Option<Path>, target: &mut Option<[f64; 3]>) {
        let i = self.closest_node(level, self.at);
        let podium = self.podium(level, HeightmapKind::MotionBlockingNoLeaves);
        let centre = [f64::from(podium[0]) + 0.5, f64::from(podium[1]) + 0.5, f64::from(podium[2]) + 0.5];
        let lift = 6.0 / (distance_sq(centre, self.at).sqrt() as f32 / 4.0).max(1.0);
        let pitch = -lift * 1.5 * 5.0 * RAD;
        let yaw = self.yaw * RAD;
        // Its head looks along minus its view vector.
        let look = [f64::from(yaw.sin() * pitch.cos()), f64::from(yaw.cos() * pitch.cos())];
        let o = [f64::from(self.origin[0]), f64::from(self.origin[2])];
        let j = self.closest_node(level, [o[0] + look[0] * 40.0, 105.0, o[1] - look[1] * 40.0]) as i32;
        let j = ((j - 12) & 7) + 12;
        *path = self.find_path(level, i, j as usize, None);
        if let Some(p) = path.as_mut() {
            p.advance();
        }
        *target = self.navigate(path).or(*target);
    }

    /// SittingScanning: it turns to face a player close by and roars at them, or with nobody
    /// near for five seconds takes off, charging whoever it sees within 150 blocks.
    fn scan_tick(&mut self, level: &Level<'static>, seen: &[Seen], ticks: &mut u32) -> Option<Phase> {
        *ticks += 1;
        let at = self.at;
        let close = Self::nearest(seen, at, |s| distance_sq(s.feet, at) <= 400.0 && (s.feet[1] - at[1]).abs() <= 10.0 && self.sees(level, s));
        if let Some(p) = close {
            if *ticks > 25 {
                return Some(Phase::SittingAttacking { ticks: 0 });
            }
            let to = normalize([p.feet[0] - at[0], 0.0, p.feet[2] - at[2]]);
            let yaw = self.yaw * RAD;
            let facing = to[0] * f64::from(yaw.sin()) - to[2] * f64::from(yaw.cos());
            let angle = facing.clamp(-1.0, 1.0).acos().to_degrees() as f32 + 0.5;
            if !(0.0..=10.0).contains(&angle) {
                let head = self.parts[HEAD];
                let (dx, dz) = (p.feet[0] - head[0], p.feet[2] - head[2]);
                let turn = wrap(180.0 - dx.atan2(dz).to_degrees() as f32 - self.yaw).clamp(-100.0, 100.0);
                let g = dx.hypot(dz) as f32 + 1.0;
                self.turning = self.turning * 0.8 + turn * (0.7 / g.min(40.0) / g);
                self.yaw += self.turning;
            }
            None
        } else if *ticks >= 100 {
            Some(match Self::nearest(seen, at, |s| distance_sq(s.feet, at) <= 150.0 * 150.0 && self.sees(level, s)) {
                Some(c) => Phase::ChargingPlayer { target: c.feet, since: 0 },
                None => Phase::Takeoff { first: true, path: None, target: None },
            })
        } else {
            None
        }
    }

    /// SittingFlaming's breath: a cloud on the ground two and a half blocks beyond its head.
    fn breathe(&mut self, level: &Level<'static>) {
        let head = self.parts[HEAD];
        let dir = normalize([head[0] - self.at[0], 0.0, head[2] - self.at[2]]);
        let (x, z) = (head[0] + dir[0] * 2.5, head[2] + dir[2] * 2.5);
        let head_y = head[1] + 0.5;
        let mut y = head_y;
        let blocks = &level.registries().blocks;
        while blocks.is_air(level.block(BlockPos::new(x.floor() as i32, y.floor() as i32, z.floor() as i32))) {
            y -= 1.0;
            if y < f64::from(level.min_y()) {
                y = head_y;
                break;
            }
        }
        let at = [x, y.floor() + 1.0, z];
        self.clouds.push(Cloud { at, radius: 5.0, growth: 0.0, duration: 200, age: 0, damage: 3.0, touched: Vec::new(), breath: true });
    }

    // -- the node graph --------------------------------------------------------------------------

    /// The nodes round the origin, each 5 or 15 blocks over the ground there (never below 73),
    /// fixed once built.
    fn nodes(&mut self, level: &Level<'static>) -> [[i32; 3]; 24] {
        let origin = self.origin;
        *self.nodes.get_or_insert_with(|| {
            std::array::from_fn(|i| {
                let (x, z) = (origin[0] + NODES[i][0], origin[2] + NODES[i][1]);
                let lift = if (12..20).contains(&i) { 15 } else { 5 };
                [x, (World::height_at(level, HeightmapKind::MotionBlockingNoLeaves, x, z) + lift).max(73), z]
            })
        })
    }

    /// `findClosestNode`: the inner node nearest `p`, or node 0 if none is within 100 blocks.
    fn closest_node(&mut self, level: &Level<'static>, p: [f64; 3]) -> usize {
        let nodes = self.nodes(level);
        let p = p.map(|c| c.floor());
        let mut best = (10000.0, 0);
        for (i, n) in nodes.iter().enumerate().skip(FIRST_NODE) {
            let d = distance_sq(n.map(f64::from), p);
            if d < best.0 {
                best = (d, i);
            }
        }
        best.1
    }

    /// `findPath`: A* along the graph's links from `from` to `to`, and on to `last` if given;
    /// failing that, as far towards `to` as it gets.
    fn find_path(&mut self, level: &Level<'static>, from: usize, to: usize, last: Option<[i32; 3]>) -> Option<Path> {
        let nodes = self.nodes(level);
        let dist = |a: usize, b: usize| distance_sq(nodes[a].map(f64::from), nodes[b].map(f64::from)).sqrt();
        let mut g = [f64::MAX; 24];
        let mut came = [usize::MAX; 24];
        let mut closed = [false; 24];
        let mut open = vec![from];
        g[from] = 0.0;
        let mut best = from;
        let mut reached = None;
        while let Some(k) = (0..open.len()).min_by(|&a, &b| (g[open[a]] + dist(open[a], to)).total_cmp(&(g[open[b]] + dist(open[b], to)))) {
            let c = open.swap_remove(k);
            if c == to {
                reached = Some(to);
                break;
            }
            if dist(c, to) < dist(best, to) {
                best = c;
            }
            closed[c] = true;
            for j in FIRST_NODE..24 {
                if LINKS[c] & (1 << j) == 0 || closed[j] {
                    continue;
                }
                let cost = g[c] + dist(c, j);
                if cost < g[j] {
                    came[j] = c;
                    g[j] = cost;
                    if !open.contains(&j) {
                        open.push(j);
                    }
                }
            }
        }
        let end = match reached {
            Some(end) => end,
            None if best == from => return None,
            None => best,
        };
        let mut chain = vec![end];
        while let Some(&c) = chain.last().filter(|&&c| c != from) {
            chain.push(came[c]);
        }
        chain.reverse();
        let mut path: Vec<[i32; 3]> = chain.into_iter().map(|i| nodes[i]).collect();
        path.extend(last);
        Some(Path { nodes: path, next: 0 })
    }

    // -- fireballs and clouds --------------------------------------------------------------------

    /// `DragonFireball`: it speeds up along its line (to nearly two blocks a tick) and bursts on
    /// whatever it meets into a cloud of breath, which settles on a player close by.
    fn tick_fireballs(&mut self, level: &Level<'static>, seen: &[Seen], out: &mut KaijuTick) {
        let mut fireballs = std::mem::take(&mut self.fireballs);
        fireballs.retain_mut(|f| {
            let along = normalize(f.velocity);
            f.velocity = [(f.velocity[0] + along[0] * 0.1) * 0.95, (f.velocity[1] + along[1] * 0.1) * 0.95, (f.velocity[2] + along[2] * 0.1) * 0.95];
            let next = [f.at[0] + f.velocity[0], f.at[1] + f.velocity[1], f.at[2] + f.velocity[2]];
            let wall = first_solid(level, f.at, next, |_| false);
            let end = wall.unwrap_or(next);
            let struck = seen
                .iter()
                .filter_map(|s| clip(inflate(player_box(s), 0.3, 0.3, 0.3), f.at, end))
                .min_by(f64::total_cmp)
                .map(|t| [f.at[0] + (end[0] - f.at[0]) * t, f.at[1] + (end[1] - f.at[1]) * t, f.at[2] + (end[2] - f.at[2]) * t]);
            let Some(at) = struck.or(wall) else {
                f.at = next;
                f.life += 1;
                return f.life < FIREBALL_LIFE;
            };
            let settle = seen.iter().find(|s| distance_sq(s.feet, at) < 16.0).map_or(at, |s| s.feet);
            let pitch = 0.9 + self.next_float() * 0.1;
            out.sounds.push(sound("entity.dragon_fireball.explode", at, 1.0, pitch));
            self.clouds.push(Cloud { at: settle, radius: 3.0, growth: (7.0 - 3.0) / 600.0, duration: 600, age: 0, damage: 6.0, touched: Vec::new(), breath: false });
            false
        });
        self.fireballs = fireballs;
    }

    /// `AreaEffectCloud`: after a second's wait it harms whoever stands in it, each at most once
    /// a second, until it runs out.
    fn tick_clouds(&mut self, seen: &[Seen], out: &mut KaijuTick) {
        let mut clouds = std::mem::take(&mut self.clouds);
        clouds.retain_mut(|c| {
            c.age += 1;
            if c.age >= c.duration + 20 {
                return false;
            }
            if c.age < 20 {
                return true;
            }
            c.radius += c.growth;
            if c.age.is_multiple_of(5) {
                let age = c.age;
                c.touched.retain(|&(_, until)| age < until);
                for s in seen {
                    let (dx, dz) = (s.feet[0] - c.at[0], s.feet[2] - c.at[2]);
                    let level = s.feet[1] < c.at[1] + 0.5 && s.feet[1] + s.height > c.at[1];
                    if level && dx * dx + dz * dz <= f64::from(c.radius * c.radius) && c.touched.iter().all(|&(id, _)| id != s.id) {
                        c.touched.push((s.id, age + 20));
                        let kind = PlayerHitKind::Explosion { knockback: glam::DVec3::ZERO };
                        out.player_hits.push(PlayerHit { player_id: s.id, damage: c.damage * PLAYER_SHARE_MELEE, kind, source: Some(self.id) });
                    }
                }
            }
            true
        });
        self.clouds = clouds;
    }
}
