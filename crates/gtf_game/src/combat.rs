//! Combat (PLAN.md §5.6.4): the weapons of `weapons.toml`; hitscan fire along the camera's aim
//! ray, then from the muzzle to the hit point so a wall in between takes the shot; spread that
//! grows with fire and movement and recoil that kicks the camera; magazines and reloads; melee
//! combos; aim assist that snaps to the nearest person in a 12° cone on L2; the player's vitals
//! (health 200 regenerating to half after 5 s unharmed, armour taking 70% of the damage); people's
//! health, hit reactions and deaths; and the tracers, muzzle flashes and impact marks the view
//! draws. Every random draw comes from the sim's rng.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::{Vec2, Vec3};
use pav_core::color::Color;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::input::pad;
use pav_core::present::{CameraIntent, Crosshair, DecalDraw, DrawList, LightDraw, PrimDraw, TracerDraw};
use pav_core::{InputFrame, Sim};
use serde::{Deserialize, Serialize};

use crate::camera::OrbitCam;

/// What a weapon does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    None,
    Melee,
    Gun,
    Thrown,
}

/// One weapon of `weapons.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponDef {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub kind: Kind,
    #[serde(default)]
    pub damage: f32,
    #[serde(default)]
    pub rpm: f32,
    #[serde(default)]
    pub magazine: u32,
    #[serde(default)]
    pub reserve: u32,
    #[serde(default)]
    pub range: f32,
    #[serde(default = "one")]
    pub pellets: u32,
    #[serde(default)]
    pub spread: f32,
    #[serde(default)]
    pub bloom: f32,
    #[serde(default)]
    pub recoil: f32,
    #[serde(default)]
    pub reload: f32,
    #[serde(default = "one_f")]
    pub head: f32,
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub knockback: f32,
    #[serde(default)]
    pub cue: String,
    #[serde(default)]
    pub combo: Vec<String>,
    /// Melee: the clip of the heavy swing ○ held winds up (empty: none).
    #[serde(default)]
    pub heavy: String,
    /// Fired from a vehicle's window (L1 aim, R1 fire): pistols and SMGs.
    #[serde(default)]
    pub driveby: bool,
    /// The prop in the hand (`LIB/gtf_weapons/p9_pistol`; empty: nothing shown).
    #[serde(default)]
    pub asset: String,
}

/// Puts library prop `asset` in character `id`'s hand (empty: an empty hand); nothing changes
/// when it already holds it.
pub fn hold(sim: &mut Sim, id: EntityId, asset: &str) {
    let base = &sim.config.puppet;
    let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) else { return };
    // (The player wears the scene's look until given one of their own.)
    let look = c.puppet.as_deref().unwrap_or(base);
    if look.weapon.asset == asset {
        return;
    }
    let mut look = look.clone();
    look.weapon.asset = asset.to_string();
    look.weapon.kind = if asset.is_empty() { pav_core::puppet::WeaponKind::None } else { pav_core::puppet::WeaponKind::Dagger };
    c.puppet = Some(Arc::new(look));
}

/// The upper body's poses with a gun: up on the sights, and reloading.
pub const AIM_POSE: &str = "QUATERNIUS/Pistol_Aim_Neutral";
pub const RELOAD_POSE: &str = "QUATERNIUS/Pistol_Reload";

/// Character `id` raises its gun to aim (`on`), or lowers it; a shot or a reload playing over
/// the pose is left to finish.
pub fn aim_pose(sim: &mut Sim, id: EntityId, on: bool) {
    let pose = pav_core::clips::find_cached(AIM_POSE);
    let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) else { return };
    if pose == 0 {
        return;
    }
    let other = c.anim.clip != 0 && c.anim.clip != pose && c.anim.clip_flags & pav_core::clips::STOP == 0;
    if on && !other {
        c.anim.play_clip(pose, pav_core::clips::UPPER, 1.0);
    } else if !on && c.anim.clip == pose {
        c.anim.stop_clip();
    }
}

/// The prop for weapon `id` (weapons.toml), or "" (fists, the phone, unknown).
pub fn asset_of(id: &str) -> String {
    def().ok().and_then(|d| d.weapon.iter().find(|w| w.id == id).map(|w| w.asset.clone())).unwrap_or_default()
}

fn one() -> u32 {
    1
}

fn one_f() -> f32 {
    1.0
}

/// `weapons.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponsDef {
    pub weapon: Vec<WeaponDef>,
}

