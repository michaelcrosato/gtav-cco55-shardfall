//! Armed opponents (PLAN.md §5.6.4): gang members and guards, spawned by missions from
//! `content/freeway/enemies.toml`. Once they know about the player they take cover points facing
//! the threat and work peek-and-shoot cycles (down for 1.5–3 s, up for 1.5–2.5 s firing); a player
//! who stays in one cover for more than 8 s gets flanked (one of them runs to a spot the player's
//! wall does not cover); those with grenades lob one at a player dug in behind cover; gangs fall
//! back when hurt. Their shots hit by distance, the player's speed and cover.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::input::buttons;
use pav_core::puppet::{BodyPlan, PuppetDef};
use pav_core::sim::Sim;
use pav_world::City;
use serde::{Deserialize, Serialize};

/// A side's people.
#[derive(Clone, Debug, Deserialize)]
pub struct SideDef {
    pub name: String,
    pub prefix: String,
    pub weapons: Vec<String>,
    pub accuracy: f32,
    pub damage: f32,
    pub retreat: f32,
    #[serde(default)]
    pub grenades: u8,
    pub skin: Vec<String>,
    pub shirt: Vec<String>,
    pub pants: Vec<String>,
    pub shoes: Vec<String>,
    pub height: [f32; 2],
    pub build: [f32; 2],
}

#[derive(Clone, Debug, Deserialize)]
pub struct EnemiesDef {
    pub side: BTreeMap<String, SideDef>,
}

/// The sides (cached; re-read when the file changes during native hot reload).
pub fn def() -> Result<Arc<EnemiesDef>> {
    static CACHE: Mutex<Option<(String, Arc<EnemiesDef>)>> = Mutex::new(None);
    let text = crate::content::text("enemies.toml", include_str!("../../../content/freeway/enemies.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref()
        && *t == *text
    {
        return Ok(d.clone());
    }
    let d: EnemiesDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/enemies.toml: {e}"))?;
    if d.side.is_empty() {
        return Err(anyhow!("content/freeway/enemies.toml: no sides"));
    }
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// Down behind cover and up shooting (s); the hold that gets the player flanked (s); how near a
/// spot counts as reached (m); how far they notice the player (m); how many cover spots one
/// decision weighs (each costs a few rays).
const HIDE: [f32; 2] = [1.5, 3.0];
const PEEK: [f32; 2] = [1.5, 2.5];
const CAMP: f32 = 8.0;
const AT: f32 = 0.6;
const NOTICE: f32 = 45.0;
const WEIGH: usize = 14;
/// Seconds between grenades from anyone, and the range they are thrown at (m).
const GRENADE_EVERY: f32 = 10.0;
const GRENADE_RANGE: [f32; 2] = [8.0, 25.0];

/// A place to fight from: behind a wall, a low wall or a stopped car — or open ground.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    pub pos: Vec3,
    /// Away from the cover (zero on open ground), whether it is tall enough to stand behind,
    /// and the step out to shoot past it (zero: stand up over it).
    pub normal: Vec2,
    pub high: bool,
    #[serde(default)]
    pub peek: Vec2,
}

impl Spot {
    fn open(pos: Vec3) -> Spot {
        Spot { pos, normal: Vec2::ZERO, high: true, peek: Vec2::ZERO }
    }

    /// Eye height while down behind it.
    fn low_eye(&self) -> f32 {
        if self.high { 1.5 } else { 0.85 }
    }
}

/// What one of them is doing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Act {
    /// Unaware (at their post), or about to choose where to fight from.
    Idle,
    /// On the way to a spot.
    Move { to: Spot, flank: bool },
    /// Down behind it for `t` more seconds.
    Hide { at: Spot, t: f32 },
    /// Up and shooting for `t` more seconds (`saw`: had the player in sight this time).
    Peek { at: Spot, t: f32, saw: bool },
    /// Hurt (gangs): falling back to a spot farther off.
    Retreat { to: Spot },
}

/// One armed opponent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Enemy {
    pub id: EntityId,
    pub side: String,
    /// Weapon id (weapons.toml).
    pub weapon: String,
    pub act: Act,
    pub aware: bool,
    /// Seconds to the next shot, and the grenades left.
    pub cooldown: f32,
    pub grenades: u8,
    /// Has fallen back once (it does not keep running).
    pub retreated: bool,
    /// Where it was a moment ago and the seconds since (making no headway toward a spot: give it
    /// up), and the nearest it came to the spot.
    #[serde(default)]
    pub anchor: Vec3,
    pub stuck: f32,
    pub best: f32,
    /// The health it was first seen with (a mission may start it below its side's full): hurt
    /// means less than this.
    #[serde(default)]
    pub start: f32,
}

