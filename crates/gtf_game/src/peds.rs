//! Pedestrians (PLAN.md §5.6.7, §6.6.8, §6.6.10): people on Paloma Bay's sidewalks, crossings,
//! paths, parks and beach. The population manager keeps the zone's and the hour's share of
//! `peds.toml`'s cap around the player, spawning people out of view 60–115 m away and letting the
//! far, unseen ones go; each person walks to a destination over the walk tiles (A* on the
//! `WalkMap`), waits at a signalled crossing for its walk light, and lingers a while (an idle
//! clip) before the next walk. Driven like the player, one `InputFrame` per tick; every choice
//! draws on the sim's rng, so a run replays exactly.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::{Vec2, Vec3};
use pav_core::entity::EntityId;
use pav_core::input::buttons;
use pav_core::present::CameraIntent;
use pav_core::puppet::{BodyPlan, PuppetDef};
use pav_core::rng::Rng;
use pav_core::{InputFrame, Sim};
use pav_world::City;
use pav_world::walk::Walk;
use serde::{Deserialize, Serialize};

/// `peds.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PedsDef {
    pub density: Density,
    pub archetype: Vec<Archetype>,
}

/// How many people, where and when.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Density {
    pub max: usize,
    pub radius: f32,
    pub spawn: [f32; 2],
    pub despawn: f32,
    /// The share of the day's crowd still out at 03:00.
    pub night: f32,
    /// Crowd by zone, a share of `max` (others: 0.3).
    #[serde(default)]
    pub zones: BTreeMap<String, f32>,
}

/// A kind of person: palettes and proportions to draw a look from, walks, idles, pace, places.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Archetype {
    pub id: String,
    pub weight: f32,
    #[serde(default)]
    pub zones: Vec<String>,
    pub skin: Vec<String>,
    pub shirt: Vec<String>,
    pub pants: Vec<String>,
    pub shoes: Vec<String>,
    /// Standing height (m) and build (1 = average).
    pub height: [f32; 2],
    pub build: [f32; 2],
    #[serde(default)]
    pub walks: Vec<String>,
    #[serde(default)]
    pub idles: Vec<String>,
    /// Speed (m/s): walking, or running when `run`.
    pub pace: [f32; 2],
    #[serde(default)]
    pub run: bool,
}

/// The game's pedestrians (cached; re-read when the file changes during native hot reload).
pub fn def() -> Result<Arc<PedsDef>> {
    static CACHE: Mutex<Option<(String, Arc<PedsDef>)>> = Mutex::new(None);
    let text = crate::content::text("peds.toml", include_str!("../../../content/freeway/peds.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref() {
        if *t == *text {
            return Ok(d.clone());
        }
    }
    let d: PedsDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/peds.toml: {e}"))?;
    if d.archetype.is_empty() {
        return Err(anyhow!("content/freeway/peds.toml: no archetypes"));
    }
    for a in &d.archetype {
        if a.skin.is_empty() || a.shirt.is_empty() || a.pants.is_empty() || a.shoes.is_empty() {
            return Err(anyhow!("content/freeway/peds.toml: '{}' needs skin, shirt, pants and shoes", a.id));
        }
    }
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// The walking speed the movement tuning gives with FOCUS held, and the running speed.
const WALK: f32 = crate::player::WALK;
const RUN: f32 = crate::player::RUN;
/// Standing height of the default biped (m).
const BIPED_HEIGHT: f32 = 1.75;
/// People farther than this from the player go low detail (no character controller), and come
/// back to full detail nearer than `LOD_NEAR`.
const LOD_FAR: f32 = 30.0;
const LOD_NEAR: f32 = 24.0;

/// What a person is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Doing {
    Walk,
    /// At a crossing, waiting for the walk light.
    Wait,
    Linger,
    /// Running away from a threat (gunfire, a fight), or crouched where it could not get away.
    Flee,
    Cower,
}

/// One person.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ped {
    pub id: EntityId,
    pub arch: u16,
    pub doing: Doing,
    /// Tile centres to walk through, and the next one.
    pub path: Vec<Vec2>,
    pub next: usize,
    /// Seconds left lingering; since the last progress check, and where it stood then.
    pub timer: f32,
    pub check_t: f32,
    pub check: Vec2,
    pub run: bool,
}

/// Everyone out on foot (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Peds {
    pub list: Vec<Ped>,
    /// Seconds to the next population pass.
    pub pass: f32,
    /// Counters for soaks (PLAN.md §7.5): spawned, despawned, stuck and rerouted, failed spawns.
    pub spawned: u32,
    pub despawned: u32,
    pub stuck: u32,
    pub misses: u32,
}