/// The game's weapons (cached; re-read when the file changes during native hot reload).
pub fn def() -> Result<Arc<WeaponsDef>> {
    static CACHE: Mutex<Option<(String, Arc<WeaponsDef>)>> = Mutex::new(None);
    let text = crate::content::text("weapons.toml", include_str!("../../../content/freeway/weapons.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref() {
        if *t == *text {
            return Ok(d.clone());
        }
    }
    let d: WeaponsDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/weapons.toml: {e}"))?;
    if d.weapon.is_empty() {
        return Err(anyhow!("content/freeway/weapons.toml: no weapons"));
    }
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

pub const MAX_HEALTH: f32 = 200.0;
pub const MAX_ARMOR: f32 = 100.0;
/// People's health: everyone, police, and the share of a hit armour takes.
const PERSON_HP: f32 = 100.0;
const POLICE_HP: f32 = 120.0;
const GUARD_HP: f32 = 150.0;

/// Someone's full health by who they are (the name's prefix): police 120, guards 150 (with
/// armour), everyone else 100.
pub fn max_hp(name: &str) -> f32 {
    if name.starts_with("cop:") {
        POLICE_HP
    } else if name.starts_with("guard:") {
        GUARD_HP
    } else {
        PERSON_HP
    }
}
const ARMOR_SHARE: f32 = 0.7;
/// Seconds a body stays before it is taken away.
const BODY_TIME: f32 = 25.0;
const DEATHS: [&str; 5] =
    ["MESH2MOTION/Death_A", "MESH2MOTION/Death_B", "MESH2MOTION/Death_C", "MESH2MOTION/Death_D", "QUATERNIUS/Death01"];
const HITS: [&str; 2] = ["QUATERNIUS/Hit_Chest", "QUATERNIUS/Hit_Head"];

/// A shot's trail and where it struck, drawn for a moment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Trace {
    pub from: Vec3,
    pub to: Vec3,
    pub normal: Vec3,
    /// It struck a person (a red puff) or a surface (a mark that stays a while).
    pub person: bool,
    pub age: f32,
}

/// A grenade in flight: where, how fast, seconds left on its fuse.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grenade {
    pub pos: Vec3,
    pub vel: Vec3,
    pub fuse: f32,
}

/// A grenade's fuse (s), blast radius (m) and throw speed (m/s).
/// The bat's place in weapons.toml: after the wheel's eight, in the fists' slot once owned.
pub const BAT: usize = 8;

/// Holding ○ this long (s) winds up the heavy swing; it lands this far in (s) and hits this many
/// times as hard, knocking people back further (PLAN §5.6.1: heavy on hold).
pub const HEAVY_HOLD: f32 = 0.45;
const HEAVY_LANDS: f32 = 0.36;
const HEAVY_DAMAGE: f32 = 2.2;
const HEAVY_KNOCK: f32 = 2.6;

pub const FUSE: f32 = 3.0;
pub const BLAST: f32 = 6.0;
const THROW: f32 = 13.0;

/// Where a grenade leaves the hand and how fast, thrown along the camera's view.
pub fn throw_from(s: &Shooter) -> (Vec3, Vec3) {
    let dir = (s.camera.target - s.camera.eye).normalize_or(Vec3::NEG_Z);
    let flat = Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::NEG_Z);
    let from = s.feet + Vec3::Y * 1.6 + flat * 0.4;
    (from, flat * THROW * (1.0 - dir.y.max(0.0) * 0.5) + Vec3::Y * (5.0 + dir.y * THROW))
}

/// The arc a throw would follow (preview points, up to two seconds or the first surface).
pub fn arc(sim: &Sim, from: Vec3, vel: Vec3) -> Vec<Vec3> {
    let (mut p, mut v) = (from, vel);
    let mut out = vec![p];
    let dt = 1.0 / 30.0;
    for _ in 0..60 {
        v.y -= 9.81 * dt;
        let step = v * dt;
        if let Some(h) = sim.raycast_static(p, step.normalize_or(Vec3::NEG_Y), step.length()) {
            out.push(h.point);
            break;
        }
        p += step;
        out.push(p);
    }
    out
}

/// The player's arms and vitals, and everyone's health (part of the game's state).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Combat {
    /// Rounds in each weapon's magazine and in reserve (by weapon index).
    pub mag: Vec<u32>,
    pub reserve: Vec<u32>,
    /// Seconds to the next shot, of reloading left, and the spread fire has added (degrees).
    pub cooldown: f32,
    pub reloading: f32,
    pub bloom: f32,
    /// The melee combo step and seconds since the swing began (the hit lands partway).
    pub combo: u8,
    pub swing: f32,
    pub swing_landed: bool,
    /// Seconds ○ has been held with fists or a melee weapon, whether this hold already swung
    /// heavy, and whether the swing under way is the heavy one.
    #[serde(default)]
    pub held_east: f32,
    #[serde(default)]
    pub heavy_spent: bool,
    #[serde(default)]
    pub heavy: bool,
    pub traces: Vec<Trace>,
    /// Seconds since the last shot (the muzzle flash) and the hit and kill markers (0..1).
    pub flash: f32,
    pub hit: f32,
    pub kill: f32,
    pub health: f32,
    pub armor: f32,
    pub since_hurt: f32,
    /// People's health by entity id, and the fallen (seconds since they fell).
    pub hp: BTreeMap<u32, f32>,
    pub fallen: Vec<(EntityId, f32)>,
    /// L2 last tick (aim assist snaps on the press), and R2's travel last tick (a trigger
    /// crossing halfway is a pull).
    pub aiming: bool,
    pub r2_last: f32,
    /// Grenades in flight, and the throw a held aim would make (the preview arc).
    #[serde(default)]
    pub grenades: Vec<Grenade>,
    #[serde(default)]
    pub arc: Option<(Vec3, Vec3)>,
    /// Totals for tests and soaks.
    pub shots: u32,
    pub hits: u32,
    pub kills: u32,
    pub headshots: u32,
    /// The player's special ability this tick (1 = none): damage dealt and taken, the aim
    /// assist's cone, the fists' shove.
    pub power: f32,
    pub tough: f32,
    pub assist_cone: f32,
    /// The aim assist setting (0 = off, 1 = full): how wide its cone and how hard its pull.
    #[serde(default = "one_f")]
    pub assist_strength: f32,
    pub shove: f32,
    /// Damage the player took since the game last looked (the Overdrive meter fills on it).
    pub taken: f32,
}