/// Everyone fighting the player (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Enemies {
    pub list: Vec<Enemy>,
    /// The cover the player holds and for how long (the flank clock).
    pub camp_at: Option<Vec3>,
    pub camp: f32,
    /// Seconds before anyone may throw a grenade.
    pub grenade_t: f32,
    /// Counts for tests: shots, hits, flanks, grenades, retreats, people who went down.
    pub shots: u32,
    pub hits: u32,
    pub flanks: u32,
    pub thrown: u32,
    pub retreats: u32,
    pub down: u32,
}

/// What they need to know this tick.
pub struct Moment<'a> {
    pub city: &'a City,
    /// The player's feet and velocity; their cover (spot, away from the wall) and whether they
    /// are down behind it; in a vehicle.
    pub player: Vec3,
    pub player_vel: Vec3,
    pub cover: Option<(Vec3, Vec2)>,
    pub covered: bool,
    pub in_car: bool,
    /// Gunfire this tick (everyone near it turns to the fight).
    pub gunfire: Option<Vec3>,
    pub dt: f32,
}

/// What they did this tick.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Damage the hits did to the player.
    pub hits: Vec<f32>,
    /// Shots: from, to, and whether it hit (tracers).
    pub shots: Vec<(Vec3, Vec3, bool)>,
    /// Grenades thrown: from, velocity.
    pub grenades: Vec<(Vec3, Vec3)>,
}

fn pick<'a>(v: &'a [String], sim: &mut Sim) -> &'a str {
    &v[sim.state.rng.below(v.len().max(1) as u32) as usize]
}

