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
    /// A fleeing car overtaking: seconds left out in the next lane over (oncoming traffic too).
    #[serde(default)]
    pub passing: f32,
    /// A mission's orders: drive to `goal` along the lanes and stop there (`arrived` once
    /// stopped by it; `planned` once the route to it is laid), or run from `away` (moved each
    /// tick by the mission; with a goal too, it drives there like a getaway driver — through
    /// lights, out past slower cars: a race). A mission's car is never removed by the population
    /// pass.
    #[serde(default)]
    pub goal: Option<Vec3>,
    #[serde(default)]
    pub away: Option<Vec3>,
    #[serde(default)]
    pub mission: bool,
    #[serde(default)]
    pub arrived: bool,
    #[serde(default)]
    pub planned: bool,
    /// Lanes a goal-directed driver found blocked (a police car across the road): its route
    /// to the goal goes round them. Seconds sat behind the same stopped car.
    #[serde(default)]
    pub avoid: Vec<u32>,
    #[serde(default)]
    pub jam: f32,
    /// The lane its route to the goal ends on (none: no way found yet), and seconds before
    /// it looks for one again.
    #[serde(default)]
    pub end: Option<u32>,
    #[serde(default)]
    pub plan_wait: f32,
    /// What it is braking for this tick (the `traffic` tool shows it).
    #[serde(skip)]
    pub why: &'static str,
}

/// The cars driving around the player, the parked ones and their counters (part of the game's
/// state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Traffic {
    pub cars: Vec<Agent>,
    /// A mission's share of the usual moving traffic (None: all of it).
    pub thin: Option<f32>,
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
    /// Soak counters (PLAN §7.5): cars given up as stuck and traffic hitting traffic — the
    /// traffic's own (`stuck`, `ai_crashes`), and an incident's (`stuck_incident`: a police car,
    /// a wreck or an abandoned car within 40 m; `incident_crashes`: one of the two fleeing, pulled
    /// over or shaken).
    pub stuck: u32,
    pub spawn_in_view: u32,
    pub despawn_in_view: u32,
    pub ai_crashes: u32,
    pub nan: u32,
    #[serde(default)]
    pub stuck_incident: u32,
    #[serde(default)]
    pub incident_crashes: u32,
    /// Pairs of traffic cars that collided lately (ids, tick).
    #[serde(default)]
    pub crashed: Vec<(u32, u32, u64)>,
    /// Where the first AI crashes happened and between whom, and where cars were given up as
    /// stuck (for the soak report).
    #[serde(skip)]
    pub crash_sites: Vec<String>,
    #[serde(skip)]
    pub stuck_sites: Vec<String>,
    /// The autopilot's input for the player's car this tick.
    pub pilot_input: Option<VehicleInput>,
    /// The autopilot is running from this point (the police): it ignores the lights, drives
    /// hard and turns away from it at every junction.
    pub escape: Option<Vec3>,
    /// Fleeing cars bearing down on people this tick (they honk; the people jump clear).
    #[serde(skip)]
    pub honks: Vec<Vec3>,
    /// The bus loop (derived from the city and the data).
    #[serde(skip)]
    pub bus_loop: Option<Arc<BusLoop>>,
    /// Lanes that start or end where others do (derived from the lane graph).
    #[serde(skip)]
    pub siblings: Option<Arc<Siblings>>,
}

/// Lanes that share their start (turns fanning out of one approach) or their end (connectors and
/// ramps merging into one lane): cars on one are seen from the others where they overlap.
#[derive(Debug, Default)]
pub struct Siblings {
    lanes: usize,
    start: BTreeMap<u32, Vec<u32>>,
    end: BTreeMap<u32, Vec<u32>>,
}

impl Siblings {
    fn of(g: &LaneGraph) -> Siblings {
        let mut by_prev: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        let mut by_next: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for (i, l) in g.lanes.iter().enumerate() {
            if let [p] = l.prev[..] {
                by_prev.entry(p).or_default().push(i as u32);
            }
            if let [n] = l.next[..] {
                by_next.entry(n).or_default().push(i as u32);
            }
        }
        let pair_up = |groups: BTreeMap<u32, Vec<u32>>| {
            let mut out: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            for list in groups.values().filter(|l| l.len() > 1) {
                for &a in list {
                    out.insert(a, list.iter().copied().filter(|&b| b != a).collect());
                }
            }
            out
        };
        Siblings { lanes: g.lanes.len(), start: pair_up(by_prev), end: pair_up(by_next) }
    }
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
    /// The police's vehicles (the autopilot on the run does not brake for them: that is how a
    /// PIT stops a car).
    pub police: Vec<EntityId>,
    /// Police officers on foot (feet): the autopilot on the run leans on the horn and drives on
    /// rather than waiting for them to step aside.
    pub officers: Vec<Vec3>,
    /// The player is wanted: the autopilot drives like a getaway driver (through the lights,
    /// round slower cars, never sitting still).
    pub wanted: bool,
}

/// Whether the camera sees a point (in front, within a wide cone, not far off; close by always).
/// Everyone sitting in `car` but the player.
pub fn occupants(sim: &Sim, car: EntityId) -> Vec<EntityId> {
    let Some(v) = sim.state.entities.get(car).and_then(|e| e.vehicle.as_ref()) else { return Vec::new() };
    v.driver.into_iter().chain(v.passengers.iter().flatten().copied()).filter(|&id| Some(id) != sim.state.player).collect()
}

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
/// Whether every way on from `lane` runs off the map (the freeway's far end) or stops within
/// `depth` lanes: the player's autopilot keeps out of such roads.
fn dead_end(g: &LaneGraph, lane: u32, depth: u32) -> bool {
    // (A road that goes on only by turning round, at the map's edge or a cul-de-sac, ends.)
    let open: Vec<u32> = g.lanes[lane as usize]
        .next
        .iter()
        .copied()
        .filter(|&n| {
            let l = &g.lanes[n as usize];
            !l.closed && drivable(l.kind) && l.turn != Turn::UTurn
        })
        .collect();
    if open.is_empty() {
        return true;
    }
    depth > 0 && open.iter().all(|&n| dead_end(g, n, depth - 1))
}