impl Default for Combat {
    fn default() -> Self {
        Combat {
            mag: Vec::new(),
            reserve: Vec::new(),
            cooldown: 0.0,
            reloading: 0.0,
            bloom: 0.0,
            combo: 0,
            swing: 9.0,
            swing_landed: true,
            held_east: 0.0,
            heavy_spent: false,
            heavy: false,
            traces: Vec::new(),
            flash: 9.0,
            hit: 0.0,
            kill: 0.0,
            health: MAX_HEALTH,
            armor: 50.0,
            since_hurt: 9.0,
            grenades: Vec::new(),
            arc: None,
            hp: BTreeMap::new(),
            fallen: Vec::new(),
            aiming: false,
            r2_last: 0.0,
            shots: 0,
            hits: 0,
            kills: 0,
            headshots: 0,
            power: 1.0,
            tough: 1.0,
            assist_cone: 1.0,
            assist_strength: 1.0,
            shove: 1.0,
            taken: 0.0,
        }
    }
}

/// Who fires, from where, looking how.
pub struct Shooter<'a> {
    pub id: EntityId,
    pub feet: Vec3,
    /// The direction the body faces (radians, 0 = +z).
    pub facing: f32,
    pub camera: &'a CameraIntent,
    pub moving: bool,
    /// Firing blind from cover (three times the spread).
    pub blind: bool,
    /// The vehicle the shooter sits in (a drive-by): shots pass through its own body.
    pub exclude: Option<EntityId>,
}

/// What combat did this tick, for the people and the heat.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// Where a shot was fired (people hear it 40 m away).
    pub gunfire: Option<Vec3>,
    /// People hurt and killed, and where.
    pub hurt: Vec<(EntityId, Vec3)>,
    pub killed: Vec<(EntityId, Vec3)>,
    /// Vehicles the shots struck.
    pub vehicles: Vec<EntityId>,
    /// Where shots struck the fixed world, and where from (glass breaks).
    pub walls: Vec<(Vec3, Vec3)>,
    /// Shots to the head, and the damage done to people.
    pub headshots: u32,
    pub dealt: f32,
}

/// A random direction within `spread` degrees of `dir`.
fn scatter(dir: Vec3, spread: f32, rng: &mut pav_core::rng::Rng) -> Vec3 {
    let right = dir.cross(Vec3::Y).normalize_or(Vec3::X);
    let up = right.cross(dir).normalize_or(Vec3::Y);
    let a = rng.f32() * std::f32::consts::TAU;
    let r = rng.f32().sqrt() * spread.to_radians();
    (dir + (right * a.cos() + up * a.sin()) * r.tan()).normalize()
}

/// The tyre (wheel index) or lamp (0 head left, 1 head right, 2 tail left, 3 tail right) of
/// vehicle `id` a round struck at `point`, if either.
fn struck_part(sim: &Sim, id: EntityId, point: Vec3) -> Option<(Option<usize>, Option<usize>)> {
    let e = sim.state.entities.get(id)?;
    let v = e.vehicle.as_ref()?;
    let view = v.view(e.pos, e.rot);
    let at = |p: Vec3| e.pos + e.rot * (p - Vec3::Y * view.base);
    let wheel = view.wheels.iter().position(|w| at(w.center).distance(point) < w.radius + 0.12);
    let lamps = view.head.iter().take(2).enumerate().chain(view.tail.iter().take(2).enumerate().map(|(i, p)| (i + 2, p)));
    let lamp = lamps.map(|(i, p)| (i, at(*p).distance(point))).filter(|(_, d)| *d < 0.3).min_by(|a, b| a.1.total_cmp(&b.1));
    Some((wheel, lamp.map(|l| l.0)))
}

/// Whether an entity is someone who can be hurt (a character other than `not`, still standing).
fn person(sim: &Sim, id: EntityId, not: EntityId) -> bool {
    id != not && sim.state.entities.get(id).is_some_and(|e| e.character.is_some() && !e.name.starts_with("dead:"))
}

impl Combat {
    /// Fills the magazines on first use (and after `weapons.toml` grows).
    pub fn arm(&mut self, def: &WeaponsDef) {
        while self.mag.len() < def.weapon.len() {
            let w = &def.weapon[self.mag.len()];
            self.mag.push(w.magazine);
            self.reserve.push(w.reserve);
        }
    }