/// What the population manager needs to know about the moment.
pub struct Moment<'a> {
    pub city: &'a City,
    pub player: Vec3,
    pub camera: CameraIntent,
    /// Game seconds (the signal clock) and the hour of the day.
    pub time: f32,
    pub hour: f32,
    pub dt: f32,
}

fn pick<'a>(v: &'a [String], rng: &mut Rng) -> &'a str {
    &v[rng.below(v.len() as u32) as usize]
}

/// A look drawn from an archetype.
pub fn look(a: &Archetype, rng: &mut Rng) -> PuppetDef {
    let mut p = PuppetDef::preset(BodyPlan::Biped);
    p.skin = pick(&a.skin, rng).into();
    p.shirt = pick(&a.shirt, rng).into();
    p.pants = pick(&a.pants, rng).into();
    p.shoes = pick(&a.shoes, rng).into();
    p.scale *= rng.range(a.height[0], a.height[1]) / BIPED_HEIGHT;
    let build = rng.range(a.build[0], a.build[1]);
    p.torso_radius *= build;
    p.hip_width *= build;
    p.shoulder_width *= build.sqrt();
    p.limb_radius *= build.sqrt();
    if !a.walks.is_empty() {
        p.walk_clip = pick(&a.walks, rng).into();
    }
    if !a.idles.is_empty() {
        p.idle_clip = pick(&a.idles, rng).into();
    }
    p
}

/// The day's crowd at `hour`: all of it in the afternoon, `night` of it in the small hours.
pub fn time_share(hour: f32, night: f32) -> f32 {
    let day = 0.5 + 0.5 * ((hour - 14.0) / 24.0 * std::f32::consts::TAU).cos();
    night + (1.0 - night) * day.sqrt()
}

/// Whether the camera sees a point (in front, within a wide cone, not far off).
fn seen(cam: &CameraIntent, p: Vec3) -> bool {
    let d = p - cam.eye;
    let forward = (cam.target - cam.eye).normalize_or(Vec3::NEG_Z);
    d.length() < 260.0 && d.normalize_or(Vec3::ZERO).dot(forward) > 0.45
}