/// A side's look.
fn look(side: &SideDef, sim: &mut Sim) -> PuppetDef {
    let mut p = PuppetDef::preset(BodyPlan::Biped);
    p.skin = pick(&side.skin, sim).into();
    p.shirt = pick(&side.shirt, sim).into();
    p.pants = pick(&side.pants, sim).into();
    p.shoes = pick(&side.shoes, sim).into();
    p.scale *= sim.state.rng.range(side.height[0], side.height[1]) / 1.75;
    let build = sim.state.rng.range(side.build[0], side.build[1]);
    p.torso_radius *= build;
    p.hip_width *= build;
    p.shoulder_width *= build.sqrt();
    p.limb_radius *= build.sqrt();
    p.idle_clip = "MESH2MOTION/Idle_A".into();
    p
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

fn up(v: Vec2) -> Vec3 {
    Vec3::new(v.x, 0.0, v.y)
}

/// Whether nothing — walls, vehicles, people — stands between `from` and `to` (`me` and the
/// player aside: a ray to the player's chest that hits nothing has them in sight).
fn clear(sim: &Sim, from: Vec3, to: Vec3, me: EntityId) -> bool {
    let d = to - from;
    let len = d.length();
    len < 0.5 || sim.raycast(from, d / len, len - 0.3, Some(me)).is_none()
}

impl Enemies {
    /// One of `side` at `feet`, facing `facing`, carrying `weapon` (None: one of the side's).
    pub fn spawn(&mut self, sim: &mut Sim, side: &str, feet: Vec3, facing: f32, weapon: Option<&str>) -> Option<EntityId> {
        let d = def().ok()?;
        let s = d.side.get(side)?;
        let weapon = weapon.map(str::to_string).unwrap_or_else(|| pick(&s.weapons, sim).to_string());
        let mut puppet = look(s, sim);
        // The gun in their hand.
        puppet.weapon.asset = crate::combat::asset_of(&weapon);
        if !puppet.weapon.asset.is_empty() {
            puppet.weapon.kind = pav_core::puppet::WeaponKind::Dagger;
        }
        let id = sim.spawn_npc(&format!("{}:{}", s.prefix, side), feet, facing, puppet, None, None);
        crate::world::ambient(sim, id);
        self.list.push(Enemy {
            id,
            side: side.into(),
            weapon,
            act: Act::Idle,
            aware: false,
            cooldown: sim.state.rng.range(0.5, 1.5),
            grenades: s.grenades,
            retreated: false,
            anchor: feet,
            stuck: 0.0,
            best: f32::MAX,
            start: 0.0,
        });
        Some(id)
    }

    /// Whether `id` is one of them.
    pub fn is_enemy(&self, id: EntityId) -> bool {
        self.list.iter().any(|e| e.id == id)
    }

    /// Everyone turns to the fight (they heard it, or a mission says so).
    pub fn alert(&mut self) {
        for e in &mut self.list {
            e.aware = true;
        }
    }

    /// Everyone gone (a mission ended, the player respawned).
    pub fn clear(&mut self, sim: &mut Sim) {
        for e in self.list.drain(..) {
            sim.despawn(e.id);
        }
        self.camp_at = None;
        self.camp = 0.0;
    }

    /// One tick: their moves go into `moves`; `hp` is everyone's health (`Combat::hp`).
    pub fn tick(
        &mut self,
        sim: &mut Sim,
        m: &Moment,
        hp: &BTreeMap<u32, f32>,
        moves: &mut BTreeMap<EntityId, InputFrame>,
    ) -> Report {
        let mut rep = Report::default();
        let Ok(d) = def() else { return rep };
        let weapons = crate::combat::def().ok();
        // The fallen leave the roster.
        let before = self.list.len();
        self.list.retain(|e| sim.state.entities.get(e.id).is_some_and(|x| !x.name.starts_with("dead:")));
        self.down += (before - self.list.len()) as u32;
        // The flank clock: how long the player has held this cover.
        match m.cover {
            Some((at, _)) if self.camp_at.is_some_and(|c| c.distance(at) < 1.5) => self.camp += m.dt,
            Some((at, _)) => {
                self.camp_at = Some(at);
                self.camp = 0.0;
            }
            None => {
                self.camp_at = None;
                self.camp = 0.0;
            }
        }
        self.grenade_t -= m.dt;
        let chest = m.player + Vec3::Y * 1.1;
        // One flanker at a time, when the player has dug in.
        let flanking = self.list.iter().any(|e| matches!(e.act, Act::Move { flank: true, .. }));
        let flank_now = if self.camp > CAMP && !flanking {
            self.list
                .iter()
                .filter(|e| e.aware && matches!(e.act, Act::Hide { .. } | Act::Peek { .. }))
                .filter_map(|e| sim.feet(e.id).map(|f| (e.id, f.distance(m.player))))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|x| x.0)
        } else {
            None
        };
        for i in 0..self.list.len() {
            let mut e = self.list[i].clone();
            let Some(side) = d.side.get(&e.side) else { continue };
            let Some(feet) = sim.feet(e.id) else { continue };
            let eye = feet + Vec3::Y * 1.5;
            let dist = feet.distance(m.player);
            let sees = dist < NOTICE && (clear(sim, eye, chest, e.id) || clear(sim, eye, chest + Vec3::Y * 0.5, e.id));
            let full = sim.state.entities.get(e.id).map_or(100.0, |x| crate::combat::max_hp(&x.name));
            let now = hp.get(&e.id.0).copied().unwrap_or(full);
            if e.start <= 0.0 {
                e.start = now;
            }
            // The share of its health left.
            let health = now / e.start.max(1.0);
            // Awareness: seen, heard or hurt.
            if !e.aware && (sees || health < 1.0 || m.gunfire.is_some_and(|g| g.distance(feet) < 60.0)) {
                e.aware = true;
            }
            let taken: Vec<Vec3> = self.list.iter().filter(|o| o.id != e.id).filter_map(|o| act_spot(&o.act)).collect();
            // Hurt gangs fall back, once.
            if e.aware && !e.retreated && side.retreat > 0.0 && health < side.retreat {
                e.retreated = true;
                self.retreats += 1;
                let away = feet + up((flat(feet) - flat(m.player)).normalize_or(Vec2::X)) * 22.0;
                let to = cover_from(sim, m.city, e.id, away, chest, &taken, (dist + 10.0, dist + 40.0), 20.0)
                    .or_else(|| m.city.walk.nearest_walkable(flat(away), 6).map(|p| Spot::open(Vec3::new(p.x, feet.y, p.y))));
                if let Some(to) = to {
                    e.act = Act::Retreat { to };
                    e.best = f32::MAX;
                }
            }
            if Some(e.id) == flank_now
                && let Some((pat, pn)) = m.cover
                && let Some(to) = flank_spot(m.city, sim, e.id, pat, pn, &taken)
            {
                e.act = Act::Move { to, flank: true };
                e.best = f32::MAX;
                self.flanks += 1;
                self.camp = 0.0;
            }
            let mut frame = InputFrame::default();
            let mut face = None;
            let mut shoot = false;
            e.act = match e.act {
                // Choose where to fight from: cover with a line on the player, else closer in.
                Act::Idle if e.aware => {
                    e.best = f32::MAX;
                    // Having fallen back, they keep their distance (and do not come in again).
                    let band = if e.retreated { (22.0, 48.0) } else { (6.0, 32.0) };
                    match cover_from(sim, m.city, e.id, feet, chest, &taken, band, 18.0) {
                        Some(to) => Act::Move { to, flank: false },
                        None if sees || e.retreated => {
                            Act::Peek { at: Spot::open(feet), t: sim.state.rng.range(PEEK[0], PEEK[1]), saw: false }
                        }
                        None => {
                            let toward =
                                feet + up((flat(m.player) - flat(feet)).normalize_or(Vec2::X)) * (dist - 8.0).clamp(2.0, 12.0);
                            let p = m.city.walk.nearest_walkable(flat(toward), 4).unwrap_or(flat(toward));
                            Act::Move { to: Spot::open(Vec3::new(p.x, feet.y, p.y)), flank: false }
                        }
                    }
                }
                Act::Idle => Act::Idle,
                // Arrived: a flanker or someone on open ground comes up shooting; the rest get down.
                Act::Move { to, flank } if flat(to.pos).distance(flat(feet)) < AT => {
                    if flank || to.normal == Vec2::ZERO {
                        Act::Peek { at: to, t: sim.state.rng.range(PEEK[0], PEEK[1]) + 1.0, saw: false }
                    } else {
                        Act::Hide { at: to, t: sim.state.rng.range(HIDE[0], HIDE[1]) * 0.5 }
                    }
                }
                Act::Retreat { to } if flat(to.pos).distance(flat(feet)) < AT => Act::Hide { at: to, t: HIDE[1] },
                Act::Move { to, flank } => {
                    frame.move_dir = way(m.city, feet, to.pos);
                    if e.gave_up(feet, to.pos, m.dt) { Act::Idle } else { Act::Move { to, flank } }
                }
                Act::Retreat { to } => {
                    frame.move_dir = way(m.city, feet, to.pos);
                    if e.gave_up(feet, to.pos, m.dt) {
                        Act::Hide { at: Spot::open(feet), t: HIDE[1] }
                    } else {
                        Act::Retreat { to }
                    }
                }
                Act::Hide { at, t } => {
                    face = Some(m.player);
                    frame.move_dir = step_to(feet, at.pos);
                    if !at.high {
                        frame.held |= buttons::CROUCH;
                    }
                    // The player got a line on the spot: find another.
                    let exposed =
                        at.normal != Vec2::ZERO && clear(sim, at.pos + Vec3::Y * at.low_eye(), chest + Vec3::Y * 0.5, e.id);
                    if exposed {
                        Act::Idle
                    } else if t - m.dt <= 0.0 {
                        Act::Peek { at, t: sim.state.rng.range(PEEK[0], PEEK[1]), saw: false }
                    } else {
                        Act::Hide { at, t: t - m.dt }
                    }
                }
                Act::Peek { at, t, saw } => {
                    face = Some(m.player);
                    frame.move_dir = step_to(feet, at.pos + up(at.peek));
                    frame.held |= buttons::FOCUS;
                    shoot = true;
                    let saw = saw || sees;
                    if t - m.dt <= 0.0 {
                        // Never had a line from here: choose again.
                        if saw { Act::Hide { at, t: sim.state.rng.range(HIDE[0], HIDE[1]) } } else { Act::Idle }
                    } else {
                        Act::Peek { at, t: t - m.dt, saw }
                    }
                }
            };
            // Fire: at the player, when up and in sight.
            e.cooldown -= m.dt;
            let w = weapons.as_ref().and_then(|ws| ws.weapon.iter().find(|w| w.id == e.weapon).cloned());
            if shoot
                && sees
                && let Some(w) = w.as_ref()
                && e.cooldown <= 0.0
                && dist < w.range * 1.2
            {
                // Bursts for automatic weapons, single shots otherwise.
                let gap = 60.0 / w.rpm.max(1.0);
                e.cooldown = if w.auto { gap.max(0.12) * 1.5 } else { (gap * 2.5).max(0.45) } + sim.state.rng.range(0.0, 0.35);
                let speed = m.player_vel.length();
                let chance = side.accuracy
                    * (1.0 - dist / w.range.max(1.0)).clamp(0.15, 1.0)
                    * if speed > 5.0 { 0.55 } else { 1.0 }
                    * if m.covered { 0.25 } else { 1.0 }
                    * if m.in_car { 0.5 } else { 1.0 };
                let muzzle = feet + Vec3::Y * 1.35 + (chest - eye).normalize_or(Vec3::Z) * 0.45;
                let pellets = w.pellets.max(1);
                let mut landed = 0.0;
                for _ in 0..pellets {
                    let hit = sim.state.rng.f32() < chance;
                    let miss =
                        Vec3::new(sim.state.rng.range(-1.4, 1.4), sim.state.rng.range(-0.5, 0.8), sim.state.rng.range(-1.4, 1.4));
                    if hit {
                        landed += w.damage * side.damage;
                    }
                    if pellets == 1 || hit {
                        rep.shots.push((muzzle, if hit { chest } else { chest + miss }, hit));
                    }
                }
                if pellets > 1 && landed == 0.0 {
                    rep.shots.push((muzzle, chest + Vec3::new(1.2, 0.4, -1.0), false));
                }
                self.shots += 1;
                if landed > 0.0 {
                    self.hits += 1;
                    rep.hits.push(landed);
                }
                if let Some(c) = sim.state.entities.get_mut(e.id).and_then(|x| x.character.as_mut()) {
                    let clip = pav_core::clips::find_cached("QUATERNIUS/Pistol_Shoot");
                    if clip != 0 {
                        c.anim.replay_clip(clip, pav_core::clips::UPPER | pav_core::clips::ONCE | pav_core::clips::QUICK, 2.0);
                    }
                }
            }
            // A grenade at a player dug in behind cover.
            if e.grenades > 0
                && self.grenade_t <= 0.0
                && m.covered
                && self.camp > 4.0
                && (GRENADE_RANGE[0]..GRENADE_RANGE[1]).contains(&dist)
                && matches!(e.act, Act::Peek { .. })
            {
                let from = feet + Vec3::Y * 1.6;
                rep.grenades.push((from, lob(from, m.player + Vec3::Y * 0.3)));
                e.grenades -= 1;
                self.grenade_t = GRENADE_EVERY;
                self.thrown += 1;
                if let Some(c) = sim.state.entities.get_mut(e.id).and_then(|x| x.character.as_mut()) {
                    let clip = pav_core::clips::find_cached("MESH2MOTION/OverhandThrow");
                    if clip != 0 {
                        c.anim.replay_clip(clip, pav_core::clips::UPPER | pav_core::clips::ONCE, 1.2);
                    }
                }
            }
            // Down behind cover: the cover pose (facing the wall); otherwise the body's own.
            let pose = match e.act {
                Act::Hide { at, .. } if at.normal != Vec2::ZERO && frame.move_dir == Vec2::ZERO => {
                    face = Some(at.pos - up(at.normal));
                    pav_core::clips::find_cached(if at.high { crate::cover::POSE_HIGH } else { crate::cover::POSE_LOW })
                }
                _ => 0,
            };
            if let Some(c) = sim.state.entities.get_mut(e.id).and_then(|x| x.character.as_mut()) {
                c.face = face.map(|p| {
                    let to = p - feet;
                    to.x.atan2(to.z)
                });
                if pose != 0 {
                    c.anim.play_clip(pose, 0, 1.0);
                } else if c.anim.clip != 0
                    && [crate::cover::POSE_LOW, crate::cover::POSE_HIGH]
                        .iter()
                        .any(|n| pav_core::clips::find_cached(n) == c.anim.clip)
                {
                    c.anim.stop_clip();
                }
            }
            moves.insert(e.id, frame);
            self.list[i] = e;
        }
        rep
    }
}