/// Whether there is room to move over into a lane at `at` (its occupants): nobody beside or just
/// ahead, and nobody behind closing faster than they could brake for (two seconds of the speed
/// they gain on a car going `speed`).
fn gap_beside(cars: Option<&Vec<(f32, f32, f32, EntityId)>>, me: EntityId, at: f32, speed: f32) -> bool {
    cars.is_none_or(|v| {
        v.iter().all(|&(al, sp, half, id)| {
            let behind = 6.0 + half + ((sp - speed) * 2.0).max(0.0) + sp * 0.5;
            id == me || al < at - behind || al > at + 10.0 + half
        })
    })
}

/// Whether `lane` leads on to an open lane.
fn goes_on(g: &LaneGraph, lane: u32) -> bool {
    g.lanes[lane as usize].next.iter().any(|&n| {
        let l = &g.lanes[n as usize];
        !l.closed && drivable(l.kind)
    })
}

/// For a lane that goes nowhere: the neighbour to move into, toward the nearest lane beside it
/// that does go on (the outer lane at the last exit before the closed spur).
fn way_on(g: &LaneGraph, lane: u32) -> Option<u32> {
    let l = &g.lanes[lane as usize];
    for (first, right) in [(l.right, true), (l.left, false)] {
        let mut at = first;
        for _ in 0..4 {
            let Some(id) = at else { break };
            if goes_on(g, id) {
                return first;
            }
            let x = &g.lanes[id as usize];
            at = if right { x.right } else { x.left };
        }
    }
    None
}