impl Peds {
    /// People per tick: the population pass twice a second, then everyone's next step.
    pub fn tick(&mut self, sim: &mut Sim, m: &Moment, out: &mut BTreeMap<EntityId, InputFrame>) {
        let Ok(def) = def() else { return };
        self.pass -= m.dt;
        if self.pass <= 0.0 {
            self.pass = 0.5;
            self.populate(sim, &def, m);
        }
        let here = Vec2::new(m.player.x, m.player.z);
        for i in 0..self.list.len() {
            let id = self.list[i].id;
            let Some(feet) = sim.feet(id) else { continue };
            let at = Vec2::new(feet.x, feet.z);
            // Low detail (E11) beyond 30 m, full detail again within 24 m: only the people near
            // the player need the character controller (bumping, stepping round props).
            if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
                let d = at.distance(here);
                c.lod = if c.lod { d > LOD_NEAR } else { d > LOD_FAR };
            }
            let frame = self.step(i, sim, &def, m, at);
            out.insert(id, frame);
        }
    }

    /// One person's next move.
    fn step(&mut self, i: usize, sim: &mut Sim, def: &PedsDef, m: &Moment, pos: Vec2) -> InputFrame {
        let walk = &m.city.walk;
        let p = &mut self.list[i];
        if p.doing == Doing::Cower {
            p.timer -= m.dt;
            if p.timer <= 0.0 {
                p.doing = Doing::Linger;
                p.timer = 1.0;
                p.run = def.archetype[p.arch as usize].run;
            }
            return InputFrame { held: buttons::CROUCH, ..Default::default() };
        }
        if p.doing == Doing::Linger {
            p.timer -= m.dt;
            if p.timer > 0.0 {
                return InputFrame::default();
            }
            let to = destination(walk, pos, &mut sim.state.rng);
            match to.map(|to| walk.path(pos, to, 6000)) {
                Some(path) if path.len() >= 2 => {
                    p.path = path;
                    p.next = 1;
                    p.doing = Doing::Walk;
                    p.check = pos;
                    p.check_t = 0.0;
                }
                _ => p.timer = 2.0,
            }
            return InputFrame::default();
        }
        while p.next < p.path.len() && p.path[p.next].distance(pos) < 0.7 {
            p.next += 1;
        }
        if p.next >= p.path.len() {
            let a = &def.archetype[p.arch as usize];
            // A flight ends in a breather, then the walk resumes at the usual pace.
            p.run = a.run;
            p.doing = Doing::Linger;
            p.timer = sim.state.rng.range(4.0, 16.0) * if a.run { 0.3 } else { 1.0 };
            return InputFrame::default();
        }
        let w = p.path[p.next];
        let fleeing = p.doing == Doing::Flee;
        // A signalled crossing ahead: step off the kerb only on the walk light (unless fleeing).
        if let (Walk::Crossing { junction, ns }, false, false) =
            (walk.at(w), matches!(walk.at(pos), Walk::Crossing { .. }), fleeing)
        {
            let plan = m.city.lanes.junctions.get(junction as usize).and_then(|j| j.signal.as_ref());
            if plan.is_some_and(|s| s.walk(ns, m.time).is_none()) {
                p.doing = Doing::Wait;
                p.check = pos;
                p.check_t = 0.0;
                return InputFrame::default();
            }
        }
        if !fleeing {
            p.doing = Doing::Walk;
        }
        // Stuck: little progress for four seconds means a new destination.
        p.check_t += m.dt;
        if p.check_t > 4.0 {
            if p.check.distance(pos) < 0.4 {
                self.stuck += 1;
                let p = &mut self.list[i];
                p.doing = Doing::Linger;
                p.timer = 0.5;
                return InputFrame::default();
            }
            p.check = pos;
            p.check_t = 0.0;
        }
        let dir = (w - pos).normalize_or(Vec2::ZERO);
        let held = if p.run { 0 } else { buttons::FOCUS };
        InputFrame { move_dir: dir, held, ..Default::default() }
    }

    /// The population pass: drop people who are gone or far and unseen, then spawn toward the
    /// zone's and hour's share of the cap.
    fn populate(&mut self, sim: &mut Sim, def: &PedsDef, m: &Moment) {
        let d = &def.density;
        let here = Vec2::new(m.player.x, m.player.z);
        let mut kept = Vec::with_capacity(self.list.len());
        for p in std::mem::take(&mut self.list) {
            let Some(feet) = sim.feet(p.id) else { continue };
            let far = Vec2::new(feet.x, feet.z).distance(here) > d.despawn;
            if far && !seen(&m.camera, feet + Vec3::Y) {
                sim.despawn(p.id);
                self.despawned += 1;
            } else {
                kept.push(p);
            }
        }
        self.list = kept;
        let share = area_share(m.city, here, d) * time_share(m.hour, d.night);
        let target = ((d.max as f32 * share).round() as usize).min(d.max);
        // A new scene fills its streets at once, in view too (from 20 m out); afterwards people
        // arrive out of view in the spawn band, a couple per pass.
        let fill = self.spawned == 0;
        let (mut tries, mut made) = (0, 0);
        let (max_tries, max_made) = if fill { (600, target) } else { (24, 2) };
        while self.list.len() < target && tries < max_tries && made < max_made {
            tries += 1;
            if self.spawn(sim, def, m, fill) {
                made += 1;
            } else {
                self.misses += 1;
            }
        }
    }

    /// One person somewhere out of view in the spawn band (or anywhere from 20 m out when filling a
    /// new scene), of a kind that belongs there.
    fn spawn(&mut self, sim: &mut Sim, def: &PedsDef, m: &Moment, fill: bool) -> bool {
        let d = &def.density;
        let rng = &mut sim.state.rng;
        let a = rng.f32() * std::f32::consts::TAU;
        // Filling: evenly in distance, so the streets nearest the player are the busiest.
        let r = if fill { rng.range(15.0, d.radius) } else { rng.range(d.spawn[0], d.spawn[1]) };
        let p = Vec2::new(m.player.x, m.player.z) + Vec2::new(a.cos(), a.sin()) * r;
        let walk = &m.city.walk;
        let tile = walk.tile_of(p);
        if !matches!(walk.get(tile), Walk::Sidewalk | Walk::Path | Walk::Open) {
            return false;
        }
        let at = walk.centre(tile);
        let feet = Vec3::new(at.x, m.city.ground_y(at), at.y);
        if !fill && seen(&m.camera, feet + Vec3::Y) {
            return false;
        }
        let zone = m.city.zone_at(at).unwrap_or("");
        let fits: Vec<(usize, f32)> = def
            .archetype
            .iter()
            .enumerate()
            .filter(|(_, a)| a.zones.is_empty() || a.zones.iter().any(|z| z == zone))
            .map(|(i, a)| (i, a.weight.max(0.0)))
            .collect();
        let total: f32 = fits.iter().map(|f| f.1).sum();
        if total <= 0.0 {
            return false;
        }
        let mut roll = rng.f32() * total;
        let arch = fits.iter().find(|(_, w)| {
            roll -= w;
            roll <= 0.0
        });
        let arch = arch.map_or(fits[0].0, |f| f.0);
        let a = &def.archetype[arch];
        let puppet = look(a, rng);
        let pace = rng.range(a.pace[0], a.pace[1]);
        let facing = rng.f32() * std::f32::consts::TAU;
        let linger = rng.range(0.0, 3.0);
        let id = sim.spawn_npc(&format!("ped:{}", a.id), feet, facing, puppet, None, None);
        if let Some(c) = sim.state.entities.get_mut(id).and_then(|e| e.character.as_mut()) {
            c.haste = pace / if a.run { RUN } else { WALK } - 1.0;
        }
        self.list.push(Ped {
            id,
            arch: arch as u16,
            doing: Doing::Linger,
            path: Vec::new(),
            next: 0,
            timer: linger,
            check_t: 0.0,
            check: at,
            run: a.run,
        });
        self.spawned += 1;
        true
    }

    /// A threat at `at`: everyone within `radius` runs away from it along the walk tiles (or
    /// cowers where there is nowhere to go).
    pub fn scare(&mut self, sim: &mut Sim, walk: &pav_world::walk::WalkMap, at: Vec3, radius: f32) {
        let threat = Vec2::new(at.x, at.z);
        for p in &mut self.list {
            if matches!(p.doing, Doing::Flee | Doing::Cower) {
                continue;
            }
            let Some(feet) = sim.feet(p.id) else { continue };
            let pos = Vec2::new(feet.x, feet.z);
            if pos.distance(threat) > radius {
                continue;
            }
            let away = (pos - threat).normalize_or(Vec2::X);
            let rng = &mut sim.state.rng;
            let mut route = Vec::new();
            for _ in 0..6 {
                let a = away.y.atan2(away.x) + (rng.f32() - 0.5) * 2.4;
                let to = pos + Vec2::new(a.cos(), a.sin()) * rng.range(35.0, 80.0);
                let t = walk.tile_of(to);
                if matches!(walk.get(t), Walk::Sidewalk | Walk::Path | Walk::Open) {
                    route = walk.path(pos, walk.centre(t), 4000);
                    if route.len() >= 2 {
                        break;
                    }
                }
            }
            if route.len() >= 2 {
                p.path = route;
                p.next = 1;
                p.doing = Doing::Flee;
                p.run = true;
            } else {
                p.doing = Doing::Cower;
                p.timer = rng.range(6.0, 12.0);
            }
            p.check = pos;
            p.check_t = 0.0;
        }
    }

    /// Someone left the crowd (killed, arrested): no longer walked by the manager.
    pub fn forget(&mut self, id: EntityId) {
        self.list.retain(|p| p.id != id);
    }

    /// How many are doing what: (walking, waiting at a crossing, lingering).
    pub fn counts(&self) -> (usize, usize, usize) {
        let n = |d: Doing| self.list.iter().filter(|p| p.doing == d).count();
        (n(Doing::Walk), n(Doing::Wait), n(Doing::Linger))
    }
}