impl Enemy {
    /// Making for `to` and going nowhere (barely moved for 1.5 s), or not a step nearer for 8 s
    /// (round and round a block): give it up.
    fn gave_up(&mut self, feet: Vec3, to: Vec3, dt: f32) -> bool {
        let left = flat(to).distance(flat(feet));
        if left < self.best - 0.3 {
            self.best = left;
            self.stuck = 0.0;
        }
        self.stuck += dt;
        let moved = flat(feet).distance(flat(self.anchor)) > 0.6;
        if moved {
            self.anchor = feet;
        }
        let still = !moved && self.stuck > 1.5 && flat(feet).distance(flat(self.anchor)) < 0.6;
        if still || self.stuck > 8.0 {
            self.stuck = 0.0;
            self.best = f32::MAX;
            self.anchor = feet;
            return true;
        }
        if moved {
            // Headway: the clock only runs while it goes nowhere or gets no nearer.
            self.stuck = self.stuck.min(1.0);
        }
        false
    }
}

/// The spot an act holds or makes for.
fn act_spot(a: &Act) -> Option<Vec3> {
    match *a {
        Act::Move { to, .. } | Act::Retreat { to } => Some(to.pos),
        Act::Hide { at, .. } | Act::Peek { at, .. } => Some(at.pos),
        Act::Idle => None,
    }
}