/// When a car reaches the stop line `to_line` metres ahead at `speed`, in half-second steps (the
/// order of who goes first at an unsignalised junction).
fn first_come(to_line: f32, speed: f32) -> i32 {
    (to_line.max(0.0) / speed.max(1.0) / 0.5) as i32
}

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
    crate::world::ambient(sim, id);
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
    pub fn release(&mut self, sim: &mut Sim, car: EntityId) -> Option<EntityId> {
        let i = self.cars.iter().position(|a| a.car == car)?;
        let a = self.cars.remove(i);
        self.loose.push(car);
        // Off the rails: someone else drives it now.
        if a.tier == Tier::Rail {
            sim.set_vehicle_kinematic(car, false);
        }
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
        let Some((lane, along, _)) = g.nearest_level(c.feet(), 4.0, keep).or_else(|| g.nearest(c.feet(), keep)) else { return };
        // The nearest lane may run the other way: prefer one along the car's heading.
        let lane = if g.lanes[lane as usize].at(along).1.dot(fwd) < 0.0 {
            let keep2 = |l: &pav_world::lanes::Lane| {
                keep(l) && l.points.len() >= 2 && (l.points[1] - l.points[0]).normalize_or_zero().dot(fwd) > 0.3
            };
            g.nearest_level(c.feet(), 4.0, keep2).map(|x| x.0).unwrap_or(lane)
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
            passing: 0.0,
            goal: None,
            away: None,
            mission: false,
            arrived: false,
            planned: false,
            avoid: Vec::new(),
            jam: 0.0,
            why: "",
            end: None,
            plan_wait: 0.0,
        };
        self.extend(&mut a, g, &mut sim.state.rng);
        self.cars.push(a);
    }

    /// Puts `car` (with `driver` at the wheel) under a mission's orders: to `goal` along the
    /// lanes and stop there, or run from `away` (else cruise), at `desire` × the limits. A car
    /// already in traffic keeps its place and takes the new orders.
    #[allow(clippy::too_many_arguments)]
    pub fn command(
        &mut self,
        sim: &mut Sim,
        city: &City,
        car: EntityId,
        driver: Option<EntityId>,
        goal: Option<Vec3>,
        away: Option<Vec3>,
        desire: f32,
    ) {
        if let Some(a) = self.cars.iter_mut().find(|a| a.car == car) {
            a.goal = goal;
            a.away = away;
            a.desire = desire;
            a.mission = true;
            a.arrived = false;
            a.planned = false;
            a.route.clear();
            a.avoid.clear();
            a.jam = 0.0;
            a.end = None;
            a.plan_wait = 0.0;
            return;
        }
        self.parked.retain(|_, id| *id != car);
        self.loose.retain(|id| *id != car);
        let Some(c) = Car::read(sim, car) else { return };
        let g = &city.lanes;
        let fwd = c.forward();
        let keep = |l: &pav_world::lanes::Lane| {
            !l.closed && drivable(l.kind) && l.points.len() >= 2 && (l.points[1] - l.points[0]).normalize_or_zero().dot(fwd) > 0.3
        };
        let any = |l: &pav_world::lanes::Lane| !l.closed && drivable(l.kind);
        let Some((lane, _, _)) = g.nearest_level(c.feet(), 4.0, keep).or_else(|| g.nearest_level(c.feet(), 4.0, any)) else {
            return;
        };
        let along = g.lanes[lane as usize].project(c.feet()).0;
        let mut a = Agent {
            car,
            driver,
            key: sim.state.entities.get(car).map(|e| e.name.clone()).unwrap_or_default(),
            lane,
            along,
            route: Vec::new(),
            speed: c.speed(),
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
            passing: 0.0,
            goal,
            away,
            mission: true,
            arrived: false,
            planned: false,
            avoid: Vec::new(),
            jam: 0.0,
            why: "",
            end: None,
            plan_wait: 0.0,
        };
        if c.speed().abs() < 0.1 {
            sim.set_vehicle_kinematic(car, false);
        }
        self.extend(&mut a, g, &mut sim.state.rng);
        self.cars.push(a);
    }

    /// Moves a mission runner's point to run from (the player, each tick).
    pub fn run_from(&mut self, car: EntityId, from: Vec3) {
        if let Some(a) = self.cars.iter_mut().find(|a| a.car == car && a.away.is_some()) {
            a.away = Some(from);
        }
    }

    /// Whether a car under orders has stopped at its destination.
    pub fn arrived(&self, car: EntityId) -> bool {
        self.cars.iter().any(|a| a.car == car && a.arrived)
    }

    /// Takes a car out of the mission's hands (and out of traffic): it stays where it is.
    pub fn dismiss(&mut self, car: EntityId) {
        self.cars.retain(|a| a.car != car);
    }

    /// The player's car on autopilot drives to `goal` and stops there (None: cruises on), at
    /// `desire` × the limits — above 1.1 in a hurry (through lights, past slower cars).
    pub fn pilot_goal(&mut self, goal: Option<Vec3>, desire: f32) {
        if let Some(a) = self.cars.iter_mut().find(|a| a.pilot) {
            a.away = goal.filter(|_| desire > 1.1);
            if a.goal.is_some_and(|g| goal.is_some_and(|n| n.distance(g) < 1.0)) {
                a.desire = desire;
                return;
            }
            a.goal = goal;
            a.desire = desire;
            a.arrived = false;
            a.planned = false;
            a.route.clear();
            a.avoid.clear();
            a.jam = 0.0;
            a.end = None;
            a.plan_wait = 0.0;
        }
    }

    /// Whether the player's car on autopilot has stopped at its destination.
    pub fn pilot_arrived(&self) -> bool {
        self.cars.iter().any(|a| a.pilot && a.arrived)
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
    /// physics); two traffic cars hitting each other count as an AI crash — an incident's when one
    /// of them was fleeing, pulled over or shaken, or it happened within 40 m of a chase (`hot`:
    /// the police vehicles, and the player while wanted).
    pub fn crashes(&mut self, sim: &mut Sim, events: &[pav_core::frame::SimEvent], hot: &[Vec3]) {
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
                let calm = |id: EntityId| self.cars.iter().any(|a| a.car == id && a.mood == Mood::Cruise);
                // Two cars that keep bumping (wedged, nudging free) are one crash.
                let pair = (vehicle.0, o.0);
                let again = self.crashed.iter().any(|&(a, b, t)| (a, b) == pair && sim.state.tick < t + 20 * 60);
                self.crashed.retain(|&(_, _, t)| sim.state.tick < t + 20 * 60);
                self.crashed.push((pair.0, pair.1, sim.state.tick));
                let at = sim.state.entities.get(*vehicle).map(|e| e.pos).unwrap_or_default();
                let chase = hot.iter().map(|p| p.distance(at)).fold(f32::MAX, f32::min);
                if again {
                } else if calm(*vehicle) && calm(*o) && chase > 40.0 {
                    self.ai_crashes += 1;
                    if self.crash_sites.len() < 16 {
                        let what = |id: EntityId| {
                            let a = self.cars.iter().find(|a| a.car == id);
                            let c = Car::read(sim, id);
                            format!(
                                "{}#{} {:?} lane {} along {:.0} speed {:.1} held {} pass {:.1}",
                                sim.state.entities.get(id).map(|e| e.name.as_str()).unwrap_or("?"),
                                id.0,
                                a.map(|a| a.tier),
                                a.map(|a| a.lane).unwrap_or(0),
                                a.map(|a| a.along).unwrap_or(0.0),
                                c.map(|c| c.speed()).unwrap_or(0.0),
                                a.is_some_and(|a| a.held),
                                a.map(|a| a.passing).unwrap_or(0.0),
                            )
                        };
                        self.crash_sites.push(format!(
                            "tick {} at {:.0},{:.0} impulse {:.0} chase {:.0} m: {} / {}",
                            sim.state.tick,
                            at.x,
                            at.z,
                            impulse,
                            chase.min(9999.0),
                            what(*vehicle),
                            what(*o)
                        ));
                    }
                } else {
                    self.incident_crashes += 1;
                }
            }
            for id in [Some(*vehicle), *other].into_iter().flatten() {
                let loaded = sim.state.entities.get(id).is_some_and(|e| crate::world::ground_loaded(sim, e.pos));
                if let Some(a) = self.cars.iter_mut().find(|a| a.car == id) {
                    if a.tier == Tier::Rail && loaded {
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
            "stuck_incident": self.stuck_incident,
            "spawn_in_view": self.spawn_in_view,
            "despawn_in_view": self.despawn_in_view,
            "ai_crashes": self.ai_crashes,
            "incident_crashes": self.incident_crashes,
            "ai_crash_sites": self.crash_sites,
            "stuck_sites": self.stuck_sites,
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
            // Nobody climbs out of a wreck (and a car that is gone leaves no driver behind).
            let people = if sim.state.entities.get(car).is_some() {
                self.loose.push(car);
                occupants(sim, car)
            } else {
                driver.into_iter().filter(|&d| Some(d) != sim.state.player).collect()
            };
            for p in people {
                sim.despawn(p);
            }
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
        for a in self.cars.iter().filter(|a| !a.pilot && !a.mission) {
            let Some(e) = sim.state.entities.get(a.car) else { continue };
            let far = e.pos.distance(player) > d.density.despawn;
            let stuck = a.blocked > d.drive.give_up && !seen(&m.camera, e.pos);
            if stuck && self.stuck_sites.len() < 16 {
                let l = &g.lanes[a.lane as usize];
                self.stuck_sites.push(format!(
                    "tick {} {}#{} at {:.0},{:.0} lane {} ({:?} {:?}, len {:.0}) along {:.0} route {} {:?} {:?} goes_on {}",
                    sim.state.tick,
                    e.name,
                    a.car.0,
                    e.pos.x,
                    e.pos.z,
                    a.lane,
                    l.kind,
                    l.turn,
                    l.length,
                    a.along,
                    a.route.len(),
                    a.tier,
                    a.mood,
                    goes_on(g, a.lane)
                ));
            }
            if (far && !seen(&m.camera, e.pos)) || stuck {
                gone.push((a.car, a.driver, stuck));
            }
        }
        for (car, driver, stuck) in gone {
            self.cars.retain(|a| a.car != car);
            if stuck {
                // An incident nearby (a vehicle that is neither traffic nor parked: the police, a
                // wreck, an abandoned car), or the traffic's own doing.
                let at = sim.state.entities.get(car).map(|e| e.pos).unwrap_or(player);
                let incident = sim.state.entities.iter().any(|e| {
                    e.vehicle.is_some()
                        && e.id != car
                        && e.pos.distance(at) < 40.0
                        && !self.cars.iter().any(|a| a.car == e.id)
                        && !self.parked.values().any(|&p| p == e.id)
                });
                if incident {
                    self.stuck_incident += 1;
                } else {
                    self.stuck += 1;
                }
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
        // Loose cars go once far and out of view, with anyone left sitting in them (the player's
        // aside); an occupied one goes as soon as its ground unloads (the engine only puts empty
        // cars to sleep, so it would fall through).
        self.loose.retain(|&car| {
            let Some(e) = sim.state.entities.get(car) else { return false };
            let people = occupants(sim, car);
            let player_in = e
                .vehicle
                .as_ref()
                .is_some_and(|v| sim.state.player.is_some_and(|p| v.driver == Some(p) || v.passengers.contains(&Some(p))));
            if player_in || Some(car) == m.player_car || seen(&m.camera, e.pos) {
                return true;
            }
            let far = e.pos.distance(player) > d.density.despawn;
            if far || (!people.is_empty() && !crate::world::ground_loaded(sim, e.pos)) {
                for p in people {
                    sim.despawn(p);
                }
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
            // Not on top of something already there (the player's car, a loose car), and only on
            // loaded ground.
            if sim.state.entities.iter().any(|e| e.vehicle.is_some() && e.pos.distance(s.pos) < 3.0)
                || !crate::world::ground_loaded(sim, s.pos)
            {
                continue;
            }
            let kinds = ambient_kinds();
            let Some(key) = pick_kind(&kinds, &mut sim.state.rng) else { break };
            let paint = Some(sim.state.rng.below(8) as usize);
            if let Ok(car) = crate::vehicles::spawn(sim, &key, s.pos + Vec3::Y * 0.05, s.dir.x.atan2(s.dir.z), paint) {
                crate::world::ambient(sim, car);
                self.parked.insert(i, car);
                if hash(seed ^ 0x5eed, i as u64) < d.parked.locked {
                    locked.insert(car);
                }
            }
        }
        // Moving traffic: the hour's share of the cap.
        let want =
            (d.density.max as f32 * crate::peds::time_share(m.hour, d.density.night) * self.thin.unwrap_or(1.0)).round() as usize;
        let buses = self.cars.iter().filter(|a| a.bus.is_some()).count();
        let loop_ok = self.bus_loop.as_ref().is_some_and(|b| !b.lanes.is_empty());
        if loop_ok && buses < d.bus.count {
            self.spawn_bus(sim, m, d, first);
        }
        let tries = if first { 120 } else { 6 };
        let kinds = ambient_kinds();
        for _ in 0..tries {
            if self.cars.iter().filter(|a| !a.pilot && !a.mission).count() >= want + buses || kinds.is_empty() {
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
            crate::world::ambient(sim, car);
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
                passing: 0.0,
                goal: None,
                away: None,
                mission: false,
                arrived: false,
                planned: false,
                avoid: Vec::new(),
                jam: 0.0,
                why: "",
                end: None,
                plan_wait: 0.0,
            };
            self.extend(&mut agent, g, &mut sim.state.rng);
            if pos.distance(player) > d.density.full || !crate::world::ground_loaded(sim, pos) {
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
            crate::world::ambient(sim, car);
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
                passing: 0.0,
                goal: None,
                away: None,
                mission: false,
                arrived: false,
                planned: false,
                avoid: Vec::new(),
                jam: 0.0,
                why: "",
                end: None,
                plan_wait: 0.0,
            };
            if dist > d.density.full || !crate::world::ground_loaded(sim, pos) {
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
        // A mission's destination: the lanes there, then nothing more (it stops at the end) —
        // laid again whenever the route runs out short of the goal's lane (a lane change to
        // pass or pull over clears it).
        if let Some(goal) = a.goal {
            if !a.planned || (a.route.is_empty() && a.end != Some(a.lane) && a.plan_wait <= 0.0) {
                a.planned = true;
                a.end = None;
                let start = *a.route.last().unwrap_or(&a.lane);
                // The lanes nearest the goal, open roads at its level first, then any lane there
                // (the Ditch's bed is closed to ambient traffic but somewhere to go), then the
                // nearest at any level: in turn, until one can be reached (the nearest may run the
                // other way, as the far side of a road or the Ditch's other bed lane).
                let open = |l: &pav_world::lanes::Lane| !l.closed && drivable(l.kind) && l.kind != LaneKind::Connector;
                let any = |l: &pav_world::lanes::Lane| l.kind != LaneKind::Connector;
                let mut tried: Vec<u32> = Vec::new();
                'search: for pass in 0..4 {
                    for _ in 0..3 {
                        let fresh = |l: &pav_world::lanes::Lane| !tried.iter().any(|&t| std::ptr::eq(l, &g.lanes[t as usize]));
                        let found = match pass {
                            0 => g.nearest_level(goal, 4.0, |l| open(l) && fresh(l)),
                            1 => g.nearest_level(goal, 4.0, |l| any(l) && fresh(l)),
                            2 => g.nearest(goal, |l| open(l) && fresh(l)),
                            _ => g.nearest(goal, |l| any(l) && fresh(l)),
                        };
                        let Some((end, _, _)) = found else { break };
                        if end == start {
                            a.end = Some(end);
                            break 'search;
                        }
                        // Round the lanes found blocked, when there is another way.
                        let mut path = g.route_lanes_avoiding(start, end, &a.avoid);
                        if path.is_empty() {
                            path = g.route_lanes(start, end);
                        }
                        if path.len() > 1 && path.first() == Some(&start) {
                            a.route.extend_from_slice(&path[1..]);
                            a.end = Some(end);
                            break 'search;
                        }
                        tried.push(end);
                    }
                }
                if a.end.is_none() {
                    a.plan_wait = 2.0;
                }
            }
            return;
        }
        // Running: from the chasers (the player's car on the run), or from what a mission says.
        let from = if a.pilot { self.escape } else { a.away };
        while a.route.len() < 4 {
            let last = *a.route.last().unwrap_or(&a.lane);
            let next = match (a.pilot || a.away.is_some(), from) {
                // Running: the turn whose road leads farthest from the chasers.
                (true, Some(from)) => g.lanes[last as usize]
                    .next
                    .iter()
                    .copied()
                    .filter(|&n| {
                        let l = &g.lanes[n as usize];
                        !l.closed && drivable(l.kind) && l.turn != Turn::UTurn && !dead_end(g, n, 8)
                    })
                    .max_by(|&x, &y| {
                        let reach = |n: u32| {
                            // Two lanes on: where this choice leads.
                            let l = &g.lanes[n as usize];
                            let after = l
                                .next
                                .first()
                                .map(|&m| *g.lanes[m as usize].points.last().unwrap())
                                .unwrap_or(*l.points.last().unwrap());
                            after.distance(from)
                        };
                        reach(x).total_cmp(&reach(y)).then(y.cmp(&x))
                    })
                    .or_else(|| pick_next(g, last, rng)),
                // The player's car on autopilot keeps to roads that go somewhere.
                (true, None) => {
                    let mut tries = 0;
                    loop {
                        let n = pick_next(g, last, rng);
                        tries += 1;
                        if n.is_none_or(|n| !dead_end(g, n, 8)) || tries >= 6 {
                            break n;
                        }
                    }
                }
                _ => pick_next(g, last, rng),
            };
            match next {
                Some(n) => a.route.push(n),
                None => break,
            }
        }
    }

    /// Every car's driving this tick.
    fn drive(&mut self, sim: &mut Sim, m: &Moment, d: &TrafficDef, out: &mut BTreeMap<EntityId, VehicleInput>) {
        self.honks.clear();
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
            cars.insert(e.id, c);
        }
        // Cars on sibling lanes where they overlap this one: just into a turn that fans out of the
        // same approach (they are still in front, whichever way they go), and closing on a merge
        // (placed by their distance to it, so whoever is nearer goes first).
        if self.siblings.as_ref().is_none_or(|s| s.lanes != g.lanes.len()) {
            self.siblings = Some(Arc::new(Siblings::of(g)));
        }
        let sibs = self.siblings.clone().unwrap_or_default();
        let mut seen_from: Vec<(u32, (f32, f32, f32, EntityId))> = Vec::new();
        for (&l, others) in &sibs.start {
            for o in others {
                for &(al, sp, half, id) in occ.get(o).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if al < 8.0 {
                        seen_from.push((l, (al, sp, half, id)));
                    }
                }
            }
        }
        for (&l, others) in &sibs.end {
            let len = g.lanes[l as usize].length;
            for o in others {
                let olen = g.lanes[*o as usize].length;
                for &(al, sp, half, id) in occ.get(o).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if olen - al < 20.0 {
                        seen_from.push((l, (len - (olen - al), sp, half, id)));
                    }
                }
            }
        }
        for (l, e) in seen_from {
            let list = occ.entry(l).or_default();
            if !list.iter().any(|x| x.3 == e.3) {
                list.push(e);
            }
        }
        for list in occ.values_mut() {
            list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.3.cmp(&b.3)));
        }
        // Parked cars too, for drivers on the run to keep clear of.
        if self.cars.iter().any(|a| a.mood == Mood::Flee) {
            for &id in &parked {
                if let Some(c) = Car::read(sim, id).filter(|c| c.center.distance(m.player) < 200.0) {
                    cars.insert(id, c);
                }
            }
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
        // Unsignalised junctions: who reaches the line first goes first (in half-second steps, then
        // by id), so two cars from different sides never both pull out into an empty box.
        let mut arriving: BTreeMap<u32, Vec<(u8, (i32, EntityId))>> = BTreeMap::new();
        for a in &self.cars {
            let l = &g.lanes[a.lane as usize];
            if let (LaneKind::Street | LaneKind::Avenue | LaneKind::Boulevard, Some(j), Some(side)) =
                (l.kind, l.junction, l.approach)
                && g.junctions[j as usize].signal.is_none()
            {
                let to_line = l.length - l.stop - a.along;
                if (-3.0..15.0).contains(&to_line) {
                    arriving.entry(j).or_default().push((side, (first_come(to_line, a.speed), a.car)));
                }
            }
        }
        // Waiting at a light or behind someone who is: shared one tick late down the queue.
        let held_before: BTreeSet<EntityId> = self.cars.iter().filter(|a| a.held).map(|a| a.car).collect();
        let sirens = m.sirens.clone();
        let gunfire = m.gunfire;
        let near_full = d.density.full;
        let back_rail = d.density.rail;
        // Cars on rails are moved without the physics (they would shove anything in their way, or
        // land on it changing lanes): near a car the physics moves, they are back on their wheels.
        let railed: BTreeSet<EntityId> = self.cars.iter().filter(|a| a.tier == Tier::Rail).map(|a| a.car).collect();
        let free: Vec<Vec3> =
            cars.values().filter(|o| !railed.contains(&o.id) && !parked.contains(&o.id)).map(|o| o.center).collect();
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
            let company = |r: f32| free.iter().any(|p| *p != c.center && p.distance(c.center) < r);
            match a.tier {
                _ if a.pilot => {}
                // Back on its wheels only on loaded ground and clear of houses and other cars (a car
                // on rails passes through anything; the physics would throw it out).
                Tier::Rail
                    if (near < near_full || company(15.0))
                        && crate::world::ground_loaded(sim, c.center)
                        && !m.city.in_building(Vec2::new(c.center.x, c.center.z), 1.5)
                        && !cars
                            .values()
                            .any(|o| o.id != a.car && o.center.distance(c.center) < c.half.z + o.half.z.min(3.0)) =>
                {
                    sim.set_vehicle_kinematic(a.car, false);
                    a.tier = Tier::Full;
                }
                // Shaken cars stay on their wheels while near; unloaded ground puts anyone on rails.
                Tier::Full
                    if (near > back_rail && a.mood != Mood::Shaken && !company(25.0))
                        || !crate::world::ground_loaded(sim, c.center) =>
                {
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
                    if let Some((lane, along, off2)) = g.nearest_level(c.feet(), 4.0, keep)
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
            // A lane that goes nowhere: over toward one that does, into a gap.
            if a.route.is_empty()
                && a.change_t <= 0.0
                && !goes_on(g, a.lane)
                && let Some(side) = way_on(g, a.lane)
            {
                let s = &g.lanes[side as usize];
                let at = s.project(c.feet()).0;
                if gap_beside(occ.get(&side), a.car, at, a.speed) {
                    a.lane = side;
                    a.along = at;
                    self.extend(&mut a, g, &mut rng);
                    a.change_t = 2.0;
                }
            }
            let lane = &g.lanes[a.lane as usize];
            // Moods (the autopilot on the run drives like a fleeing driver, as does a mission's
            // runner; wanted, it does too unless told to take it easy: desire under 0.9).
            if a.pilot {
                let run = self.escape.is_some() || a.away.is_some() || (m.wanted && a.desire >= 0.9);
                a.mood = if run { Mood::Flee } else { Mood::Cruise };
                a.mood_t = 1.0;
            } else if a.away.is_some() {
                a.mood = Mood::Flee;
                a.mood_t = 1.0;
            }
            a.mood_t -= dt;
            a.plan_wait -= dt;
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
            // (The turns ahead alone: a driver in a hurry still takes them at a safe speed.)
            let mut curve = if lane.turn == Turn::Straight || lane.turn == Turn::None { f32::MAX } else { limit };
            {
                // Far enough ahead to brake in comfort from the speed it goes.
                let horizon = (a.speed * a.speed / 4.0).clamp(40.0, 240.0);
                let mut dist = lane.length - a.along;
                let mut prev = a.lane;
                for &n in a.route.iter().take(10) {
                    if dist > horizon {
                        break;
                    }
                    // Slow down in time for a turn ahead (comfortable braking over the distance),
                    // and for a sharp corner where two lanes meet (a ramp off a street's end).
                    let next = &g.lanes[n as usize];
                    let corner = joint_speed(&g.lanes[prev as usize], next);
                    let v = turn_speed(next).min(corner);
                    let brake = (v * v + 2.0 * 2.0 * dist).sqrt();
                    limit = limit.min(brake);
                    if (next.turn != Turn::Straight && next.turn != Turn::None) || corner < next.speed {
                        curve = curve.min(brake);
                    }
                    dist += next.length;
                    prev = n;
                }
            }
            // Fleeing drivers push on; the autopilot on the run drives like a getaway driver.
            let haste = match (a.mood, a.pilot) {
                (Mood::Flee, true) => 1.6,
                (Mood::Flee, false) => 1.25,
                _ => 1.0,
            };
            let mut v0 = limit * a.desire * haste;
            // Under a mission's orders (a race, a drive somewhere), hurrying never takes a turn
            // much faster than it can be taken.
            if a.goal.is_some() {
                v0 = v0.min(curve * 1.15);
            }
            // Obstacles: the nearest car ahead, a light, a junction box, people on the road.
            let mut obstacle: Option<(f32, f32)> = None;
            let mut held = false;
            // (`car`: the nearest obstacle is another vehicle, on that lane — a fleeing driver goes
            // round it.)
            let mut car_ahead = false;
            let mut car_lane: Option<u32> = None;
            let mut why = "";
            let mut consider = |gap: f32, speed: f32, excuse: bool, car: Option<u32>, what: &'static str| {
                if obstacle.is_none_or(|o| gap < o.0) {
                    obstacle = Some((gap, speed));
                    held = excuse;
                    car_ahead = car.is_some();
                    car_lane = car;
                    why = what;
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
                        // The autopilot on the run, boxed in a while, shoves a stopped car aside
                        // (a roadblock, a car pulled over in its way) rather than wait behind it.
                        if a.pilot && a.mood == Mood::Flee && a.jam > 2.0 && speed.abs() < 1.0 {
                            continue;
                        }
                        let gap = offset + along - my_half - half;
                        // Level with it (a merge): the lower id goes first.
                        let in_front = offset + along > 0.1 || ((offset + along).abs() <= 0.1 && id < a.car);
                        if in_front && gap < look {
                            consider(gap.max(0.0), speed, held_before.contains(&id), Some(lane_id), "car");
                        }
                    }
                }
                // A light at this lane's end.
                let to_line = offset + ln.length - ln.stop - my_half;
                // A line just behind the bumper still counts for a car crawling over it.
                let before = to_line > -0.5 || (to_line > -3.0 && a.speed < 3.0);
                if ln.kind != LaneKind::Connector && before && to_line < look && a.mood != Mood::Flee {
                    match g.light(lane_id, m.time) {
                        Some(Light::Red) => consider(to_line.max(0.0), 0.0, true, None, "red"),
                        Some(Light::Amber) if to_line > a.speed * a.speed / (2.0 * d.drive.decel) * 0.8 => {
                            consider(to_line.max(0.0), 0.0, true, None, "amber")
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
                        let mine = (first_come(to_line, a.speed), a.car);
                        let after =
                            arriving.get(&j).is_some_and(|v| v.iter().any(|&(s, o)| s != side && o.1 != a.car && o < mine));
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
                        // With lights, green still waits for a straggler from a crossing side.
                        let straggler = signalled
                            && boxes
                                .get(&j)
                                .is_some_and(|v| v.iter().any(|(s, id)| *s != side && *s != opposite && *id != a.car));
                        if (!signalled && (busy || after)) || oncoming || straggler {
                            consider(to_line.max(0.0), 0.0, true, None, "junction");
                        }
                    }
                }
                offset += ln.length;
                if offset > look || k >= a.route.len() {
                    // The route runs out where the lane goes nowhere: stop at its end (and wait to
                    // move over) rather than drive on into what is closed.
                    if k >= a.route.len() && offset <= look && !goes_on(g, lane_id) {
                        consider((offset - my_half - 1.0).max(0.0), 0.0, false, None, "lane end");
                    }
                    break;
                }
                lane_id = a.route[k];
                k += 1;
            }
            // A mission's destination on this lane (the route used up): stop there.
            if let Some(goal) = a.goal
                && a.planned
                && a.route.is_empty()
            {
                let (gal, _) = lane.project(goal);
                let gap = gal - a.along - my_half * 0.5;
                consider(gap.max(0.0), 0.0, true, None, "goal");
                if a.speed.abs() < 0.8 && (gap < 4.0 || c.feet().distance(goal) < 9.0) {
                    a.arrived = true;
                }
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
                            // The autopilot on the run does not stop for the officers in its way.
                            if a.pilot && a.mood == Mood::Flee && m.officers.iter().any(|o| o.distance(*p) < 0.05) {
                                continue;
                            }
                            if Vec2::new(p.x - pt.x, p.z - pt.z).length() < reach && (p.y - pt.y).abs() < 2.5 {
                                if a.mood == Mood::Flee && s > c.half.z + 6.0 {
                                    // A fleeing driver leans on the horn instead of stopping.
                                    self.honks.push(pt);
                                } else {
                                    // (Close in: it stops, and still leans on the horn — they
                                    // step out of the way.)
                                    if a.mood == Mood::Flee {
                                        self.honks.push(pt);
                                    }
                                    consider((s - c.half.z - 2.0).max(0.0), 0.0, true, None, "person");
                                }
                                break 'path;
                            }
                        }
                    }
                }
            }
            // Anything about to cross the car's path — nearest approach within 2.5 s closer than
            // the two bodies — is braked for: a car stranded in a junction, one turning across,
            // and (on the run, through red lights and round cars) everything.
            {
                let fwd = c.forward();
                let flat = |v: Vec3| Vec2::new(v.x, v.z);
                // On rails the body's velocity is not the car's: its speed along the lane is.
                let mine = if a.tier == Tier::Rail { fwd * a.speed } else { c.vel };
                for (&id, o) in cars.range(..) {
                    if id == a.car || o.center.distance(c.center) > 45.0 || (o.center.y - c.center.y).abs() > 3.0 {
                        continue;
                    }
                    if a.pilot && a.mood == Mood::Flee && m.police.contains(&id) {
                        continue;
                    }
                    let p = flat(o.center - c.center);
                    let v = flat(o.vel - mine);
                    let t = (-p.dot(v) / v.length_squared().max(1e-3)).clamp(0.0, 2.5);
                    if t < 0.1 {
                        continue;
                    }
                    let crossing = o.forward().dot(fwd).abs() < 0.6;
                    let reach = c.half.x + if crossing { o.half.z * 0.9 } else { o.half.x } + 0.6;
                    let meet = p + v * t;
                    let ahead_of_me = meet.dot(flat(fwd)) > c.half.z * 0.5;
                    if meet.length() < reach && ahead_of_me {
                        // Where the other will be, along this car's way.
                        let there = p + flat(o.vel) * t;
                        let gap = there.dot(flat(fwd)) - c.half.z - if crossing { o.half.x } else { o.half.z };
                        // (Not a car to pass: it is still in the way.)
                        consider(gap.max(0.0), o.vel.dot(fwd).max(0.0), false, None, "crossing");
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
                    consider(0.0, 0.0, true, None, "bus stop");
                    if bs.dwell <= 0.0 {
                        bs.stop = (bs.stop + 1) % bl.stops.len();
                    }
                } else if (0.0..30.0).contains(&z) && to.dot(c.left()).abs() < 6.0 {
                    // Its doors (the middle) come to rest by the stop.
                    if z < 8.0 && a.speed.abs() < 0.8 {
                        bs.dwell = d.bus.dwell;
                    }
                    consider(z.max(0.0), 0.0, true, None, "bus stop");
                } else if z < -3.0 && to.length() < 12.0 {
                    // Went past it (pushed, rerouted): on to the next.
                    bs.stop = (bs.stop + 1) % bl.stops.len();
                }
            }
            if a.mood == Mood::PullOver || a.mood == Mood::Shaken {
                v0 = 0.0;
                held = true;
            }
            // Pass a slow car on a multi-lane road (on the run, round a car stopped for anything,
            // and with less road left).
            let hurry = a.mood == Mood::Flee && car_ahead;
            let room = if hurry { 12.0 } else { 30.0 };
            if a.tier == Tier::Full
                && (a.mood == Mood::Cruise || hurry)
                && a.bus.is_none()
                && a.change_t <= 0.0
                && lane.length - a.along > room
                && obstacle.is_some_and(|(gap, sp)| gap < 25.0 && sp < 0.7 * v0)
                && (!held || hurry)
            {
                // (Not into a lane that ends short of where this one goes on.)
                for side in [lane.left, lane.right].into_iter().flatten().filter(|&s| goes_on(g, s) || !goes_on(g, a.lane)) {
                    let s = &g.lanes[side as usize];
                    let at = s.project(c.feet()).0;
                    if gap_beside(occ.get(&side), a.car, at, a.speed) && s.length - at > room {
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
            // Fleeing: out past a slower car ahead rather than queueing behind it.
            a.passing -= dt;
            if a.mood == Mood::Flee
                && a.tier == Tier::Full
                && car_ahead
                && obstacle.is_some_and(|(gap, sp)| gap < 22.0 && sp < a.speed.max(4.0))
            {
                // Only when the oncoming lane beside it is clear for a good way ahead.
                let fwd = c.forward();
                let beside = c.feet() + c.left() * lane.width;
                let keep = |l: &pav_world::lanes::Lane| !l.closed && drivable(l.kind);
                let clear = match g.nearest(beside, keep) {
                    Some((ol, oal, off)) if off < 2.5 => {
                        let opposite = g.lanes[ol as usize].at(oal).1.dot(fwd) < -0.5;
                        let busy = occ.get(&ol).is_some_and(|v| {
                            v.iter().any(|(al, _, _, id)| {
                                *id != a.car
                                    && if opposite { *al < oal + 6.0 && *al > oal - 40.0 } else { (*al - oal).abs() < 25.0 }
                            })
                        });
                        !busy
                    }
                    _ => false,
                };
                if clear {
                    a.passing = 2.5;
                }
            }
            if a.passing > 0.0 && car_ahead {
                obstacle = None;
                held = false;
                // (Not flat out on the wrong side of the road, by the cars parked along it.)
                v0 = v0.min(18.0);
            }
            // On the run to a goal and sat behind a stopped car (a police car across the road):
            // another way there, round that lane.
            if a.goal.is_some() && a.mood == Mood::Flee && (car_ahead || a.jam > 2.0) && a.speed.abs() < 2.0 {
                a.jam += dt;
            } else {
                a.jam = 0.0;
            }
            if a.jam > 6.0 {
                a.jam = 0.0;
                if let Some(l) = car_lane
                    && !a.avoid.contains(&l)
                {
                    a.avoid.push(l);
                    if a.avoid.len() > 6 {
                        a.avoid.remove(0);
                    }
                    a.route.clear();
                    a.planned = false;
                }
            }
            a.why = if obstacle.is_some() { why } else { "" };
            let accel = idm(a.speed.max(0.0), v0, obstacle, &d.drive);
            // Stuck: stopped with nothing that excuses it (on the run, crawling counts: pushing
            // against a car it is wedged against).
            let crawl = if a.mood == Mood::Flee { 1.5 } else { 0.5 };
            let stopped = a.speed.abs() < crawl && v0 > 0.5;
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
            // Turned across its lane (spun, wedged) it backs up sooner: a three-point turn.
            let askew = g.lanes[a.lane as usize].at(a.along).1.dot(c.forward()) < 0.6;
            // A fleeing driver does not sit and wait.
            let wait = if askew || a.mood == Mood::Flee { 1.5 } else { d.drive.reverse_after };
            // (Queued behind another car a calm driver just waits; backing up is for being stuck.)
            if a.blocked > wait && a.reverse <= 0.0 && a.tier == Tier::Full && (askew || !car_ahead || a.mood == Mood::Flee) {
                // Back up only into clear road (nothing within 9 m behind on this lane, nothing
                // at all right behind the bumper); on the run, whatever is there gets shoved.
                let clear = occ.get(&a.lane).is_none_or(|v| {
                    v.iter().all(|(al, _, half, id)| *id == a.car || *al > a.along || a.along - al - half - c.half.z > 9.0)
                });
                let room = cars.values().all(|o| {
                    let d = o.center - c.center;
                    let back = -d.dot(c.forward());
                    o.id == a.car
                        || back <= 0.0
                        || back > c.half.z + o.half.z + 3.5
                        || d.dot(c.left()).abs() > c.half.x + o.half.x + 0.6
                });
                let hurry = askew || a.mood == Mood::Flee;
                if (clear || hurry) && (room || a.mood == Mood::Flee) {
                    a.reverse = if hurry { 2.2 } else { 1.6 };
                }
                a.blocked = if hurry { 0.0 } else { d.drive.honk_after };
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
                        // Gently (the brake pedal reverses), no faster than 2.5 m/s; on the run,
                        // hard (shoving what is behind).
                        let (pedal, top) = if a.mood == Mood::Flee { (0.9, -5.0) } else { (0.35, -2.5) };
                        let pedal = if c.speed() > top { pedal } else { 0.0 };
                        VehicleInput { brake: pedal, steer: if side > 0.0 { 1.0 } else { -1.0 }, horn, ..Default::default() }
                    } else {
                        let pull = if a.mood == Mood::PullOver {
                            lane.width * 0.45
                        } else if a.passing > 0.0 {
                            // Over the centre line to pass.
                            -lane.width * 0.9
                        } else {
                            0.0
                        };
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
/// How fast the corner where lane `a` runs into lane `b` can be taken: the angle between them
/// turned over the gap between their ends (or a few metres), at a comfortable sideways pull.
fn joint_speed(a: &pav_world::lanes::Lane, b: &pav_world::lanes::Lane) -> f32 {
    let (Some(&a1), Some(&b0)) = (a.points.last(), b.points.first()) else { return f32::MAX };
    let (Some(&a0), Some(&b1)) = (a.points.get(a.points.len().wrapping_sub(2)), b.points.get(1)) else { return f32::MAX };
    let flat = |v: Vec3| Vec2::new(v.x, v.z).normalize_or_zero();
    let angle = flat(a1 - a0).dot(flat(b1 - b0)).clamp(-1.0, 1.0).acos();
    if angle < 25f32.to_radians() {
        return f32::MAX;
    }
    let radius = Vec2::new(b0.x - a1.x, b0.z - a1.z).length().max(3.0) / angle;
    (3.5 * radius).sqrt().clamp(3.0, b.speed)
}

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
    // Look further ahead at speed (a short look swings a fast car across its lane).
    let look = (3.5 + v.max(0.0) * 0.5 + (v - 10.0).max(0.0) * 0.6).clamp(4.0, 28.0);
    let (mut target, dir) = ahead(g, a.lane, a.along, &a.route, look);
    // `pull` metres off the lane's line: to its right (the curb), or negative to its left (out
    // past a car).
    target += Vec3::new(-dir.z, 0.0, dir.x) * pull;
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
    // Plus a pull back onto the lane's centre line at turning speeds (pure pursuit alone cuts
    // corners); none at cruising speed, where it would swing the car across the road.
    let (p, d) = g.lanes[a.lane as usize].at(a.along);
    let off = (c.feet() - p).dot(Vec3::new(d.z, 0.0, -d.x)) + pull;
    let calm = ((12.0 - v) / 6.0).clamp(0.0, 1.0);
    let steer = (-angle / (lock * share) + (off * 0.3 * calm).clamp(-0.35 * calm, 0.35 * calm)).clamp(-1.0, 1.0);
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