    /// The player's weapon this tick: aim assist, fire, reload, melee; then the timers.
    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        sim: &mut Sim,
        def: &WeaponsDef,
        weapon: usize,
        input: &InputFrame,
        s: &Shooter,
        cam: &mut OrbitCam,
        dt: f32,
        events: &mut Vec<SimEvent>,
    ) -> Outcome {
        self.arm(def);
        let mut out = Outcome::default();
        let Some(w) = def.weapon.get(weapon) else { return out };
        let aim = input.pad.l2 > 0.5 || input.pad.held & pad::L2 != 0;
        let fire_held = input.pad.r2 > 0.5 || input.pad.held & pad::R2 != 0;
        let fire_pressed = input.pad.pressed & pad::R2 != 0 || (input.pad.r2 > 0.5 && self.r2_last <= 0.5);
        self.r2_last = input.pad.r2;
        let circle = input.pad.pressed & pad::EAST != 0;
        if aim && w.kind == Kind::Gun {
            if self.aiming {
                self.magnetism(sim, s, w.range, cam, dt);
            } else {
                self.assist(sim, s, w.range, cam);
            }
        }
        self.aiming = aim;
        aim_pose(sim, s.id, aim && w.kind == Kind::Gun && s.exclude.is_none());
        match w.kind {
            Kind::Gun => {
                if self.reloading > 0.0 {
                    self.reloading -= dt;
                    if self.reloading <= 0.0 {
                        let take = (w.magazine - self.mag[weapon]).min(self.reserve[weapon]);
                        self.mag[weapon] += take;
                        self.reserve[weapon] -= take;
                    }
                } else if circle && self.mag[weapon] < w.magazine && self.reserve[weapon] > 0 {
                    self.start_reload(sim, w, s, events);
                } else if (if w.auto { fire_held } else { fire_pressed }) && self.cooldown <= 0.0 {
                    if self.mag[weapon] == 0 {
                        events.push(sound("gun.empty", s.feet, 1.0));
                        self.cooldown = 0.25;
                        if self.reserve[weapon] > 0 {
                            self.start_reload(sim, w, s, events);
                        }
                    } else {
                        self.mag[weapon] -= 1;
                        self.fire(sim, w, s, aim, cam, events, &mut out);
                        if self.mag[weapon] == 0 && self.reserve[weapon] > 0 {
                            self.start_reload(sim, w, s, events);
                        }
                    }
                }
            }
            Kind::Thrown => {
                let (from, vel) = throw_from(s);
                self.arc = aim.then_some((from, vel));
                if fire_pressed && self.cooldown <= 0.0 && self.mag[weapon] > 0 {
                    self.mag[weapon] -= 1;
                    self.grenades.push(Grenade { pos: from, vel, fuse: FUSE });
                    self.cooldown = 0.8;
                    events.push(sound(&w.cue, s.feet, 1.0));
                    if self.mag[weapon] == 0 && self.reserve[weapon] > 0 {
                        self.mag[weapon] = 1;
                        self.reserve[weapon] -= 1;
                    }
                    let clip = pav_core::clips::find_cached("MESH2MOTION/OverhandThrow");
                    if let Some(c) = sim.state.entities.get_mut(s.id).and_then(|e| e.character.as_mut())
                        && clip != 0
                    {
                        c.anim.replay_clip(clip, pav_core::clips::ONCE, 1.0);
                    }
                }
            }
            Kind::Melee | Kind::None if circle || (w.kind == Kind::Melee && fire_pressed && !aim) => {
                let fists = &def.weapon[0];
                let mw = if w.kind == Kind::Melee { w } else { fists };
                self.start_swing(sim, mw, s, events);
            }
            _ => {}
        }
        // ○ held past the jab winds up the heavy swing (once a hold).
        if matches!(w.kind, Kind::Melee | Kind::None) && input.pad.held & pad::EAST != 0 {
            self.held_east += dt;
            let mw = if w.kind == Kind::Melee { w } else { &def.weapon[0] };
            if self.held_east >= HEAVY_HOLD && !self.heavy_spent && !mw.heavy.is_empty() && self.swing >= 0.3 {
                self.heavy_spent = true;
                self.start_heavy(sim, mw, s, events);
            }
        } else {
            self.held_east = 0.0;
            self.heavy_spent = false;
        }
        // The swing lands partway through.
        self.swing += dt;
        if !self.swing_landed && self.swing > if self.heavy { HEAVY_LANDS } else { 0.18 } {
            self.swing_landed = true;
            let heavy = std::mem::take(&mut self.heavy);
            let mw = if w.kind == Kind::Melee { w } else { &def.weapon[0] };
            self.land(sim, mw, s, heavy, events, &mut out);
        }
        self.cooldown = (self.cooldown - dt).max(0.0);
        self.bloom = (self.bloom - dt * 4.0).max(0.0);
        self.flash += dt;
        self.hit = (self.hit - dt * 4.0).max(0.0);
        self.kill = (self.kill - dt * 2.0).max(0.0);
        out
    }

    fn start_reload(&mut self, sim: &mut Sim, w: &WeaponDef, s: &Shooter, events: &mut Vec<SimEvent>) {
        self.reloading = w.reload;
        events.push(sound("gun.reload", s.feet, 1.0));
        let clip = pav_core::clips::find_cached(RELOAD_POSE);
        if let Some(c) = sim.state.entities.get_mut(s.id).and_then(|e| e.character.as_mut())
            && clip != 0
        {
            c.anim.replay_clip(clip, pav_core::clips::UPPER | pav_core::clips::ONCE, 1.0);
        }
    }

    /// One trigger pull: a pellet or ten along the aim ray, stopped by the first thing in the way
    /// from the muzzle.
    #[allow(clippy::too_many_arguments)]
    fn fire(
        &mut self,
        sim: &mut Sim,
        w: &WeaponDef,
        s: &Shooter,
        aim: bool,
        cam: &mut OrbitCam,
        events: &mut Vec<SimEvent>,
        out: &mut Outcome,
    ) {
        let eye = s.camera.eye;
        let forward = (s.camera.target - eye).normalize_or(Vec3::NEG_Z);
        let body = Vec3::new(s.facing.sin(), 0.0, s.facing.cos());
        let right = Vec3::new(body.z, 0.0, -body.x);
        let muzzle = s.feet + Vec3::Y * 1.35 + body * 0.35 + right * 0.22;
        let spread = (w.spread + self.bloom + if s.moving { 1.2 } else { 0.0 })
            * if aim { 1.0 } else { 1.8 }
            * if s.blind { 3.0 } else { 1.0 };
        let reach = w.range + eye.distance(muzzle);
        for _ in 0..w.pellets.max(1) {
            let dir = scatter(forward, spread, &mut sim.state.rng);
            let first = sim.raycast(eye, dir, reach, s.exclude);
            let target = first.as_ref().map_or(eye + dir * reach, |h| h.point);
            // From the muzzle: a wall between the gun and the aim point takes the shot.
            let to = target - muzzle;
            let len = to.length();
            let hit = if len > 0.05 { sim.raycast(muzzle, to / len, (len - 0.02).max(0.0), s.exclude).or(first) } else { first };
            let (point, normal, entity) = match &hit {
                Some(h) => (h.point, h.normal, h.entity),
                None => (target, Vec3::Y, None),
            };
            let struck = entity.filter(|&id| person(sim, id, s.id));
            if let Some(id) = struck {
                let feet = sim.feet(id).unwrap_or(point);
                let head = point.y - feet.y > 1.45;
                let amount = w.damage * if head { w.head } else { 1.0 } * self.power;
                if head {
                    self.headshots += 1;
                    out.headshots += 1;
                }
                out.dealt += amount;
                self.damage(sim, id, point, (point - muzzle).normalize_or(body), amount, w.knockback, events, out);
            } else if let Some(id) = entity.filter(|&id| sim.state.entities.get(id).is_some_and(|e| e.vehicle.is_some())) {
                // Cars soak up rounds (four or five dozen from a pistol) until they catch fire; a
                // round into a tyre bursts it, into a lamp breaks it.
                sim.damage_vehicle(id, w.damage * 1.5);
                if let Some((wheel, lamp)) = struck_part(sim, id, point) {
                    match (wheel, lamp) {
                        (Some(i), _) => {
                            sim.burst_tyre(id, i);
                        }
                        (None, Some(i)) => {
                            sim.break_vehicle_light(id, i);
                        }
                        _ => {}
                    }
                }
                events.push(sound("bullet.wall", point, 0.7));
                if !out.vehicles.contains(&id) {
                    out.vehicles.push(id);
                }
            } else if hit.is_some() {
                events.push(sound("bullet.wall", point, 0.6));
                if entity.is_none() {
                    out.walls.push((point, muzzle));
                }
            }
            self.traces.push(Trace { from: muzzle, to: point, normal, person: struck.is_some(), age: 0.0 });
        }
        self.shots += 1;
        self.cooldown = 60.0 / w.rpm.max(1.0);
        self.bloom = (self.bloom + w.bloom).min(8.0);
        self.flash = 0.0;
        // Recoil: the camera kicks up, a little to one side.
        let side = (sim.state.rng.f32() - 0.5) * w.recoil * 0.6;
        cam.elevation -= w.recoil;
        cam.yaw += side;
        events.push(sound(&w.cue, muzzle, 1.0));
        out.gunfire = Some(muzzle);
        if let Some(c) = sim.state.entities.get_mut(s.id).and_then(|e| e.character.as_mut()) {
            let id = pav_core::clips::find_cached("QUATERNIUS/Pistol_Shoot");
            if id != 0 {
                c.anim.replay_clip(id, pav_core::clips::UPPER | pav_core::clips::ONCE | pav_core::clips::QUICK, 2.0);
            }
        }
        self.traces.retain(|t| t.age < 6.0);
        if self.traces.len() > 64 {
            let n = self.traces.len() - 64;
            self.traces.drain(..n);
        }
    }

    fn start_swing(&mut self, sim: &mut Sim, w: &WeaponDef, s: &Shooter, events: &mut Vec<SimEvent>) {
        if self.swing < 0.35 {
            return;
        }
        self.combo = if self.swing < 0.9 { (self.combo + 1) % w.combo.len().max(1) as u8 } else { 0 };
        self.swing = 0.0;
        self.swing_landed = false;
        events.push(sound(&w.cue, s.feet + Vec3::Y, 1.0));
        if let (Some(c), Some(clip)) =
            (sim.state.entities.get_mut(s.id).and_then(|e| e.character.as_mut()), w.combo.get(self.combo as usize))
        {
            let id = pav_core::clips::find_cached(clip);
            if id != 0 {
                c.anim.replay_clip(id, pav_core::clips::ONCE | pav_core::clips::QUICK, 1.3);
            }
        }
    }

    /// The heavy swing: slower, harder, a lunge that knocks people down.
    fn start_heavy(&mut self, sim: &mut Sim, w: &WeaponDef, s: &Shooter, events: &mut Vec<SimEvent>) {
        self.combo = 0;
        self.swing = 0.0;
        self.swing_landed = false;
        self.heavy = true;
        events.push(sound(&w.cue, s.feet + Vec3::Y, 1.3));
        let id = pav_core::clips::find_cached(&w.heavy);
        if let Some(c) = sim.state.entities.get_mut(s.id).and_then(|e| e.character.as_mut())
            && id != 0
        {
            c.anim.replay_clip(id, pav_core::clips::ONCE | pav_core::clips::QUICK, 1.1);
        }
    }

    /// The swing lands: everyone within reach in front of the player (`heavy`: harder, and
    /// reaching a little further).
    fn land(&mut self, sim: &mut Sim, w: &WeaponDef, s: &Shooter, heavy: bool, events: &mut Vec<SimEvent>, out: &mut Outcome) {
        let body = Vec3::new(s.facing.sin(), 0.0, s.facing.cos());
        let chest = s.feet + Vec3::Y * 1.2;
        let near: Vec<EntityId> = sim
            .state
            .entities
            .iter()
            .filter(|e| person(sim, e.id, s.id))
            .filter(|e| {
                let d = e.pos - chest;
                let flat = Vec3::new(d.x, 0.0, d.z);
                flat.length() < w.range + if heavy { 0.7 } else { 0.4 }
                    && d.y.abs() < 1.2
                    && flat.normalize_or(body).dot(body) > 0.45
            })
            .map(|e| e.id)
            .collect();
        let (hard, knock) = if heavy { (HEAVY_DAMAGE, HEAVY_KNOCK) } else { (1.0, 1.0) };
        for id in near {
            let at = sim.state.entities.get(id).map_or(chest, |e| e.pos);
            let amount = w.damage * self.power * hard;
            out.dealt += amount;
            self.damage(sim, id, at, body, amount, w.knockback * self.shove * knock, events, out);
            events.push(sound("melee.hit", at, if heavy { 1.4 } else { 1.0 }));
        }
    }

    /// Someone takes `amount` at `at`, pushed along `dir`: a flinch, or a fall.
    #[allow(clippy::too_many_arguments)]
    pub fn damage(
        &mut self,
        sim: &mut Sim,
        id: EntityId,
        at: Vec3,
        dir: Vec3,
        amount: f32,
        knock: f32,
        events: &mut Vec<SimEvent>,
        out: &mut Outcome,
    ) {
        let full = sim.state.entities.get(id).map_or(PERSON_HP, |e| max_hp(&e.name));
        let hp = self.hp.entry(id.0).or_insert(full);
        *hp -= amount;
        let dead = *hp <= 0.0;
        self.hits += 1;
        self.hit = 1.0;
        events.push(sound("bullet.body", at, 0.8));
        let pick = sim.state.rng.below(if dead { DEATHS.len() as u32 } else { HITS.len() as u32 }) as usize;
        if let Some(e) = sim.state.entities.get_mut(id) {
            if let Some(c) = e.character.as_mut() {
                c.impulse += Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::ZERO) * knock;
                let clip = pav_core::clips::find_cached(if dead { DEATHS[pick] } else { HITS[pick] });
                if dead {
                    // The fall holds its last pose (no idle or walk clip to stand back up into).
                    c.lod = true;
                    if let Some(look) = c.puppet.as_deref() {
                        c.puppet = Some(Arc::new(pav_core::puppet::PuppetDef {
                            idle_clip: String::new(),
                            walk_clip: String::new(),
                            run_clip: String::new(),
                            ..look.clone()
                        }));
                    }
                    if clip != 0 {
                        c.anim.replay_clip(clip, pav_core::clips::CLAMP, 1.0);
                    }
                } else if clip != 0 {
                    c.anim.replay_clip(clip, pav_core::clips::ONCE, 1.0);
                }
            }
            if dead {
                e.name = format!("dead:{}", e.name);
            }
        }
        if dead {
            // (A body neither stops bullets nor blocks the way.)
            sim.set_solid(id, false);
            self.kills += 1;
            self.kill = 1.0;
            self.hp.remove(&id.0);
            self.fallen.push((id, 0.0));
            out.killed.push((id, at));
        } else {
            out.hurt.push((id, at));
        }
    }

    /// The nearest standing person within `cone` degrees of the view's heading (and 35° of its
    /// pitch: the on-foot camera looks down at the player) and `range` metres whom the camera can
    /// see: their chest.
    fn sight(&self, sim: &Sim, s: &Shooter, range: f32, cone: f32, pitch: f32) -> Option<Vec3> {
        let eye = s.camera.eye;
        let forward = (s.camera.target - eye).normalize_or(Vec3::NEG_Z);
        let flat = Vec2::new(forward.x, forward.z).normalize_or(Vec2::Y);
        let (cos, pitch_of) = (cone.to_radians().cos(), |d: Vec3| d.normalize_or(Vec3::Z).y.clamp(-1.0, 1.0).asin());
        sim.state
            .entities
            .iter()
            .filter(|e| person(sim, e.id, s.id))
            .filter_map(|e| {
                let chest = sim.feet(e.id)? + Vec3::Y * 1.25;
                let d = chest - eye;
                let dist = d.length();
                let heading = Vec2::new(d.x, d.z).normalize_or(flat).dot(flat);
                let tilt = (pitch_of(d) - pitch_of(forward)).abs().to_degrees();
                (dist < range + 4.0 && heading > cos && tilt < pitch).then_some((chest, dist))
            })
            .filter(|(chest, dist)| sim.raycast_static(eye, (*chest - eye).normalize(), *dist - 0.3).is_none())
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(chest, _)| chest)
    }

    /// On L2: the reticle snaps to the nearest standing person in a 12° cone within range; with
    /// nobody there the view levels to chest height ahead.
    fn assist(&mut self, sim: &Sim, s: &Shooter, range: f32, cam: &mut OrbitCam) {
        // Solved for the aiming pose the camera is easing into, so the snap holds.
        let mut pose = cam.clone();
        pose.distance = crate::camera::AIM_DISTANCE;
        pose.reach = pose.distance;
        pose.shoulder = crate::camera::AIM_SHOULDER * pose.side;
        pose.lift = crate::camera::AIM_LIFT;
        pose.target += Vec3::Y * (crate::camera::AIM_LIFT - cam.lift);
        let k = self.assist_strength.clamp(0.0, 1.0);
        match self.sight(sim, s, range, 12.0 * self.assist_cone * k, 35.0).filter(|_| k > 0.0) {
            Some(chest) => look_at(&mut pose, chest, k),
            None => {
                let ahead = pose.target + Vec3::new(pose.yaw.sin(), 0.0, -pose.yaw.cos()) * 20.0;
                look_at(&mut pose, ahead, 1.0);
            }
        }
        cam.yaw = pose.yaw;
        cam.elevation = pose.elevation;
    }

    /// While aiming: the view drifts toward a person close to the reticle (within 4°).
    fn magnetism(&mut self, sim: &Sim, s: &Shooter, range: f32, cam: &mut OrbitCam, dt: f32) {
        let k = self.assist_strength.clamp(0.0, 1.0);
        if k > 0.0
            && let Some(chest) = self.sight(sim, s, range, 4.0 * k, 4.0)
        {
            look_at(cam, chest, (1.0 - (-6.0 * dt).exp()) * k);
        }
    }

    /// The player takes `amount`: armour soaks up 70% of it while it lasts.
    pub fn hurt_player(&mut self, amount: f32) {
        let amount = amount * self.tough;
        self.taken += amount;
        let soak = (amount * ARMOR_SHARE).min(self.armor);
        self.armor -= soak;
        self.health = (self.health - (amount - soak)).max(0.0);
        self.since_hurt = 0.0;
    }

    /// Grenades fly, bounce and go off: each blast pushes things (`Sim::explode`, whose event
    /// hurts people) and wrecks vehicles near it (cars set burning go up after). Returns the
    /// blasts' centres.
    pub fn grenades_tick(&mut self, sim: &mut Sim, dt: f32, events: &mut Vec<SimEvent>) -> Vec<Vec3> {
        let mut blasts = Vec::new();
        for g in &mut self.grenades {
            g.fuse -= dt;
            g.vel.y -= 9.81 * dt;
            let step = g.vel * dt;
            let len = step.length();
            match sim.raycast_static(g.pos, step / len.max(1e-6), len + 0.06) {
                Some(h) if len > 1e-5 => {
                    // Bounce, losing most of the speed.
                    g.pos = h.point + h.normal * 0.07;
                    g.vel = (g.vel - 2.0 * g.vel.dot(h.normal) * h.normal) * 0.35;
                }
                _ => g.pos += step,
            }
            if g.fuse <= 0.0 {
                blasts.push(g.pos);
            }
        }
        self.grenades.retain(|g| g.fuse > 0.0);
        for &at in &blasts {
            sim.explode(at, BLAST, events);
            let near: Vec<(EntityId, f32)> = sim
                .state
                .entities
                .iter()
                .filter_map(|e| {
                    let v = e.vehicle.as_ref()?;
                    let d = e.pos.distance(at);
                    (d < BLAST).then(|| (e.id, v.def.health * 1.2 * (1.0 - d / BLAST)))
                })
                .collect();
            for (id, dmg) in near {
                sim.damage_vehicle(id, dmg);
            }
        }
        blasts
    }

    /// Someone else's shot (the police's): its tracer and where it struck.
    pub fn trace(&mut self, from: Vec3, to: Vec3, person: bool) {
        let normal = (from - to).normalize_or(Vec3::Y);
        self.traces.push(Trace { from, to, normal, person, age: 0.0 });
    }

    /// Regeneration and the fallen: health creeps back to half after 5 s unharmed; bodies are
    /// taken away after a while.
    pub fn update(&mut self, sim: &mut Sim, dt: f32) {
        self.since_hurt += dt;
        if self.since_hurt > 5.0 && self.health < MAX_HEALTH * 0.5 {
            self.health = (self.health + 8.0 * dt).min(MAX_HEALTH * 0.5);
        }
        for t in &mut self.traces {
            t.age += dt;
        }
        self.traces.retain(|t| t.age < if t.person { 0.3 } else { 6.0 });
        for f in &mut self.fallen {
            f.1 += dt;
        }
        let gone: Vec<EntityId> = self.fallen.iter().filter(|f| f.1 > BODY_TIME).map(|f| f.0).collect();
        for id in gone {
            sim.despawn(id);
        }
        self.fallen.retain(|f| f.1 <= BODY_TIME);
    }

    /// The reticle while aiming a gun.
    pub fn crosshair(&self, w: &WeaponDef, aiming: bool, moving: bool) -> Option<Crosshair> {
        (w.kind == Kind::Gun && aiming).then_some(Crosshair {
            spread: w.spread + self.bloom + if moving { 1.2 } else { 0.0 },
            hit: self.hit,
            kill: self.kill,
            target: false,
        })
    }

    /// Tracers for a frame or two, the muzzle flash, red puffs on people, marks on walls.
    pub fn draw(&self, draw: &mut DrawList) {
        for g in &self.grenades {
            draw.prims.push(PrimDraw {
                shape: pav_core::shape::Shape::Sphere { radius: 0.07 },
                pos: g.pos,
                rot: glam::Quat::IDENTITY,
                color: Color::hex("#3d4a2f"),
                emissive: if (g.fuse * 4.0).fract() < 0.2 { 0.8 } else { 0.0 },
                look: pav_core::shape::Look::Lit,
                shadow: true,
                glow: Default::default(),
            });
        }
        for t in &self.traces {
            if t.age < 0.06 {
                draw.tracers.push(TracerDraw { from: t.from, to: t.to, color: Color::hex("#ffe9a8"), width: 0.025 });
            }
            if t.person {
                let k = 1.0 - t.age / 0.3;
                draw.prims.push(PrimDraw {
                    shape: pav_core::shape::Shape::Sphere { radius: 0.08 + 0.12 * (1.0 - k) },
                    pos: t.to,
                    rot: glam::Quat::IDENTITY,
                    color: Color::hex("#c8102e"),
                    emissive: 0.4 * k,
                    look: pav_core::shape::Look::Unlit,
                    shadow: false,
                    glow: Default::default(),
                });
            } else {
                draw.decals.push(DecalDraw {
                    pos: t.to + t.normal * 0.01,
                    normal: t.normal,
                    yaw: 0.0,
                    size: Vec2::splat(0.1),
                    color: Color::hex("#2b2622"),
                    alpha: (1.0 - t.age / 6.0).clamp(0.0, 1.0) * 0.9,
                });
            }
        }
        if self.flash < 0.05 {
            if let Some(t) = self.traces.last() {
                draw.lights.push(LightDraw {
                    pos: t.from,
                    color: Color::hex("#ffc870"),
                    intensity: 6.0,
                    radius: 6.0,
                    shadows: false,
                });
            }
        }
    }
}