/// The crowd share of the zones around a point (a 5 × 5 grid 30 m apart, roads skipped).
fn area_share(city: &City, at: Vec2, d: &Density) -> f32 {
    let (mut sum, mut n) = (0.0, 0);
    for z in -2..=2 {
        for x in -2..=2 {
            if let Some(zone) = city.zone_at(at + Vec2::new(x as f32, z as f32) * 30.0) {
                sum += d.zones.get(zone).copied().unwrap_or(0.3);
                n += 1;
            }
        }
    }
    if n == 0 { 0.3 } else { sum / n as f32 }
}

/// A walkable tile 30–150 m from `from` (a few tries), as a place to walk to.
fn destination(walk: &pav_world::walk::WalkMap, from: Vec2, rng: &mut Rng) -> Option<Vec2> {
    for _ in 0..8 {
        let a = rng.f32() * std::f32::consts::TAU;
        let r = rng.range(30.0, 150.0);
        let p = from + Vec2::new(a.cos(), a.sin()) * r;
        let t = walk.tile_of(p);
        if matches!(walk.get(t), Walk::Sidewalk | Walk::Path | Walk::Open) {
            return Some(walk.centre(t));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_archetypes_load_and_make_many_distinct_people() {
        let def = def().unwrap();
        assert!(def.archetype.len() >= 8);
        let mut rng = Rng::new(5);
        let mut looks = std::collections::BTreeSet::new();
        for a in &def.archetype {
            for _ in 0..20 {
                let p = look(a, &mut rng);
                looks.insert(format!("{}{}{}{}{:.2}", p.skin, p.shirt, p.pants, p.shoes, p.scale));
            }
        }
        assert!(looks.len() >= 30, "{} distinct looks", looks.len());
    }

    #[test]
    fn fewer_people_are_out_at_night() {
        assert!((time_share(14.0, 0.35) - 1.0).abs() < 1e-3);
        assert!((time_share(2.0, 0.35) - 0.35).abs() < 1e-3);
        assert!(time_share(20.0, 0.35) > time_share(23.0, 0.35));
    }
}
