//! Traffic (PLAN.md §5.6.6, §6.6.9): cars driven along Paloma Bay's lane graph. Each car keeps
//! a route of lanes (random turns, or a bus's loop), follows the car ahead with the Intelligent
//! Driver Model, stops for red and amber lights, gives way inside junctions (unsignalised boxes,
//! left turns across oncoming traffic), waits for people on the road ahead, changes lanes past
//! slow cars, pulls over for sirens, flees gunfire, honks, backs up and goes around when stuck.
//! Near the player and the camera a car is a full physics vehicle steered by pure pursuit; far
//! away it runs on rails (kinematic, moved along its lane at its IDM speed). A population pass
//! keeps the count by the hour, spawning out of view; parked cars fill the curbside spots near
//! the player; buses run their loop and wait at the stops. Data: `content/freeway/traffic.toml`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use glam::{Vec2, Vec3};
use pav_core::entity::EntityId;
use pav_core::present::CameraIntent;
use pav_core::rng::Rng;
use pav_core::sim::Sim;
use pav_core::vehicle::VehicleInput;
use pav_world::City;
use pav_world::lanes::{LaneGraph, LaneKind, Light, Turn};
use serde::{Deserialize, Serialize};

use crate::drive::Car;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrafficDef {
    pub density: Density,
    pub drive: DriveDef,
    pub parked: Parked,
    pub bus: BusDef,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Density {
    pub max: usize,
    pub radius: f32,
    pub spawn: [f32; 2],
    pub despawn: f32,
    pub night: f32,
    pub full: f32,
    pub rail: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriveDef {
    pub gap: f32,
    pub headway: f32,
    pub accel: f32,
    pub decel: f32,
    pub personality: [f32; 2],
    pub honk_after: f32,
    pub reverse_after: f32,
    pub give_up: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parked {
    pub radius: f32,
    pub despawn: f32,
    pub share: f32,
    pub locked: f32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusDef {
    pub key: String,
    pub count: usize,
    pub dwell: f32,
    pub stops: Vec<[f32; 2]>,
}

/// The traffic data (`content/freeway/traffic.toml`, read again when its text changes).
pub fn def() -> Result<Arc<TrafficDef>> {
    static CACHE: Mutex<Option<(String, Arc<TrafficDef>)>> = Mutex::new(None);
    let text = crate::content::text("traffic.toml", include_str!("../../../content/freeway/traffic.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref()
        && *t == *text
    {
        return Ok(d.clone());
    }
    let d: TrafficDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/traffic.toml: {e}"))?;
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// How a car is simulated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    /// A physics vehicle steered with `VehicleInput`.
    Full,
    /// Kinematic, moved along its lane.
    Rail,
}

/// What a driver is up to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mood {
    Cruise,
    /// A siren behind: over to the curb lane and stopped until it passes.
    PullOver,
    /// Gunfire: away at speed, lights ignored.
    Flee,
    /// Just crashed: stopped for a moment.
    Shaken,
}

/// A bus's place on its loop.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct BusState {
    /// The stop it drives to next, and seconds left waiting at a stop.
    pub stop: usize,
    pub dwell: f32,
}

/// One car in traffic.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Agent {
    pub car: EntityId,
    pub driver: Option<EntityId>,
    pub key: String,
    pub lane: u32,
    /// Metres along `lane`, and the lanes it will take after it.
    pub along: f32,
    pub route: Vec<u32>,
    /// m/s along the lane (rails: the speed it is moved at).
    pub speed: f32,
    /// Its share of the speed limit.
    pub desire: f32,
    pub tier: Tier,
    pub mood: Mood,
    pub mood_t: f32,
    /// Seconds stopped with nothing that excuses it; backing up; since the last horn and lane
    /// change.
    pub blocked: f32,
    pub reverse: f32,
    pub honk: f32,
    pub change_t: f32,
    /// Waiting for a light, a person or a car that is itself waiting (not stuck).
    #[serde(default)]
    pub held: bool,
    #[serde(default)]
    pub bus: Option<BusState>,
    /// The player's own car on autopilot (bots, the attract mode): driven like traffic, never
    /// removed, its input handed to the player's controls.
    #[serde(default)]
    pub pilot: bool,
}

/// The cars driving around the player, the parked ones and their counters (part of the game's
/// state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Traffic {
    pub cars: Vec<Agent>,
    /// Parking spot index → its parked car.
    pub parked: BTreeMap<u32, EntityId>,
    /// Vehicles left behind (abandoned, carjacked, moved from their spot): removed when far and
    /// out of view.
    pub loose: Vec<EntityId>,
    /// Seconds to the next population pass.
    pub pass: f32,
    pub filled: bool,
    pub spawned: u32,
    pub despawned: u32,
    /// Soak counters (PLAN §7.5).
    pub stuck: u32,
    pub spawn_in_view: u32,
    pub despawn_in_view: u32,
    pub ai_crashes: u32,
    pub nan: u32,
    /// The autopilot's input for the player's car this tick.
    pub pilot_input: Option<VehicleInput>,
    /// The bus loop (derived from the city and the data).
    #[serde(skip)]
    pub bus_loop: Option<Arc<BusLoop>>,
}

/// A bus route: the lanes around the loop and the stops.
#[derive(Clone, Debug, Default)]
pub struct BusLoop {
    pub lanes: Vec<u32>,
    pub stops: Vec<Vec3>,
}

/// What traffic needs to know this tick.
pub struct Moment<'a> {
    pub city: &'a City,
    pub player: Vec3,
    pub camera: CameraIntent,
    /// Seconds on the signal clock.
    pub time: f32,
    pub hour: f32,
    pub dt: f32,
    /// The player's vehicle (an obstacle like any other, never driven by traffic).
    pub player_car: Option<EntityId>,
    /// Sirens sounding: where and which way they go.
    pub sirens: Vec<(Vec3, Vec3)>,
    /// People on foot near the player (feet).
    pub people: Vec<Vec3>,
    /// Gunfire this tick: where.
    pub gunfire: Option<Vec3>,
}

/// Whether the camera sees a point (in front, within a wide cone, not far off; close by always).
pub fn seen(cam: &CameraIntent, p: Vec3) -> bool {
    let d = p - cam.eye;
    let len = d.length();
    if len < 20.0 {
        return true;
    }
    let forward = (cam.target - cam.eye).normalize_or(Vec3::NEG_Z);
    len < 300.0 && (d / len).dot(forward) > 0.5
}

/// A lane traffic may drive (not closed, not a channel bed).
fn drivable(kind: LaneKind) -> bool {
    !matches!(kind, LaneKind::Channel)
}

/// The point `dist` metres past `along` on `lane`, carrying on through `route`, and the
/// direction of travel there.
fn ahead(g: &LaneGraph, lane: u32, along: f32, route: &[u32], dist: f32) -> (Vec3, Vec3) {
    let (mut l, mut d, mut i) = (lane, along + dist, 0);
    loop {
        let ln = &g.lanes[l as usize];
        if d <= ln.length || i >= route.len() {
            return ln.at(d.min(ln.length));
        }
        d -= ln.length;
        l = route[i];
        i += 1;
    }
}

/// Picks the next lane after `lane`: straight on most of the time, then right, then left;
/// U-turns only at dead ends.
fn pick_next(g: &LaneGraph, lane: u32, rng: &mut Rng) -> Option<u32> {
    let options: Vec<(u32, f32)> = g.lanes[lane as usize]
        .next
        .iter()
        .filter(|&&n| {
            let l = &g.lanes[n as usize];
            !l.closed && drivable(l.kind)
        })
        .map(|&n| {
            let w = match g.lanes[n as usize].turn {
                Turn::Straight | Turn::None => 3.0,
                Turn::Right => 1.5,
                Turn::Left => 1.0,
                Turn::UTurn => 0.02,
            };
            (n, w)
        })
        .collect();
    let total: f32 = options.iter().map(|o| o.1).sum();
    if total <= 0.0 {
        return None;
    }
    let mut r = rng.f32() * total;
    for (n, w) in &options {
        if r < *w {
            return Some(*n);
        }
        r -= w;
    }
    options.last().map(|o| o.0)
}

/// The Intelligent Driver Model: acceleration at speed `v` toward `v0` with an obstacle `gap`
/// metres ahead moving at its own speed.
fn idm(v: f32, v0: f32, gap: Option<(f32, f32)>, d: &DriveDef) -> f32 {
    let v0 = v0.max(0.5);
    let free = 1.0 - (v / v0).powi(4);
    let inter = match gap {
        Some((s, vl)) => {
            let s = s.max(0.1);
            let star = d.gap + (v * d.headway + v * (v - vl) / (2.0 * (d.accel * d.decel).sqrt())).max(0.0);
            (star / s).powi(2)
        }
        None => 0.0,
    };
    (d.accel * (free - inter)).clamp(-9.0, d.accel)
}

/// A small deterministic hash for per-spot choices (which spots hold a car, which are locked).
fn hash(a: u64, b: u64) -> f32 {
    let mut x = a.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ b.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 31)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// The cars on each lane this tick: (along, speed, half length, car).
type Occupancy = BTreeMap<u32, Vec<(f32, f32, f32, EntityId)>>;

/// The archetypes ambient traffic is drawn from, by weight (no sirens, no buses).
fn ambient_kinds() -> Vec<(String, f32)> {
    crate::vehicles::all()
        .map(|all| {
            all.iter()
                .filter(|a| a.traffic_weight > 0.0 && !a.def.siren && !a.helicopter() && a.def.class != "bus")
                .map(|a| (a.key.clone(), a.traffic_weight))
                .collect()
        })
        .unwrap_or_default()
}

fn pick_kind(kinds: &[(String, f32)], rng: &mut Rng) -> Option<String> {
    let total: f32 = kinds.iter().map(|k| k.1).sum();
    let mut r = rng.f32() * total;
    for (k, w) in kinds {
        if r < *w {
            return Some(k.clone());
        }
        r -= w;
    }
    kinds.last().map(|k| k.0.clone())
}

/// A driver: a person seated at the wheel (drawn as an occupant, pulled out by carjackers).
fn seat_driver(sim: &mut Sim, car: EntityId, at: Vec3) -> Option<EntityId> {
    let look = crate::peds::def().ok().and_then(|d| {
        let n = d.archetype.len() as u32;
        let a = d.archetype.get(sim.state.rng.below(n.max(1)) as usize)?.clone();
        Some(crate::peds::look(&a, &mut sim.state.rng))
    });
    let id = sim.spawn_npc("driver", at, 0.0, look.unwrap_or_default(), None, None);
    if sim.enter_vehicle_seat(id, car, 0) {
        Some(id)
    } else {
        sim.despawn(id);
        None
    }
}

impl Traffic {
    /// Stop driving `car` (carjacked, abandoned): it is left where it is. Returns its driver if
    /// still seated.
    pub fn release(&mut self, car: EntityId) -> Option<EntityId> {
        let i = self.cars.iter().position(|a| a.car == car)?;
        let a = self.cars.remove(i);
        self.loose.push(car);
        a.driver
    }

    /// Puts the player's car `car` on autopilot (random turns on the lane graph), or takes it
    /// off (`None`).
    pub fn autopilot(&mut self, sim: &mut Sim, city: &City, car: Option<EntityId>) {
        self.cars.retain(|a| !a.pilot);
        self.pilot_input = None;
        let Some(car) = car else { return };
        let Some(c) = Car::read(sim, car) else { return };
        let g = &city.lanes;
        let fwd = c.forward();
        let keep = |l: &pav_world::lanes::Lane| !l.closed && drivable(l.kind);
        let Some((lane, along, _)) = g.nearest(c.feet(), keep) else { return };
        // The nearest lane may run the other way: prefer one along the car's heading.
        let lane = if g.lanes[lane as usize].at(along).1.dot(fwd) < 0.0 {
            let keep2 = |l: &pav_world::lanes::Lane| {
                keep(l) && l.points.len() >= 2 && (l.points[1] - l.points[0]).normalize_or_zero().dot(fwd) > 0.3
            };
            g.nearest(c.feet(), keep2).map(|x| x.0).unwrap_or(lane)
        } else {
            lane
        };
        let along = g.lanes[lane as usize].project(c.feet()).0;
        let mut a = Agent {
            car,
            driver: None,
            key: String::new(),
            lane,
            along,
            route: Vec::new(),
            speed: c.speed(),
            desire: 1.0,
            tier: Tier::Full,
            mood: Mood::Cruise,
            mood_t: 0.0,
            blocked: 0.0,
            reverse: 0.0,
            honk: 0.0,
            change_t: 0.0,
            held: false,
            bus: None,
            pilot: true,
        };
        self.extend(&mut a, g, &mut sim.state.rng);
        self.cars.push(a);
    }

    /// A parked car taken from its spot (the player got in): no longer kept at the curb.
    pub fn unpark(&mut self, car: EntityId) {
        let before = self.parked.len();
        self.parked.retain(|_, id| *id != car);
        if self.parked.len() != before {
            self.loose.push(car);
        }
    }

    /// Crashes this tick: traffic cars that hit something stop for a moment (and drop to full
    /// physics); two traffic cars hitting each other count as an AI crash.
    pub fn crashes(&mut self, sim: &mut Sim, events: &[pav_core::frame::SimEvent]) {
        for ev in events {
            let pav_core::frame::SimEvent::Crash { vehicle, other, impulse, .. } = ev else { continue };
            if *impulse < 800.0 {
                continue;
            }
            let ours = |id: EntityId| self.cars.iter().any(|a| a.car == id && !a.pilot);
            if let Some(o) = other
                && ours(*vehicle)
                && ours(*o)
                && vehicle.0 < o.0
            {
                self.ai_crashes += 1;
            }
            for id in [Some(*vehicle), *other].into_iter().flatten() {
                if let Some(a) = self.cars.iter_mut().find(|a| a.car == id) {
                    if a.tier == Tier::Rail {
                        sim.set_vehicle_kinematic(id, false);
                        a.tier = Tier::Full;
                    }
                    if a.mood != Mood::Flee {
                        a.mood = Mood::Shaken;
                        a.mood_t = 2.5;
                    }
                }
            }
        }
    }

    /// Counts for `status` and the soaks.
    pub fn summary(&self) -> serde_json::Value {
        let n = |f: &dyn Fn(&Agent) -> bool| self.cars.iter().filter(|a| !a.pilot && f(a)).count();
        serde_json::json!({
            "cars": n(&|_| true),
            "autopilot": self.cars.iter().any(|a| a.pilot),
            "full": n(&|a| a.tier == Tier::Full),
            "rail": n(&|a| a.tier == Tier::Rail),
            "buses": n(&|a| a.bus.is_some()),
            "pulled_over": n(&|a| a.mood == Mood::PullOver),
            "fleeing": n(&|a| a.mood == Mood::Flee),
            "held": n(&|a| a.held),
            "parked": self.parked.len(),
            "loose": self.loose.len(),
            "spawned": self.spawned,
            "despawned": self.despawned,
            "stuck_cars": self.stuck,
            "spawn_in_view": self.spawn_in_view,
            "despawn_in_view": self.despawn_in_view,
            "ai_crashes": self.ai_crashes,
            "nan_positions": self.nan,
        })
    }

    /// The bus loop through the data's stops (None when a leg has no lane route).
    fn bus_loop(city: &City, d: &BusDef) -> Option<BusLoop> {
        let g = &city.lanes;
        if d.stops.len() < 2 {
            return None;
        }
        let keep = |l: &pav_world::lanes::Lane| !l.closed && !matches!(l.kind, LaneKind::Connector | LaneKind::Channel);
        let ends: Vec<(u32, Vec3)> = d
            .stops
            .iter()
            .map(|s| {
                let p = Vec3::new(s[0], city.terrain.height_at(Vec2::new(s[0], s[1])), s[1]);
                let (lane, along, _) = g.nearest(p, keep)?;
                Some((lane, g.lanes[lane as usize].at(along).0))
            })
            .collect::<Option<_>>()?;
        let mut lanes = Vec::new();
        for i in 0..ends.len() {
            let (a, b) = (ends[i].0, ends[(i + 1) % ends.len()].0);
            let leg = g.route_lanes(a, b);
            if leg.is_empty() {
                return None;
            }
            lanes.extend_from_slice(&leg[..leg.len() - 1]);
        }
        // Each stop where the loop passes nearest it (on the bus's own side of the road).
        let stops = ends
            .iter()
            .map(|(_, p)| {
                lanes
                    .iter()
                    .filter(|&&l| g.lanes[l as usize].kind != LaneKind::Connector)
                    .map(|&l| {
                        let ln = &g.lanes[l as usize];
                        let (along, off) = ln.project(*p);
                        (off, ln.at(along).0)
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .map(|x| x.1)
                    .unwrap_or(*p)
            })
            .collect();
        Some(BusLoop { lanes, stops })
    }

    /// One tick: the population pass twice a second, then every car's driving. Inputs for
    /// full-physics cars go into `out`; cars on rails are moved directly. `locked` gets the
    /// parked cars that spawn locked.
    pub fn tick(
        &mut self,
        sim: &mut Sim,
        m: &Moment,
        out: &mut BTreeMap<EntityId, VehicleInput>,
        locked: &mut BTreeSet<EntityId>,
    ) {
        let Ok(d) = def() else { return };
        if self.bus_loop.is_none() {
            self.bus_loop = Some(Arc::new(Self::bus_loop(m.city, &d.bus).unwrap_or_default()));
        }
        // Gone or wrecked cars leave traffic.
        let mut dead = Vec::new();
        self.cars.retain(|a| {
            let ok = sim.state.entities.get(a.car).and_then(|e| e.vehicle.as_ref()).is_some_and(|v| !v.state.wrecked);
            if !ok {
                dead.push((a.car, a.driver));
            }
            ok
        });
        for (car, driver) in dead {
            if sim.state.entities.get(car).is_some() {
                self.loose.push(car);
            }
            let _ = driver;
        }
        self.pass -= m.dt;
        if self.pass <= 0.0 {
            self.pass = 0.5;
            self.populate(sim, m, &d, locked);
        }
        self.drive(sim, m, &d, out);
    }

    /// Spawns and removes cars: moving traffic to the hour's count, parked cars near the player,
    /// the buses; anything far and out of view goes.
    fn populate(&mut self, sim: &mut Sim, m: &Moment, d: &TrafficDef, locked: &mut BTreeSet<EntityId>) {
        let g = &m.city.lanes;
        let player = m.player;
        let first = !self.filled;
        // Leave: moving cars beyond the radius, parked and loose ones beyond theirs.
        let mut gone = Vec::new();
        for a in self.cars.iter().filter(|a| !a.pilot) {
            let Some(e) = sim.state.entities.get(a.car) else { continue };
            let far = e.pos.distance(player) > d.density.despawn;
            let stuck = a.blocked > d.drive.give_up && !seen(&m.camera, e.pos);
            if (far && !seen(&m.camera, e.pos)) || stuck {
                gone.push((a.car, a.driver, stuck));
            }
        }
        for (car, driver, stuck) in gone {
            self.cars.retain(|a| a.car != car);
            if stuck {
                self.stuck += 1;
            }
            if let Some(dr) = driver {
                sim.despawn(dr);
            }
            sim.despawn(car);
            self.despawned += 1;
        }
        let mut leave = Vec::new();
        for (&spot, &car) in &self.parked {
            let Some(e) = sim.state.entities.get(car) else {
                leave.push((spot, None));
                continue;
            };
            let s = &g.parking[spot as usize];
            if Vec2::new(e.pos.x - s.pos.x, e.pos.z - s.pos.z).length() > 3.0 {
                // Moved from its spot: no longer kept here.
                leave.push((spot, Some((car, false))));
            } else if e.pos.distance(player) > d.parked.despawn && !seen(&m.camera, e.pos) {
                leave.push((spot, Some((car, true))));
            }
        }
        for (spot, what) in leave {
            self.parked.remove(&spot);
            match what {
                Some((car, true)) => {
                    sim.despawn(car);
                    locked.remove(&car);
                }
                Some((car, false)) => self.loose.push(car),
                None => {}
            }
        }
        self.loose.retain(|&car| {
            let Some(e) = sim.state.entities.get(car) else { return false };
            let ridden = e.vehicle.as_ref().is_some_and(|v| v.driver.is_some());
            if !ridden && Some(car) != m.player_car && e.pos.distance(player) > d.density.despawn && !seen(&m.camera, e.pos) {
                sim.despawn(car);
                return false;
            }
            true
        });
        // Parked cars at the curb spots near the player (each spot always has or lacks one).
        let seed = m.city.def.seed;
        for (i, s) in g.parking.iter().enumerate() {
            let i = i as u32;
            if self.parked.contains_key(&i) || s.pos.distance(player) > d.parked.radius {
                continue;
            }
            if hash(seed, i as u64) >= d.parked.share {
                continue;
            }
            if !first && seen(&m.camera, s.pos) && s.pos.distance(player) < d.parked.radius * 0.8 {
                continue;
            }
            // Not on top of something already there (the player's car, a loose car).
            if sim.state.entities.iter().any(|e| e.vehicle.is_some() && e.pos.distance(s.pos) < 3.0) {
                continue;
            }
            let kinds = ambient_kinds();
            let Some(key) = pick_kind(&kinds, &mut sim.state.rng) else { break };
            let paint = Some(sim.state.rng.below(8) as usize);
            if let Ok(car) = crate::vehicles::spawn(sim, &key, s.pos + Vec3::Y * 0.05, s.dir.x.atan2(s.dir.z), paint) {
                self.parked.insert(i, car);
                if hash(seed ^ 0x5eed, i as u64) < d.parked.locked {
                    locked.insert(car);
                }
            }
        }
        // Moving traffic: the hour's share of the cap.
        let want = (d.density.max as f32 * crate::peds::time_share(m.hour, d.density.night)).round() as usize;
        let buses = self.cars.iter().filter(|a| a.bus.is_some()).count();
        let loop_ok = self.bus_loop.as_ref().is_some_and(|b| !b.lanes.is_empty());
        if loop_ok && buses < d.bus.count {
            self.spawn_bus(sim, m, d, first);
        }
        let tries = if first { 120 } else { 6 };
        let kinds = ambient_kinds();
        for _ in 0..tries {
            if self.cars.iter().filter(|a| !a.pilot).count() >= want + buses || kinds.is_empty() {
                break;
            }
            let rng = &mut sim.state.rng;
            let a = rng.f32() * std::f32::consts::TAU;
            let lo = if first { 25.0 } else { d.density.spawn[0] };
            let r = rng.range(lo, d.density.spawn[1]);
            let p = player + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
            let keep = |l: &pav_world::lanes::Lane| !l.closed && drivable(l.kind) && l.kind != LaneKind::Connector;
            let Some((lane, along, off)) = g.nearest(p, keep) else { continue };
            if off > 25.0 {
                continue;
            }
            let ln = &g.lanes[lane as usize];
            // Not right before a junction (no room to stop for its light).
            let room = if ln.junction.is_some() { 30.0 } else { 6.0 };
            if ln.length < room + 8.0 {
                continue;
            }
            let along = along.clamp(4.0, ln.length - room);
            let (pos, dir) = ln.at(along);
            if pos.distance(player) < lo || (!first && seen(&m.camera, pos)) {
                continue;
            }
            if sim.state.entities.iter().any(|e| e.vehicle.is_some() && e.pos.distance(pos) < 14.0) {
                continue;
            }
            let Some(key) = pick_kind(&kinds, &mut sim.state.rng) else { break };
            let paint = Some(sim.state.rng.below(8) as usize);
            let Ok(car) = crate::vehicles::spawn(sim, &key, pos + Vec3::Y * 0.05, dir.x.atan2(dir.z), paint) else { continue };
            let driver = seat_driver(sim, car, pos);
            let desire = sim.state.rng.range(d.drive.personality[0], d.drive.personality[1]);
            let speed = ln.speed * desire * 0.8;
            let mut agent = Agent {
                car,
                driver,
                key,
                lane,
                along,
                route: Vec::new(),
                speed,
                desire,
                tier: Tier::Full,
                mood: Mood::Cruise,
                mood_t: 0.0,
                blocked: 0.0,
                reverse: 0.0,
                honk: 0.0,
                change_t: 0.0,
                held: false,
                bus: None,
                pilot: false,
            };
            self.extend(&mut agent, g, &mut sim.state.rng);
            if pos.distance(player) > d.density.full {
                sim.set_vehicle_kinematic(car, true);
                agent.tier = Tier::Rail;
            }
            self.cars.push(agent);
            self.spawned += 1;
        }
        self.filled = true;
    }

    /// Puts a bus on its loop somewhere out of view.
    fn spawn_bus(&mut self, sim: &mut Sim, m: &Moment, d: &TrafficDef, first: bool) {
        let Some(bl) = self.bus_loop.clone() else { return };
        let g = &m.city.lanes;
        let n = bl.lanes.len();
        let start = sim.state.rng.below(n as u32) as usize;
        for k in 0..n {
            let i = (start + k * 7) % n;
            let lane = bl.lanes[i];
            let ln = &g.lanes[lane as usize];
            if ln.kind == LaneKind::Connector || ln.length < 12.0 {
                continue;
            }
            let (pos, dir) = ln.at(ln.length * 0.5);
            let dist = pos.distance(m.player);
            if dist > d.density.radius || (!first && (seen(&m.camera, pos) || dist < d.density.spawn[0] * 0.7)) {
                continue;
            }
            if sim.state.entities.iter().any(|e| e.vehicle.is_some() && e.pos.distance(pos) < 18.0) {
                continue;
            }
            let Ok(car) = crate::vehicles::spawn(sim, &d.bus.key, pos + Vec3::Y * 0.05, dir.x.atan2(dir.z), None) else { return };
            let driver = seat_driver(sim, car, pos);
            // The next stop ahead along the loop.
            let stop = bl
                .stops
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    let da = (*a.1 - pos).dot(dir).rem_euclid(1e9);
                    let db = (*b.1 - pos).dot(dir).rem_euclid(1e9);
                    da.total_cmp(&db)
                })
                .map(|s| s.0)
                .unwrap_or(0);
            let route: Vec<u32> = (1..40).map(|j| bl.lanes[(i + j) % n]).collect();
            let mut agent = Agent {
                car,
                driver,
                key: d.bus.key.clone(),
                lane,
                along: ln.length * 0.5,
                route,
                speed: 0.0,
                desire: 0.95,
                tier: Tier::Full,
                mood: Mood::Cruise,
                mood_t: 0.0,
                blocked: 0.0,
                reverse: 0.0,
                honk: 0.0,
                change_t: 99.0,
                held: false,
                bus: Some(BusState { stop, dwell: 0.0 }),
                pilot: false,
            };
            if dist > d.density.full {
                sim.set_vehicle_kinematic(car, true);
                agent.tier = Tier::Rail;
            }
            self.cars.push(agent);
            self.spawned += 1;
            return;
        }
    }

    /// Keeps a few lanes of route ahead: random turns, or the bus loop.
    fn extend(&self, a: &mut Agent, g: &LaneGraph, rng: &mut Rng) {
        if a.bus.is_some()
            && let Some(bl) = &self.bus_loop
            && !bl.lanes.is_empty()
        {
            while a.route.len() < 40 {
                let last = *a.route.last().unwrap_or(&a.lane);
                let i = bl.lanes.iter().position(|&l| l == last).map(|i| (i + 1) % bl.lanes.len()).unwrap_or(0);
                a.route.push(bl.lanes[i]);
            }
            return;
        }
        while a.route.len() < 4 {
            let last = *a.route.last().unwrap_or(&a.lane);
            match pick_next(g, last, rng) {
                Some(n) => a.route.push(n),
                None => break,
            }
        }
    }

    /// Every car's driving this tick.
    fn drive(&mut self, sim: &mut Sim, m: &Moment, d: &TrafficDef, out: &mut BTreeMap<EntityId, VehicleInput>) {
        let g = &m.city.lanes;
        let dt = m.dt;
        if dt <= 0.0 {
            return;
        }
        // Where every car is on its lane (traffic first, then other vehicles near a lane).
        let mut occ: Occupancy = BTreeMap::new();
        let mut cars: BTreeMap<EntityId, Car> = BTreeMap::new();
        for a in &self.cars {
            if let Some(c) = Car::read(sim, a.car) {
                occ.entry(a.lane).or_default().push((a.along, a.speed, c.half.z, a.car));
                cars.insert(a.car, c);
            }
        }
        let ours: BTreeSet<EntityId> = self.cars.iter().map(|a| a.car).collect();
        let parked: BTreeSet<EntityId> = self.parked.values().copied().collect();
        for e in sim.state.entities.iter() {
            if e.vehicle.is_none() || ours.contains(&e.id) || parked.contains(&e.id) || e.pos.distance(m.player) > 200.0 {
                continue;
            }
            let Some(c) = Car::read(sim, e.id) else { continue };
            let keep = |l: &pav_world::lanes::Lane| !l.closed && drivable(l.kind);
            if let Some((lane, along, off)) = g.nearest(c.feet(), keep)
                && off < 3.0
            {
                occ.entry(lane).or_default().push((along, c.speed().max(0.0), c.half.z, e.id));
            }
        }
        for list in occ.values_mut() {
            list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.3.cmp(&b.3)));
        }
        // Junction boxes: which approaches have cars on their connectors.
        let mut boxes: BTreeMap<u32, Vec<(u8, EntityId)>> = BTreeMap::new();
        for a in &self.cars {
            let l = &g.lanes[a.lane as usize];
            if l.kind == LaneKind::Connector
                && let Some(j) = l.junction
            {
                let side = l.prev.first().and_then(|&p| g.lanes[p as usize].approach).unwrap_or(9);
                boxes.entry(j).or_default().push((side, a.car));
            }
        }
        // Waiting at a light or behind someone who is: shared one tick late down the queue.
        let held_before: BTreeSet<EntityId> = self.cars.iter().filter(|a| a.held).map(|a| a.car).collect();
        let sirens = m.sirens.clone();
        let gunfire = m.gunfire;
        let near_full = d.density.full;
        let back_rail = d.density.rail;
        let camera_target = m.camera.target;
        let mut rng = std::mem::replace(&mut sim.state.rng, Rng::new(0));
        for i in 0..self.cars.len() {
            let mut a = self.cars[i].clone();
            let Some(c) = cars.get(&a.car).cloned() else { continue };
            if !c.center.is_finite() {
                self.nan += 1;
                continue;
            }
            // Level of detail by distance to the player and to what the camera looks at.
            let near = c.center.distance(m.player).min(c.center.distance(camera_target));
            match a.tier {
                _ if a.pilot => {}
                Tier::Rail if near < near_full => {
                    sim.set_vehicle_kinematic(a.car, false);
                    a.tier = Tier::Full;
                }
                Tier::Full if near > back_rail && a.mood != Mood::Shaken => {
                    sim.set_vehicle_kinematic(a.car, true);
                    a.tier = Tier::Rail;
                }
                _ => {}
            }
            // Progress along the lane.
            if a.tier == Tier::Full {
                let (along, off) = g.lanes[a.lane as usize].project(c.feet());
                a.along = along;
                a.speed = c.speed();
                if off > 7.0 {
                    // Knocked off its lane: pick up the nearest lane that goes its way.
                    let fwd = c.forward();
                    let keep = |l: &pav_world::lanes::Lane| !l.closed && drivable(l.kind);
                    if let Some((lane, along, off2)) = g.nearest(c.feet(), keep)
                        && off2 < 7.0
                        && g.lanes[lane as usize].at(along).1.dot(fwd) > 0.3
                    {
                        a.lane = lane;
                        a.along = along;
                        a.route.clear();
                    }
                }
            }
            while a.along >= g.lanes[a.lane as usize].length - 0.3 && !a.route.is_empty() {
                a.along -= g.lanes[a.lane as usize].length;
                a.lane = a.route.remove(0);
                a.along = a.along.max(0.0);
                if a.tier == Tier::Full {
                    a.along = g.lanes[a.lane as usize].project(c.feet()).0;
                }
            }
            self.extend(&mut a, g, &mut rng);
            let lane = &g.lanes[a.lane as usize];
            // Moods.
            a.mood_t -= dt;
            a.honk -= dt;
            a.change_t -= dt;
            if let Some(at) = gunfire
                && at.distance(c.center) < 40.0
                && a.bus.is_none()
                && !a.pilot
            {
                a.mood = Mood::Flee;
                a.mood_t = 15.0;
            }
            let behind_siren = sirens.iter().any(|(p, dir)| {
                let to = c.center - *p;
                let fwd = c.forward();
                to.length() < 45.0 && to.dot(fwd) > 3.0 && dir.dot(fwd) > 0.5 && to.dot(Vec3::new(fwd.z, 0.0, -fwd.x)).abs() < 9.0
            });
            match a.mood {
                Mood::PullOver if !behind_siren && a.mood_t <= 0.0 => a.mood = Mood::Cruise,
                Mood::Shaken | Mood::Flee if a.mood_t <= 0.0 => a.mood = Mood::Cruise,
                _ => {}
            }
            if behind_siren && a.mood != Mood::Flee && !a.pilot {
                a.mood = Mood::PullOver;
                a.mood_t = 3.0;
                // Over to the curb lane.
                if let Some(r) = lane.right
                    && a.change_t <= 0.0
                {
                    a.along = g.lanes[r as usize].project(c.feet()).0;
                    a.lane = r;
                    a.route.clear();
                    self.extend(&mut a, g, &mut rng);
                    a.change_t = 1.5;
                }
            }
            let lane = &g.lanes[a.lane as usize];
            // The limit: this lane's and the next one's within braking reach (turns are slower).
            let look = (a.speed.max(0.0) * 4.0).clamp(25.0, 90.0);
            let mut limit = turn_speed(lane);
            {
                let mut dist = lane.length - a.along;
                for &n in a.route.iter().take(3) {
                    if dist > 40.0 {
                        break;
                    }
                    // Slow down in time for a turn ahead (comfortable braking over the distance).
                    let v = turn_speed(&g.lanes[n as usize]);
                    limit = limit.min((v * v + 2.0 * 2.0 * dist).sqrt());
                    dist += g.lanes[n as usize].length;
                }
            }
            let mut v0 = limit * a.desire * if a.mood == Mood::Flee { 1.35 } else { 1.0 };
            // Obstacles: the nearest car ahead, a light, a junction box, people on the road.
            let mut obstacle: Option<(f32, f32)> = None;
            let mut held = false;
            let mut consider = |gap: f32, speed: f32, excuse: bool| {
                if obstacle.is_none_or(|o| gap < o.0) {
                    obstacle = Some((gap, speed));
                    held = excuse;
                }
            };
            let mut offset = -a.along;
            let mut lane_id = a.lane;
            let mut k = 0;
            let my_half = c.half.z;
            loop {
                let ln = &g.lanes[lane_id as usize];
                if let Some(list) = occ.get(&lane_id) {
                    for &(along, speed, half, id) in list {
                        if id == a.car {
                            continue;
                        }
                        let gap = offset + along - my_half - half;
                        if offset + along > 0.1 && gap < look {
                            consider(gap.max(0.0), speed, held_before.contains(&id));
                        }
                    }
                }
                // A light at this lane's end.
                let to_line = offset + ln.length - ln.stop - my_half;
                // A line just behind the bumper still counts for a car crawling over it.
                let before = to_line > -0.5 || (to_line > -3.0 && a.speed < 3.0);
                if ln.kind != LaneKind::Connector && before && to_line < look && a.mood != Mood::Flee {
                    match g.light(lane_id, m.time) {
                        Some(Light::Red) => consider(to_line.max(0.0), 0.0, true),
                        Some(Light::Amber) if to_line > a.speed * a.speed / (2.0 * d.drive.decel) * 0.8 => {
                            consider(to_line.max(0.0), 0.0, true)
                        }
                        _ => {}
                    }
                    // Junction boxes: give way to cars already crossing from another side
                    // (unsignalised), or to oncoming traffic when turning left.
                    if let (Some(j), Some(side)) = (ln.junction, ln.approach)
                        && to_line < 12.0
                    {
                        let next = if k < a.route.len() { a.route.get(k).copied() } else { None };
                        let signalled = g.junctions[j as usize].signal.is_some();
                        let busy = boxes.get(&j).is_some_and(|v| v.iter().any(|(s, id)| *s != side && *id != a.car));
                        let left = next.is_some_and(|n| g.lanes[n as usize].turn == Turn::Left);
                        let opposite = (side + 2) % 4;
                        let oncoming = left
                            && signalled
                            && (boxes.get(&j).is_some_and(|v| v.iter().any(|(s, _)| *s == opposite))
                                || self.cars.iter().any(|o| {
                                    let ol = &g.lanes[o.lane as usize];
                                    o.car != a.car
                                        && ol.junction == Some(j)
                                        && ol.approach == Some(opposite)
                                        && ol.kind != LaneKind::Connector
                                        && ol.length - o.along < 35.0
                                        && o.speed > 1.5
                                }));
                        if (!signalled && busy) || oncoming {
                            consider(to_line.max(0.0), 0.0, true);
                        }
                    }
                }
                offset += ln.length;
                if offset > look || k >= a.route.len() {
                    break;
                }
                lane_id = a.route[k];
                k += 1;
            }
            // People on the road ahead: along the path the car will take, so a turn sees the
            // crossing it turns into.
            if !m.people.is_empty() {
                let feet = c.feet();
                let near: Vec<Vec3> = m.people.iter().copied().filter(|p| p.distance(feet) < 26.0).collect();
                if !near.is_empty() {
                    let reach = c.half.x + 1.1;
                    'path: for k in 1..=9 {
                        let s = k as f32 * 2.0;
                        let (pt, _) = ahead(g, a.lane, a.along, &a.route, s);
                        for p in &near {
                            if Vec2::new(p.x - pt.x, p.z - pt.z).length() < reach && (p.y - pt.y).abs() < 2.5 {
                                consider((s - c.half.z - 2.0).max(0.0), 0.0, true);
                                break 'path;
                            }
                        }
                    }
                }
            }
            // A bus stops at its stops.
            if let (Some(bs), Some(bl)) = (a.bus.as_mut(), self.bus_loop.as_ref())
                && let Some(stop) = bl.stops.get(bs.stop)
            {
                let to = *stop - c.feet();
                let z = to.dot(c.forward());
                if bs.dwell > 0.0 {
                    bs.dwell -= dt;
                    consider(0.0, 0.0, true);
                    if bs.dwell <= 0.0 {
                        bs.stop = (bs.stop + 1) % bl.stops.len();
                    }
                } else if (0.0..30.0).contains(&z) && to.dot(c.left()).abs() < 6.0 {
                    // Its doors (the middle) come to rest by the stop.
                    if z < 8.0 && a.speed.abs() < 0.8 {
                        bs.dwell = d.bus.dwell;
                    }
                    consider(z.max(0.0), 0.0, true);
                } else if z < -3.0 && to.length() < 12.0 {
                    // Went past it (pushed, rerouted): on to the next.
                    bs.stop = (bs.stop + 1) % bl.stops.len();
                }
            }
            if a.mood == Mood::PullOver || a.mood == Mood::Shaken {
                v0 = 0.0;
                held = true;
            }
            // Pass a slow car on a multi-lane road.
            if a.tier == Tier::Full
                && a.mood == Mood::Cruise
                && a.bus.is_none()
                && a.change_t <= 0.0
                && lane.length - a.along > 30.0
                && obstacle.is_some_and(|(gap, sp)| gap < 25.0 && sp < 0.7 * v0)
                && !held
            {
                for side in [lane.left, lane.right].into_iter().flatten() {
                    let s = &g.lanes[side as usize];
                    let at = s.project(c.feet()).0;
                    let clear = occ.get(&side).is_none_or(|v| v.iter().all(|(al, _, _, _)| *al < at - 12.0 || *al > at + 18.0));
                    if clear && s.length - at > 30.0 {
                        a.lane = side;
                        a.along = at;
                        a.route.clear();
                        self.extend(&mut a, g, &mut rng);
                        a.change_t = 6.0;
                        obstacle = None;
                        break;
                    }
                }
            }
            let accel = idm(a.speed.max(0.0), v0, obstacle, &d.drive);
            // Stuck: stopped with nothing that excuses it.
            let stopped = a.speed.abs() < 0.5 && v0 > 0.5;
            if stopped && !held {
                a.blocked += dt;
            } else if a.speed.abs() > 2.0 {
                a.blocked = 0.0;
            }
            a.held = held && a.speed.abs() < 1.0;
            let horn = a.blocked > d.drive.honk_after && a.honk <= 0.0 && a.tier == Tier::Full;
            if horn {
                a.honk = 4.0;
            }
            if a.blocked > d.drive.reverse_after && a.reverse <= 0.0 && a.tier == Tier::Full {
                // Back up only into clear road (nothing within 9 m behind on this lane).
                let clear = occ.get(&a.lane).is_none_or(|v| {
                    v.iter().all(|(al, _, half, id)| *id == a.car || *al > a.along || a.along - al - half - c.half.z > 9.0)
                });
                if clear {
                    a.reverse = 1.6;
                }
                a.blocked = d.drive.honk_after;
            }
            match a.tier {
                Tier::Rail => {
                    a.speed = (a.speed + accel * dt).clamp(0.0, v0.max(0.0) * 1.05);
                    a.along += a.speed * dt;
                    while a.along > g.lanes[a.lane as usize].length && !a.route.is_empty() {
                        a.along -= g.lanes[a.lane as usize].length;
                        a.lane = a.route.remove(0);
                    }
                    let (p, dir) = g.lanes[a.lane as usize].at(a.along);
                    sim.move_kinematic_vehicle(a.car, p, dir.x.atan2(dir.z));
                }
                Tier::Full => {
                    a.reverse -= dt;
                    let input = if a.reverse > 0.0 {
                        // Back up, wheels turned away from where it was going.
                        let (target, _) = ahead(g, a.lane, a.along, &a.route, 8.0);
                        let side = (target - c.feet()).dot(c.left());
                        // Gently (the brake pedal reverses), no faster than 2.5 m/s.
                        let pedal = if c.speed() > -2.5 { 0.35 } else { 0.0 };
                        VehicleInput { brake: pedal, steer: if side > 0.0 { 1.0 } else { -1.0 }, horn, ..Default::default() }
                    } else {
                        let pull = if a.mood == Mood::PullOver { lane.width * 0.45 } else { 0.0 };
                        steer_along(g, &a, &c, accel, v0, pull, horn)
                    };
                    if a.pilot {
                        self.pilot_input = Some(input);
                    } else {
                        out.insert(a.car, input);
                    }
                }
            }
            self.cars[i] = a;
        }
        sim.state.rng = rng;
    }
}