/// Turns the camera so its view ray points at `point` (`k` of the way: 1 snaps), a few rounds
/// since the eye moves as it turns.
fn look_at(cam: &mut OrbitCam, point: Vec3, k: f32) {
    for _ in 0..4 {
        let c = cam.intent();
        let have = (c.target - c.eye).normalize_or(Vec3::NEG_Z);
        let want = (point - c.eye).normalize_or(have);
        let yaw = |d: Vec3| d.x.atan2(-d.z);
        let mut dyaw = yaw(want) - yaw(have);
        dyaw = (dyaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let dpitch = want.y.clamp(-1.0, 1.0).asin() - have.y.clamp(-1.0, 1.0).asin();
        cam.yaw += dyaw * k;
        cam.elevation = (cam.elevation - dpitch * k).clamp(-0.15, 1.25);
    }
}

fn sound(cue: &str, pos: Vec3, gain: f32) -> SimEvent {
    SimEvent::Sound { cue: cue.into(), pos: Some(pos), gain, pitch: 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_weapons_match_the_wheel() {
        let def = def().unwrap();
        let icons: Vec<&str> = def.weapon.iter().take(BAT).map(|w| w.icon.as_str()).collect();
        let wheel: Vec<&str> = crate::hud::WEAPONS.iter().map(|w| w.1).collect();
        assert_eq!(icons, wheel, "weapons.toml lists the wheel's slots in order");
        assert_eq!(def.weapon[0].kind, Kind::Melee, "the first slot is the fists");
        let bat = &def.weapon[BAT];
        assert_eq!(
            (bat.id.as_str(), bat.icon.as_str(), bat.kind),
            ("bat", "fists", Kind::Melee),
            "the bat follows, in the fists' slot"
        );
        assert_eq!(def.weapon.len(), BAT + 1);
        for w in def.weapon.iter().filter(|w| w.kind == Kind::Gun) {
            assert!(w.damage > 0.0 && w.rpm > 0.0 && w.magazine > 0 && w.range > 0.0 && !w.cue.is_empty(), "{}", w.id);
        }
    }

    #[test]
    fn a_round_at_a_wheel_finds_its_tyre_and_at_a_lamp_the_lamp() {
        crate::register();
        let mut sim = Sim::new("freeway/roam", 1).unwrap();
        sim.step(&pav_core::InputFrame::default());
        let feet = sim.feet(sim.state.player.unwrap()).unwrap();
        let car = crate::vehicles::spawn(&mut sim, "okami_kaze", feet + Vec3::new(5.0, 0.05, 0.0), 0.4, None).unwrap();
        for _ in 0..10 {
            sim.step(&pav_core::InputFrame::default());
        }
        let e = sim.state.entities.get(car).unwrap();
        let view = e.vehicle.as_ref().unwrap().view(e.pos, e.rot);
        let at = |p: Vec3| e.pos + e.rot * (p - Vec3::Y * view.base);
        for (i, w) in view.wheels.iter().enumerate() {
            assert_eq!(struck_part(&sim, car, at(w.center)).map(|p| p.0), Some(Some(i)), "wheel {i}");
        }
        let lamp = at(view.head[0]);
        assert_eq!(struck_part(&sim, car, lamp), Some((None, Some(0))), "the left headlamp");
        assert_eq!(struck_part(&sim, car, e.pos + Vec3::Y * 3.0), Some((None, None)), "the roof: neither");
    }

    #[test]
    fn armour_soaks_and_health_comes_back_to_half() {
        let mut c = Combat { armor: 100.0, ..Default::default() };
        c.hurt_player(100.0);
        assert_eq!((c.health, c.armor), (170.0, 30.0));
        c.hurt_player(150.0);
        assert!((c.armor - 0.0).abs() < 1e-4 && (c.health - 50.0).abs() < 1e-3, "{c:?}");
        c.hurt_player(30.0);
        assert!((c.health - 20.0).abs() < 1e-3);
    }
}