/// A short step toward `to` (holding a spot, stepping out to peek and back).
fn step_to(feet: Vec3, to: Vec3) -> Vec2 {
    let d = flat(to) - flat(feet);
    if d.length() < 0.25 { Vec2::ZERO } else { d.normalize() * d.length().min(1.0) }
}

/// Places near `from` to fight from: the district's cover points (any facing) and the far sides
/// of stopped vehicles, nearest first.
fn candidates(sim: &Sim, city: &City, from: Vec3, threat: Vec3, reach: f32) -> Vec<Spot> {
    let mut out: Vec<(f32, Spot)> = city
        .cover
        .within(from, reach)
        .into_iter()
        .map(|p| (p.pos.distance(from), Spot { pos: p.pos, normal: p.normal, high: p.high, peek: Vec2::ZERO }))
        .collect();
    for e in sim.state.entities.iter().filter(|e| e.vehicle.is_some() && e.pos.distance(from) < reach + 3.0) {
        let Some(c) = crate::drive::Car::read(sim, e.id) else { continue };
        if c.vel.length() > 1.5 || c.heli {
            continue;
        }
        let away = flat(c.center - threat).normalize_or(Vec2::X);
        let ext = away.dot(flat(c.forward())).abs() * c.half.z + away.dot(flat(c.left())).abs() * c.half.x;
        let pos = Vec3::new(c.center.x, c.feet().y, c.center.z) + up(away) * (ext + 0.75);
        out.push((pos.distance(from), Spot { pos, normal: away, high: false, peek: Vec2::ZERO }));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter().map(|(_, s)| s).collect()
}

/// Whether `s` is cover from a threat whose head is at `head`: down behind it nothing sees
/// through. With the step out to shoot from (standing over low cover, or out past the end of a
/// wall) and whether that step has a line on the head now (the player may be down behind cover of
/// their own: the spot still serves, peeking and waiting for them to show).
fn judge(sim: &Sim, me: EntityId, mut s: Spot, head: Vec3) -> Option<(Spot, bool)> {
    if clear(sim, s.pos + Vec3::Y * s.low_eye(), head, me) {
        return None;
    }
    if !s.high {
        s.peek = Vec2::ZERO;
        if clear(sim, s.pos + Vec3::Y * 1.6, head, me) {
            return Some((s, true));
        }
    }
    let along = Vec2::new(s.normal.y, -s.normal.x);
    for side in [1.0f32, -1.0] {
        let out = along * side * 1.0;
        if clear(sim, s.pos + up(out) + Vec3::Y * 1.5, head, me) {
            s.peek = out;
            return Some((s, true));
        }
    }
    if s.high {
        s.peek = along;
    }
    Some((s, false))
}

/// The best cover within `reach` of `from` against a threat whose chest is at `chest`, at a
/// distance from it within `band`, not taken by another.
#[allow(clippy::too_many_arguments)]
fn cover_from(
    sim: &Sim,
    city: &City,
    me: EntityId,
    from: Vec3,
    chest: Vec3,
    taken: &[Vec3],
    band: (f32, f32),
    reach: f32,
) -> Option<Spot> {
    let threat = chest - Vec3::Y * 1.1;
    let mut options: Vec<Spot> = candidates(sim, city, from, threat, reach)
        .into_iter()
        .filter(|s| {
            let d = s.pos.distance(threat);
            d > band.0 && d < band.1 && taken.iter().all(|t| t.distance(s.pos) > 1.6)
        })
        .collect();
    // Near, at a fighting distance (about 16 m).
    options.sort_by(|a, b| {
        let score = |s: &Spot| s.pos.distance(from) + 0.3 * (s.pos.distance(threat) - 16.0).abs();
        score(a).total_cmp(&score(b))
    });
    // A spot with a line on the player beats one without (the first such of the nearest few);
    // without any, the nearest that hides within 25 m of them.
    let head = chest + Vec3::Y * 0.5;
    let judged: Vec<(Spot, bool)> = options.into_iter().take(WEIGH).filter_map(|s| judge(sim, me, s, head)).collect();
    judged.iter().find(|(_, line)| *line).or_else(|| judged.iter().find(|(s, _)| s.pos.distance(threat) < 25.0)).map(|(s, _)| *s)
}

/// A spot to flank a player behind cover at `at` (away from their wall: `normal`): cover on the
/// player's own side of that wall, 8–22 m off, with a line to them when up; else open ground off
/// to one side with a line.
fn flank_spot(city: &City, sim: &Sim, me: EntityId, at: Vec3, normal: Vec2, taken: &[Vec3]) -> Option<Spot> {
    let chest = at + Vec3::Y * 1.1;
    let options: Vec<Spot> = candidates(sim, city, at, at, 22.0)
        .into_iter()
        .filter(|s| {
            let d = s.pos.distance(at);
            (8.0..22.0).contains(&d) && flat(s.pos - at).dot(normal) > 1.5 && taken.iter().all(|t| t.distance(s.pos) > 1.6)
        })
        .collect();
    let head = chest + Vec3::Y * 0.5;
    if let Some((s, _)) = options.into_iter().take(WEIGH).filter_map(|s| judge(sim, me, s, head)).find(|(_, line)| *line) {
        return Some(s);
    }
    // Open ground out to the side, where the walk map allows.
    for side in [1.0f32, -1.0] {
        let along = Vec2::new(normal.y, -normal.x) * side;
        let dir = (normal * 0.7 + along).normalize_or(along);
        let want = flat(at) + dir * 13.0;
        if let Some(p) = city.walk.nearest_walkable(want, 4) {
            let pos = Vec3::new(p.x, at.y, p.y);
            if clear(sim, pos + Vec3::Y * 1.5, chest, me) {
                return Some(Spot::open(pos));
            }
        }
    }
    None
}

/// Which way to run for `to`: straight when nothing stands between, else along the walk map.
fn way(city: &City, feet: Vec3, to: Vec3) -> Vec2 {
    let direct = (flat(to) - flat(feet)).normalize_or(Vec2::ZERO);
    let d = to - feet;
    if d.length() < 3.0 {
        return direct;
    }
    let path = match (city.walk.nearest_walkable(flat(feet), 4), city.walk.nearest_walkable(flat(to), 6)) {
        (Some(a), Some(b)) => city.walk.path(a, b, 3000),
        _ => Vec::new(),
    };
    path.iter().find(|p| p.distance(flat(feet)) > 1.2).map(|p| (*p - flat(feet)).normalize_or(direct)).unwrap_or(direct)
}

/// A throw from `from` that lands at `to`: a flight of about a second, longer for far throws.
fn lob(from: Vec3, to: Vec3) -> Vec3 {
    let d = to - from;
    let reach = Vec2::new(d.x, d.z).length();
    let t = (reach / 13.0).clamp(0.7, 1.6);
    Vec3::new(d.x / t, (d.y + 0.5 * 9.81 * t * t) / t, d.z / t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sides_load_and_their_weapons_exist() {
        let d = def().unwrap();
        let w = crate::combat::def().unwrap();
        for (key, s) in &d.side {
            assert!(!s.weapons.is_empty(), "{key}: no weapons");
            for id in &s.weapons {
                assert!(w.weapon.iter().any(|x| &x.id == id), "{key}: no weapon '{id}'");
            }
        }
        assert!(d.side.contains_key("gang") && d.side.contains_key("guard"));
    }

    #[test]
    fn a_lob_lands_where_it_was_aimed() {
        let from = Vec3::new(0.0, 1.6, 0.0);
        let to = Vec3::new(12.0, 0.3, 5.0);
        let v = lob(from, to);
        let t = (Vec2::new(12.0, 5.0).length() / 13.0).clamp(0.7, 1.6);
        let land = from + v * t - Vec3::Y * 0.5 * 9.81 * t * t;
        assert!(land.distance(to) < 0.01, "{land}");
    }
}