/// How fast a lane can be driven: its limit, and turns slower (left and right 7 m/s, a U-turn
/// 3.5 m/s).
fn turn_speed(l: &pav_world::lanes::Lane) -> f32 {
    match l.turn {
        Turn::Left | Turn::Right => l.speed.min(7.0),
        Turn::UTurn => l.speed.min(3.5),
        _ => l.speed,
    }
}

/// Pure pursuit toward a point ahead on the route, and the pedals for `accel`.
fn steer_along(g: &LaneGraph, a: &Agent, c: &Car, accel: f32, v0: f32, pull: f32, horn: bool) -> VehicleInput {
    let v = c.speed();
    let look = (3.5 + v.max(0.0) * 0.5).clamp(4.0, 20.0);
    let (mut target, dir) = ahead(g, a.lane, a.along, &a.route, look);
    if pull > 0.0 {
        // Toward the curb: the lane's right side.
        target += Vec3::new(-dir.z, 0.0, dir.x) * pull;
    }
    let to = target - c.center;
    let x = to.dot(c.left());
    let z = to.dot(c.forward()).max(0.5);
    let dist = (x * x + z * z).sqrt();
    // Curvature of the arc through the target, the wheel angle for it, as a share of full lock
    // (which the engine reduces with speed: allow for it).
    let wheelbase = (c.half.z * 1.3).max(2.0);
    let curvature = 2.0 * x / (dist * dist);
    let angle = (curvature * wheelbase).atan();
    let lock = 34f32.to_radians();
    let share = 1.0 - 0.65 * ((v - 8.0) / 24.0).clamp(0.0, 1.0);
    // Plus a pull back onto the lane's centre line (pure pursuit alone cuts corners).
    let (p, d) = g.lanes[a.lane as usize].at(a.along);
    let off = (c.feet() - p).dot(Vec3::new(d.z, 0.0, -d.x)) - pull;
    let steer = (-angle / (lock * share) + (off * 0.3).clamp(-0.35, 0.35)).clamp(-1.0, 1.0);
    // The engine's brake reverses below 1 m/s: slow and stopped, the handbrake holds instead.
    if (v0 < 0.3 || accel < 0.0) && v < 1.2 {
        return VehicleInput { steer, handbrake: true, horn, ..Default::default() };
    }
    let (throttle, brake) =
        if accel >= 0.0 { ((accel / 2.5).clamp(0.0, 1.0), 0.0) } else { (0.0, (-accel / 7.0).clamp(0.05, 1.0)) };
    VehicleInput { throttle, brake, steer, handbrake: false, horn, ..Default::default() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_loads() {
        let d = def().unwrap();
        assert!(d.density.max >= 30 && d.density.spawn[0] < d.density.spawn[1]);
        assert!(d.density.full < d.density.rail);
        assert!(d.bus.stops.len() >= 2);
    }

    #[test]
    fn idm_follows_and_stops() {
        let d = def().unwrap();
        // Free road: accelerates; at the limit: none.
        assert!(idm(5.0, 14.0, None, &d.drive) > 1.0);
        assert!(idm(14.0, 14.0, None, &d.drive).abs() < 1e-3);
        // A stopped car 10 m ahead at 12 m/s: hard braking.
        assert!(idm(12.0, 14.0, Some((10.0, 0.0)), &d.drive) < -4.0);
        // Stopped behind it at the gap: stays put.
        assert!(idm(0.0, 14.0, Some((d.drive.gap, 0.0)), &d.drive) <= 0.0);
    }
}
