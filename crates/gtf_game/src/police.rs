//! The police (PLAN.md §5.6.5): units the wanted level dispatches, never spawned in view or
//! within 80 m — patrol cars (two officers each), tactical vans at 4★, the SPPD Air helicopter
//! at 3★ with its searchlight at night. Cars respond along the lane graph to the last known
//! position with sirens on, pursue a seen player (leading the chase; ramming from 3★), stop when
//! the player is on foot and let their officers out; officers close in to arrest at low levels
//! and shoot from range otherwise, and run back to the car if the player drives off. While the
//! stars flash they search the zone, their vision cones on the radar; when the heat clears they
//! drive away. Everyone's sight of the player decides the heat's "seen".

use std::collections::BTreeMap;

use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::present::{Blip, BlipIcon, CameraIntent, ConeBlip, DrawList, SpotDraw};
use pav_core::sim::Sim;
use pav_core::vehicle::VehicleInput;
use pav_core::{Color, input::buttons};
use pav_world::City;
use pav_world::lanes::LaneKind;
use serde::{Deserialize, Serialize};

use crate::drive::Car;
use crate::heat::{POLICE_SIGHT, sight};

/// Units by level (0–5 stars): patrol cars, tactical vans, helicopters.
pub const CARS: [usize; 6] = [0, 1, 2, 4, 6, 8];
pub const VANS: [usize; 6] = [0, 0, 0, 0, 1, 2];
pub const AIR: [usize; 6] = [0, 0, 0, 1, 1, 2];
/// Never spawned nearer than this, and within this.
const SPAWN_NEAR: f32 = 110.0;
const SPAWN_FAR: f32 = 230.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Car,
    Van,
    Air,
}

/// What a unit is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Driving to the last known position, sirens on.
    Respond,
    /// Chasing a player it can see.
    Pursue,
    /// Stopped by a player on foot; the crew is out.
    Halt,
    /// Driving round the search zone.
    Search,
    /// The heat is over: away, and gone out of view.
    Leave,
}

/// A police vehicle and its crew.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Unit {
    pub kind: Kind,
    pub vehicle: EntityId,
    pub crew: Vec<EntityId>,
    pub mode: Mode,
    /// The way to `goal` (points along lanes), and seconds to the next replan.
    pub path: Vec<Vec3>,
    pub replan: f32,
    pub goal: Vec3,
    /// Seconds barely moving while trying to; backing up.
    pub stuck: f32,
    pub reverse: f32,
    /// A helicopter's place on its orbit and where its searchlight points (smoothed).
    pub orbit: f32,
    pub light: Vec3,
    /// Seconds since this unit last saw the player.
    pub unseen: f32,
    /// Moved along its path kinematically while far away (the ground there may not be loaded).
    #[serde(default)]
    pub rails: bool,
    /// Seconds before it may try another PIT (it backs off after one).
    #[serde(default)]
    pub ram_t: f32,
}

/// Units farther than this from the player drive on rails, nearer on their wheels (m).
const RAILS_FAR: f32 = 100.0;
const RAILS_NEAR: f32 = 92.0;

/// The points of a lane route from where a vehicle is: the ones behind it dropped (a route
/// starts at the beginning of the nearest lane, which may be behind).
fn route_from(city: &City, from: Vec3, to: Vec3) -> Vec<Vec3> {
    let lanes = city.lanes.route(from, to);
    let mut pts = city.lanes.route_points(&lanes);
    if let Some((i, _)) = pts.iter().enumerate().min_by(|a, b| a.1.distance(from).total_cmp(&b.1.distance(from))) {
        pts.drain(..i.min(pts.len().saturating_sub(1)));
        // The nearest point itself, if it lies behind.
        if pts.len() > 1 && (pts[1] - pts[0]).dot(pts[0] - from) < 0.0 {
            pts.remove(0);
        }
    }
    // Off the road (the player on foot in a yard) the rails stop at the kerb: they never cut
    // across lots and through houses.
    if on_road(city, to) {
        pts.push(to);
    }
    pts
}

/// Whether `p` is on a road cars drive (a street, avenue, boulevard or alley).
fn on_road(city: &City, p: Vec3) -> bool {
    use pav_world::def::RoadClass;
    city.road_at(Vec2::new(p.x, p.z)).is_some_and(|r| !matches!(r, RoadClass::Path | RoadClass::Channel))
}

/// Whether something fixed stands between `from` and `to` (a roadblock out of sight).
fn sight_blocked(sim: &Sim, from: Vec3, to: Vec3) -> bool {
    !sight(sim, from, to)
}

/// Whether the ground at `p` is loaded (its streaming chunk is active).
fn ground_loaded(sim: &Sim, p: Vec3) -> bool {
    crate::world::ground_loaded(sim, p)
}

/// What an officer on foot is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Duty {
    Seated,
    /// Closing in (to arrest or to a firing position).
    Chase,
    Shoot,
    /// Back to the car (the player drove off).
    Back,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Officer {
    pub id: EntityId,
    /// The unit's vehicle and the seat they ride in.
    pub vehicle: EntityId,
    pub seat: usize,
    pub duty: Duty,
    /// Seconds to the next shot.
    pub cooldown: f32,
}

/// The police around the player (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Police {
    pub units: Vec<Unit>,
    pub officers: Vec<Officer>,
    pub spawned: u32,
    pub despawned: u32,
    /// Soak counter: units that appeared where the camera could see.
    pub spawn_in_view: u32,
    pub shots: u32,
    pub hits: u32,
    /// Seconds to the next dispatch check.
    pub pass: f32,
    /// Roadblocks across the player's way (3 stars and up), spike strips before them (4+), and
    /// seconds until the next may be set.
    pub blocks: Vec<Roadblock>,
    pub spikes: Vec<Spike>,
    pub block_t: f32,
    /// Police vehicles the player harmed lately (shot, blew up near, rammed) and seconds left:
    /// one of these blowing up is on the player; any other wreck is the police's own doing.
    pub blame: Vec<(EntityId, f32)>,
    /// Seconds the player has been all but still (in a car stopped this long, units pull up and
    /// their crews get out).
    pub still: f32,
    /// Wrecked units that left the roster this tick, for the game to clear away (with anyone
    /// still sitting in them) like any other wreck.
    pub wrecks: Vec<EntityId>,
}

/// How far from straight down a helicopter crew sees (radians).
const AIR_VIEW: f32 = 55.0 * std::f32::consts::PI / 180.0;

/// A car stopped this long (s) brings the crews out.
const STILL_HALT: f32 = 2.0;

/// How long harming a police vehicle stays on the player (s).
const BLAME: f32 = 15.0;

/// Cars parked across a road, their crews out beside them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Roadblock {
    pub at: Vec3,
    pub cars: Vec<EntityId>,
}

/// A spike strip across a lane: tyres that roll over it burst.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Spike {
    pub at: Vec3,
    /// Along the road (the strip lies across it), and its half length.
    pub dir: Vec3,
    pub half: f32,
}

/// Seconds between roadblocks, and how far ahead of the player they are set (m).
const BLOCK_EVERY: f32 = 25.0;
const BLOCK_AHEAD: [f32; 2] = [110.0, 200.0];

/// What the police need to know this tick.
pub struct Moment<'a> {
    pub city: &'a City,
    pub camera: CameraIntent,
    /// The player's feet and velocity, the vehicle the player is in.
    pub player: Vec3,
    pub player_vel: Vec3,
    pub player_car: Option<EntityId>,
    pub stars: u8,
    pub searching: bool,
    pub zone: Option<(Vec3, f32)>,
    pub last_known: Vec3,
    pub dispatch: bool,
    pub night: bool,
    /// Crouched, or somewhere that halves sight (under the freeway, an alley).
    pub hidden: bool,
    /// In cover and not peeking (shots mostly hit the wall).
    pub covered: bool,
    /// The player fired recently (officers at 1★ shoot back).
    pub firing: bool,
    pub dt: f32,
}

/// What the police did this tick.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// An officer, a car or the helicopter sees the player.
    pub sees: bool,
    /// Damage the officers' hits did to the player.
    pub hits: Vec<f32>,
    /// An officer within arm's reach of the player (an arrest at low levels).
    pub cuffing: bool,
    /// Shots: from, to (tracers), and whether it hit.
    pub shots: Vec<(Vec3, Vec3, bool)>,
}

/// Wraps an angle into -π..π.
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// An officer's look: the SPPD's navy uniform (peds.toml `[police]`: a cap, a badge, maybe
/// sunglasses). The skin comes from the sim's rng, the rest from a fork of it.
pub fn uniform(sim: &mut Sim) -> pav_core::puppet::PuppetDef {
    let peds = crate::peds::def().ok();
    let body = peds.as_ref().map(|d| d.body.clone()).unwrap_or_default();
    let mut p = body.biped();
    match peds.as_ref().and_then(|d| d.police.as_ref()) {
        Some(o) => {
            p.skin = o.skin[sim.state.rng.below(o.skin.len() as u32) as usize].clone();
            p.shirt = o.shirt.clone();
            p.pants = o.pants.clone();
            p.shoes = o.shoes.clone();
            let mut r = sim.state.rng.fork(0x0ff1_ce55);
            let (height, build) = (r.range(o.height[0], o.height[1]), r.range(o.build[0], o.build[1]));
            crate::peds::shape(&mut p, &body, height, build);
            o.wear.dress(&mut p, &sim.state.rng);
        }
        None => {
            let skins = ["#f1c9a5", "#e0ac83", "#c68a62", "#8d5a3b", "#6b4428"];
            p.skin = skins[sim.state.rng.below(skins.len() as u32) as usize].into();
            p.shirt = "#1d2b4f".into();
            p.pants = "#141a2b".into();
            p.shoes = "#0d0d0f".into();
        }
    }
    p.idle_clip = "MESH2MOTION/Idle_A".into();
    // The service pistol.
    p.weapon.asset = crate::combat::asset_of("p9");
    if !p.weapon.asset.is_empty() {
        p.weapon.kind = pav_core::puppet::WeaponKind::Dagger;
    }
    p
}

impl Police {
    /// Whether `id` is a police officer.
    pub fn is_officer(&self, id: EntityId) -> bool {
        self.officers.iter().any(|o| o.id == id)
    }

    /// Whether `id` is a police vehicle (a unit's, or parked across a road).
    pub fn is_unit(&self, id: EntityId) -> bool {
        self.units.iter().any(|u| u.vehicle == id) || self.blocks.iter().any(|b| b.cars.contains(&id))
    }

    /// The player harmed vehicle `id`: if it is the police's, its wreck is on the player for a
    /// while.
    pub fn harmed(&mut self, id: EntityId) {
        if !self.is_unit(id) {
            return;
        }
        match self.blame.iter_mut().find(|b| b.0 == id) {
            Some(b) => b.1 = BLAME,
            None => self.blame.push((id, BLAME)),
        }
    }

    /// Whether the player is to blame for what happens to police vehicle `id`.
    pub fn blamed(&self, id: EntityId) -> bool {
        self.blame.iter().any(|b| b.0 == id)
    }

    /// The heat is over: every unit drives away.
    pub fn stand_down(&mut self) {
        for u in &mut self.units {
            u.mode = Mode::Leave;
        }
    }

    /// Everything gone at once (the player respawned after Wasted or Busted).
    pub fn clear(&mut self, sim: &mut Sim) {
        for o in self.officers.drain(..) {
            sim.despawn(o.id);
        }
        for u in self.units.drain(..) {
            sim.despawn(u.vehicle);
        }
        for b in self.blocks.drain(..) {
            for c in b.cars {
                sim.despawn(c);
            }
        }
        self.spikes.clear();
        self.blame.clear();
    }

    /// One tick: dispatch by level, drive the units, move and fire the officers. Vehicle inputs
    /// go into `vin`, officers' moves into `moves`.
    pub fn tick(
        &mut self,
        sim: &mut Sim,
        m: &Moment,
        vin: &mut BTreeMap<EntityId, VehicleInput>,
        moves: &mut BTreeMap<EntityId, InputFrame>,
    ) -> Report {
        let mut rep = Report::default();
        self.still = if m.player_vel.length() < 1.0 { self.still + m.dt } else { 0.0 };
        for b in &mut self.blame {
            b.1 -= m.dt;
        }
        self.blame.retain(|b| b.1 > 0.0);
        // Gone, wrecked or emptied units leave the roster; dead officers too.
        self.officers.retain(|o| sim.state.entities.get(o.id).is_some_and(|e| !e.name.starts_with("dead:")));
        let officers = &self.officers;
        let mut wrecks = Vec::new();
        self.units.retain(|u| {
            let alive = sim.state.entities.get(u.vehicle).and_then(|e| e.vehicle.as_ref()).is_some_and(|v| !v.state.wrecked);
            let keep = alive || officers.iter().any(|o| o.vehicle == u.vehicle);
            if !keep && sim.state.entities.get(u.vehicle).is_some() {
                wrecks.push(u.vehicle);
            }
            keep
        });
        self.wrecks.extend(wrecks);
        self.pass -= m.dt;
        if self.pass <= 0.0 {
            self.pass = 1.0;
            self.dispatch(sim, m);
        }
        // One car at a time tries a PIT (3 stars and up): the nearest that may.
        let rammer = if m.stars >= 3 && m.player_car.is_some() {
            self.units
                .iter()
                .filter(|u| u.kind != Kind::Air && u.mode == Mode::Pursue && u.ram_t <= 0.0 && !u.rails)
                .filter_map(|u| sim.state.entities.get(u.vehicle).map(|e| (u.vehicle, e.pos.distance(m.player))))
                .filter(|(_, d)| *d < 18.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(id, _)| id)
        } else {
            None
        };
        for i in 0..self.units.len() {
            let mut u = self.units[i].clone();
            if let Some(input) = self.drive(sim, m, &mut u, rammer, &mut rep) {
                vin.insert(u.vehicle, input);
            }
            self.units[i] = u;
        }
        self.officers_tick(sim, m, moves, &mut rep);
        self.roadblocks(sim, m);
        // Leaving units go once far and out of view (with their crews).
        let mut gone = Vec::new();
        for u in &self.units {
            let Some(e) = sim.state.entities.get(u.vehicle) else { continue };
            let far = e.pos.distance(m.player) > 180.0;
            if u.mode == Mode::Leave && far && !crate::traffic::seen(&m.camera, e.pos) {
                gone.push(u.vehicle);
            }
        }
        for v in gone {
            self.units.retain(|u| u.vehicle != v);
            let crew: Vec<EntityId> = self.officers.iter().filter(|o| o.vehicle == v).map(|o| o.id).collect();
            self.officers.retain(|o| o.vehicle != v);
            for c in crew {
                sim.despawn(c);
            }
            sim.despawn(v);
            self.despawned += 1;
        }
        rep
    }

    /// Spawns units up to the level's numbers, out of view along the lanes (helicopters in the
    /// sky, out of view too).
    fn dispatch(&mut self, sim: &mut Sim, m: &Moment) {
        if !m.dispatch || m.stars == 0 {
            return;
        }
        let s = m.stars.min(5) as usize;
        let active = |k: Kind, units: &[Unit]| units.iter().filter(|u| u.kind == k && u.mode != Mode::Leave).count();
        for (kind, want) in [(Kind::Air, AIR[s]), (Kind::Car, CARS[s]), (Kind::Van, VANS[s])] {
            if active(kind, &self.units) >= want {
                continue;
            }
            // One a second per kind keeps arrivals staggered.
            self.spawn(sim, m, kind);
        }
    }

    fn spawn(&mut self, sim: &mut Sim, m: &Moment, kind: Kind) {
        let g = &m.city.lanes;
        for _ in 0..12 {
            let rng = &mut sim.state.rng;
            let a = rng.f32() * std::f32::consts::TAU;
            let r = rng.range(SPAWN_NEAR, SPAWN_FAR);
            let p = m.player + Vec3::new(a.cos() * r, 0.0, a.sin() * r);
            let (pos, yaw) = if kind == Kind::Air {
                let ground = m.city.ground_y(Vec2::new(p.x, p.z));
                let to = m.player - p;
                (Vec3::new(p.x, ground + 45.0, p.z), to.x.atan2(to.z))
            } else {
                let keep = |l: &pav_world::lanes::Lane| {
                    !l.closed && matches!(l.kind, LaneKind::Street | LaneKind::Avenue | LaneKind::Boulevard | LaneKind::Freeway)
                };
                let Some((lane, along, off)) = g.nearest(p, keep) else { continue };
                if off > 25.0 {
                    continue;
                }
                let (pt, dir) = g.lanes[lane as usize].at(along);
                (pt, dir.x.atan2(dir.z))
            };
            let d = pos.distance(m.player);
            if d < 80.0 || crate::traffic::seen(&m.camera, pos) {
                continue;
            }
            if kind != Kind::Air && sim.state.entities.iter().any(|e| e.vehicle.is_some() && e.pos.distance(pos) < 10.0) {
                continue;
            }
            let key = match kind {
                Kind::Car => "sppd_interceptor",
                Kind::Van => "sppd_tactical",
                Kind::Air => "sppd_air",
            };
            let Ok(vehicle) = crate::vehicles::spawn(sim, key, pos + Vec3::Y * 0.05, yaw, None) else { return };
            crate::world::ambient(sim, vehicle);
            let crew_n = match kind {
                Kind::Car => 2,
                Kind::Van => 4,
                Kind::Air => 2,
            };
            let mut crew = Vec::new();
            for seat in 0..crew_n {
                let look = uniform(sim);
                let id = sim.spawn_npc("cop:officer", pos, yaw, look, None, None);
                crate::world::ambient(sim, id);
                if sim.enter_vehicle_seat(id, vehicle, seat) {
                    crew.push(id);
                    self.officers.push(Officer { id, vehicle, seat, duty: Duty::Seated, cooldown: 1.0 });
                } else {
                    sim.despawn(id);
                }
            }
            if crate::traffic::seen(&m.camera, pos) {
                self.spawn_in_view += 1;
            }
            // Far away the ground may not be loaded yet: on rails until near.
            let rails = kind != Kind::Air && (d > RAILS_FAR || !ground_loaded(sim, pos));
            if rails {
                sim.set_vehicle_kinematic(vehicle, true);
            }
            self.units.push(Unit {
                kind,
                vehicle,
                crew,
                mode: Mode::Respond,
                path: Vec::new(),
                replan: 0.0,
                goal: m.last_known,
                stuck: 0.0,
                reverse: 0.0,
                orbit: sim.state.rng.f32() * std::f32::consts::TAU,
                light: m.player,
                unseen: 0.0,
                rails,
                ram_t: 0.0,
            });
            self.spawned += 1;
            return;
        }
    }

    /// Sight of the player from `eye` facing `facing` (a ground direction), within `range` and
    /// a cone of `half` radians.
    fn can_see(sim: &Sim, eye: Vec3, facing: Vec3, range: f32, half: f32, player: Vec3) -> bool {
        let to = player + Vec3::Y * 1.0 - eye;
        let d = to.length();
        if d > range {
            return false;
        }
        let flat = Vec3::new(to.x, 0.0, to.z).normalize_or_zero();
        if d > 4.0 && flat.dot(facing) < half.cos() {
            return false;
        }
        sight(sim, eye, player + Vec3::Y)
    }

    /// One unit's driving this tick (None: no input, e.g. a stopped unit with its crew out).
    fn drive(
        &mut self,
        sim: &mut Sim,
        m: &Moment,
        u: &mut Unit,
        rammer: Option<EntityId>,
        rep: &mut Report,
    ) -> Option<VehicleInput> {
        let c = Car::read(sim, u.vehicle)?;
        let dt = m.dt;
        let crew_in = u.crew.iter().any(|id| {
            sim.state.entities.get(*id).and_then(|e| e.character.as_ref()).is_some_and(|ch| ch.riding == Some(u.vehicle))
        });
        // Sight: cars look ahead, helicopters all round (at night only inside their light).
        let seen = if u.kind == Kind::Air {
            let range = if m.hidden { 35.0 } else { 60.0 };
            let lit = !m.night || u.light.distance(m.player) < 9.0;
            // The crew watch the ground below them (at night, the searchlight's pool): within 55°
            // of straight down, which takes in their orbit round the player — outrun it and they
            // lose you.
            let eye = c.center - Vec3::Y * 1.5;
            let below = (m.player + Vec3::Y - eye).normalize_or_zero().dot(Vec3::NEG_Y) > AIR_VIEW.cos();
            lit && below && Self::can_see(sim, eye, Vec3::ZERO, range, std::f32::consts::PI, m.player)
        } else {
            crew_in
                && Self::can_see(sim, c.center + Vec3::Y * 0.6, c.forward(), if m.hidden { 35.0 } else { 60.0 }, 1.2, m.player)
        };
        if seen {
            u.unseen = 0.0;
            rep.sees = true;
        } else {
            u.unseen += dt;
        }
        if m.stars == 0 && u.mode != Mode::Leave {
            u.mode = Mode::Leave;
        }
        // The mode for this tick.
        if u.mode != Mode::Leave && u.kind != Kind::Air {
            let near = c.center.distance(m.player);
            // On foot, or stopped in a car a moment: pull up and get out.
            let stopped = m.player_car.is_none() || self.still > STILL_HALT;
            u.mode = if stopped && near < 24.0 && (seen || u.unseen < 4.0) {
                Mode::Halt
            } else if u.mode == Mode::Halt && !crew_in {
                // Crew still out: wait for them.
                Mode::Halt
            } else if seen || (u.unseen < 3.0 && !m.searching) {
                Mode::Pursue
            } else if m.searching {
                Mode::Search
            } else {
                Mode::Respond
            };
        }
        if u.kind == Kind::Air {
            return Some(self.fly(sim, m, u, &c));
        }
        // Far away: along the path on rails; near (on loaded ground): back on the wheels.
        let dist = c.center.distance(m.player);
        if u.rails && dist < RAILS_NEAR && ground_loaded(sim, c.feet()) && on_road(m.city, c.feet()) {
            sim.set_vehicle_kinematic(u.vehicle, false);
            u.rails = false;
        } else if !u.rails && (dist > RAILS_FAR + 20.0 || !ground_loaded(sim, c.feet())) {
            sim.set_vehicle_kinematic(u.vehicle, true);
            u.rails = true;
        }
        if u.rails {
            u.replan -= dt;
            if u.replan <= 0.0 || u.path.is_empty() {
                u.replan = 3.0;
                let goal = if u.mode == Mode::Leave {
                    c.center + (c.center - m.player).normalize_or(Vec3::X) * 300.0
                } else {
                    m.last_known
                };
                u.goal = goal;
                u.path = route_from(m.city, c.feet(), goal);
            }
            let pace = if u.mode == Mode::Leave { 14.0 } else { 22.0 };
            let mut feet = c.feet();
            let mut left = pace * dt;
            while left > 0.0 && !u.path.is_empty() {
                let to = u.path[0] - feet;
                let d = to.length();
                if d <= left {
                    feet = u.path.remove(0);
                    left -= d;
                } else {
                    feet += to / d * left;
                    left = 0.0;
                }
            }
            let dir = u.path.first().map(|p| *p - feet).filter(|d| d.length() > 0.1).unwrap_or(c.forward());
            sim.move_kinematic_vehicle(u.vehicle, feet, dir.x.atan2(dir.z));
            return None;
        }
        let speed = c.speed();
        let (target, want, ram) = match u.mode {
            Mode::Halt => {
                // Stop and let the crew out.
                for o in self.officers.iter_mut().filter(|o| o.vehicle == u.vehicle && o.duty == Duty::Seated) {
                    if speed.abs() < 1.5 {
                        sim.exit_vehicle(o.id, u.vehicle);
                        o.duty = Duty::Chase;
                    }
                }
                return Some(VehicleInput {
                    handbrake: speed.abs() < 1.2,
                    brake: if speed > 1.2 { 1.0 } else { 0.0 },
                    siren: true,
                    ..Default::default()
                });
            }
            Mode::Pursue => {
                // Where it sees the player going; out of sight, the last sighting (by radio).
                let lead = if seen { m.player + m.player_vel * 0.7 } else { m.last_known };
                u.ram_t -= dt;
                let ram = rammer == Some(u.vehicle) && seen;
                // Bumper to bumper (or alongside) — contact, or as good as: back off a while.
                if ram && c.center.distance(m.player) < c.half.z * 2.0 + 1.4 {
                    u.ram_t = sim.state.rng.range(4.0, 7.0);
                }
                let target = if ram {
                    // A PIT: the nose into the rear quarter, from the side it is already on.
                    let back = -m.player_vel.normalize_or(Vec3::Z);
                    let side = Vec3::new(back.z, 0.0, -back.x);
                    let s = if (c.center - m.player).dot(side) >= 0.0 { 1.0 } else { -1.0 };
                    m.player + back * 1.6 + side * s * 2.0
                } else if seen && m.player_vel.length() > 4.0 && c.center.distance(m.player) < 16.0 {
                    // Close, not ramming: onto its tail (the lead point from alongside is into
                    // its side).
                    m.player - m.player_vel.normalize() * 7.0
                } else {
                    lead
                };
                // Closing fast from afar, the harder the more stars (at one or two a fast car
                // can pull away; from three they drive flat out); a PIT itself is a nudge (a few
                // m/s faster), not a crash.
                let pv = m.player_vel.length();
                let (gain, top) = match m.stars {
                    0 | 1 => (2.5, 31.0),
                    2 => (4.5, 36.0),
                    _ => (7.0, 42.0),
                };
                let pace = if ram {
                    (pv + 3.0).clamp(4.0, 40.0)
                } else if c.center.distance(m.player) < 30.0 {
                    (pv + gain * 0.6).clamp(8.0, top - 2.0)
                } else {
                    (pv + gain).clamp(14.0, top)
                };
                (target, pace, ram)
            }
            Mode::Respond | Mode::Search | Mode::Leave => {
                u.replan -= dt;
                if u.replan <= 0.0 || u.path.is_empty() {
                    u.replan = 4.0;
                    u.goal = match u.mode {
                        Mode::Respond => m.last_known,
                        Mode::Search => {
                            let (zc, zr) = m.zone.unwrap_or((m.last_known, 120.0));
                            let a = sim.state.rng.f32() * std::f32::consts::TAU;
                            let r = sim.state.rng.range(0.2, 0.9) * zr;
                            zc + Vec3::new(a.cos() * r, 0.0, a.sin() * r)
                        }
                        _ => {
                            let away = (c.center - m.player).normalize_or(Vec3::X);
                            c.center + away * 300.0
                        }
                    };
                    u.path = route_from(m.city, c.feet(), u.goal);
                }
                // Drop the points already passed.
                while u.path.len() > 1 && u.path[0].distance(c.feet()) < 8.0 + speed.abs() * 0.3 {
                    u.path.remove(0);
                }
                let target = u.path.first().copied().unwrap_or(u.goal);
                let pace = match u.mode {
                    Mode::Respond => 24.0,
                    Mode::Search => 11.0,
                    _ => 14.0,
                };
                (target, pace, false)
            }
        };
        // Steering toward the target; the pedals for the pace.
        let to = target - c.center;
        let x = to.dot(c.left());
        let z = to.dot(c.forward());
        let angle = x.atan2(z.max(0.3));
        let mut steer = (-angle * 1.6).clamp(-1.0, 1.0);
        let reach = to.length();
        // Not ramming: close in, then hang back at the player's pace.
        let want = if u.mode == Mode::Pursue && !ram && reach < 14.0 { want.min(m.player_vel.length() * 0.95) } else { want };
        // And off the bumper of the police car ahead (they do not shunt each other along).
        let mut want = want;
        for o in &self.units {
            if o.vehicle == u.vehicle || o.kind == Kind::Air {
                continue;
            }
            let Some(oc) = sim.state.entities.get(o.vehicle).map(|e| e.pos) else { continue };
            let d = oc - c.center;
            let z = d.dot(c.forward());
            if z > 0.0 && z < 14.0 && d.dot(c.left()).abs() < 2.4 && d.y.abs() < 3.0 {
                let ahead_speed = sim.vehicle_status(o.vehicle).map(|s| s.kmh / 3.6).unwrap_or(0.0).max(0.0);
                want = want.min((ahead_speed + (z - 7.0) * 0.8).max(0.0));
            }
        }
        // Slow for sharp turns.
        let want = want * (1.0 - (angle.abs() / 1.6).clamp(0.0, 0.6));
        let mut throttle = ((want - speed) * 0.4).clamp(0.0, 1.0);
        let mut brake = if speed > want + 3.0 && speed > 1.2 { ((speed - want) * 0.15).clamp(0.0, 1.0) } else { 0.0 };
        // Stuck: back up a moment, wheels the other way.
        if speed.abs() < 1.0 && want > 3.0 {
            u.stuck += dt;
        } else {
            u.stuck = 0.0;
        }
        if u.stuck > 2.5 && u.reverse <= 0.0 {
            u.reverse = 1.4;
            u.stuck = 0.0;
        }
        if u.reverse > 0.0 {
            u.reverse -= dt;
            throttle = 0.0;
            brake = if speed > -3.0 { 0.5 } else { 0.0 };
            steer = -steer;
        }
        let _ = rep;
        let siren = u.mode != Mode::Leave;
        Some(VehicleInput { throttle, brake, steer, siren, ..Default::default() })
    }

    /// A helicopter: orbit over the player (or the search zone), its light on the player.
    fn fly(&mut self, sim: &mut Sim, m: &Moment, u: &mut Unit, c: &Car) -> VehicleInput {
        let dt = m.dt;
        let (centre, radius) = match (u.mode, m.zone) {
            (Mode::Leave, _) => (c.center + (c.center - m.player).normalize_or(Vec3::X) * 400.0, 0.0),
            (_, Some((zc, zr))) if u.unseen > 3.0 => (zc, (zr * 0.4).max(40.0)),
            _ => (m.player, 32.0),
        };
        u.orbit += dt * 0.18;
        let ground = m.city.ground_y(Vec2::new(centre.x, centre.z)).max(m.player.y);
        let mut want = Vec3::new(centre.x + u.orbit.cos() * radius, ground + 32.0, centre.z + u.orbit.sin() * radius);
        let to = want - c.center;
        let flat = Vec2::new(to.x, to.z);
        // Over the rooftops: clear of the towers here and on the way (40 m on).
        let here = Vec2::new(c.center.x, c.center.z);
        let on = here + flat.normalize_or_zero() * flat.length().min(40.0);
        want.y = want.y.max(m.city.roof_y(here, 30.0).max(m.city.roof_y(on, 30.0)) + 15.0);
        let to = want - c.center;
        // Face the player when close, else where it is going.
        let look = if flat.length() < 25.0 { m.player - c.center } else { to };
        let yaw_want = look.x.atan2(look.z);
        let err = wrap(yaw_want - c.yaw);
        let steer = (-err * 1.8).clamp(-1.0, 1.0);
        let heading = Vec2::new(c.yaw.sin(), c.yaw.cos());
        let ahead = flat.dot(heading);
        let pitch = (ahead / 25.0).clamp(-0.6, 1.0);
        let up = ((want.y - c.center.y) * 0.25).clamp(-1.0, 1.0);
        // The searchlight swings onto the player (or sweeps the zone).
        let aim = if u.unseen < 3.0 { m.player } else { m.zone.map(|z| z.0).unwrap_or(m.last_known) };
        u.light += (aim - u.light) * (1.0 - (-1.5 * dt).exp());
        // After dark its lamps are lit (the searchlight's lens glows while `draw` casts its beam).
        sim.set_vehicle_lights(u.vehicle, m.night);
        // Its light bar flashes while it hunts, as the cars' sirens do.
        let siren = u.mode != Mode::Leave;
        VehicleInput { throttle: up.max(0.0), brake: (-up).max(0.0), steer, pitch, siren, ..Default::default() }
    }

    /// Officers on foot: close in, arrest, shoot, run back to the car.
    fn officers_tick(&mut self, sim: &mut Sim, m: &Moment, moves: &mut BTreeMap<EntityId, InputFrame>, rep: &mut Report) {
        let dt = m.dt;
        let player_aim = m.player + Vec3::Y * 1.1;
        for i in 0..self.officers.len() {
            let mut o = self.officers[i].clone();
            if o.duty == Duty::Seated {
                continue;
            }
            let Some(feet) = sim.feet(o.id) else { continue };
            let to = m.player - feet;
            let dist = Vec2::new(to.x, to.z).length();
            let facing = sim.state.entities.get(o.id).and_then(|e| e.character.as_ref()).map(|c| c.facing).unwrap_or(0.0);
            let eye = feet + Vec3::Y * 1.6;
            let fwd = Vec3::new(facing.sin(), 0.0, facing.cos());
            let range = if m.hidden { POLICE_SIGHT * 0.55 } else { POLICE_SIGHT };
            let sees = Self::can_see(sim, eye, fwd, range, 1.05, m.player);
            if sees {
                rep.sees = true;
            }
            o.cooldown -= dt;
            // The player drove off: back to the car (if it is still there).
            let car = sim.state.entities.get(o.vehicle).map(|e| e.pos);
            if m.player_car.is_some() && dist > 20.0 {
                o.duty = Duty::Back;
            }
            if m.stars == 0 {
                o.duty = Duty::Back;
            }
            let arrest = m.stars <= 1 || (m.stars <= 2 && !m.firing);
            let mut frame = InputFrame::default();
            match o.duty {
                Duty::Back => match car {
                    Some(cp) if cp.distance(feet) < 2.6 => {
                        if sim.enter_vehicle_seat(o.id, o.vehicle, o.seat) || sim.enter_vehicle_seat(o.id, o.vehicle, 0) {
                            o.duty = Duty::Seated;
                        }
                    }
                    Some(cp) => {
                        let d = cp - feet;
                        frame.move_dir = Vec2::new(d.x, d.z).normalize_or(Vec2::ZERO);
                    }
                    None => {
                        // No car to go back to: stay on foot.
                        if m.stars > 0 {
                            o.duty = Duty::Chase;
                        }
                    }
                },
                Duty::Chase | Duty::Shoot => {
                    let stand = if arrest { 1.4 } else { 13.0 };
                    if dist > stand || !sees {
                        o.duty = Duty::Chase;
                        frame.move_dir = Vec2::new(to.x, to.z).normalize_or(Vec2::ZERO);
                        if arrest && dist < 6.0 {
                            frame.held |= buttons::FOCUS;
                        }
                    } else {
                        o.duty = Duty::Shoot;
                    }
                    if arrest && dist < 2.0 {
                        rep.cuffing = true;
                    }
                    // Shoot from range when it is lethal force (or when fired upon).
                    if !arrest && sees && dist < 40.0 && o.cooldown <= 0.0 {
                        o.cooldown = sim.state.rng.range(0.9, 1.5);
                        let speed = m.player_vel.length();
                        let chance = 0.55
                            * (1.0 - dist / 45.0).clamp(0.15, 1.0)
                            * if speed > 5.0 { 0.55 } else { 1.0 }
                            * if m.hidden { 0.5 } else { 1.0 }
                            * if m.covered { 0.25 } else { 1.0 }
                            // The car's body takes most rounds meant for its driver.
                            * if m.player_car.is_some() { 0.5 } else { 1.0 };
                        let hit = sim.state.rng.f32() < chance;
                        let muzzle = feet + Vec3::Y * 1.35 + fwd * 0.4;
                        let miss = Vec3::new(
                            sim.state.rng.range(-1.5, 1.5),
                            sim.state.rng.range(-0.6, 0.8),
                            sim.state.rng.range(-1.5, 1.5),
                        );
                        let end = if hit { player_aim } else { player_aim + miss };
                        rep.shots.push((muzzle, end, hit));
                        self.shots += 1;
                        if hit {
                            self.hits += 1;
                            rep.hits.push(9.0);
                        }
                    }
                    if let Some(c) = sim.state.entities.get_mut(o.id).and_then(|e| e.character.as_mut()) {
                        c.face = (o.duty == Duty::Shoot).then(|| to.x.atan2(to.z));
                    }
                    crate::combat::aim_pose(sim, o.id, o.duty == Duty::Shoot && !arrest);
                }
                Duty::Seated => {}
            }
            moves.insert(o.id, frame);
            self.officers[i] = o;
        }
    }

    /// Roadblocks: from 3 stars, while the player drives, now and then two cars across the road
    /// ahead (round a corner, out of sight) with their crews out, and from 4 stars a spike strip
    /// before them. Tyres that roll over a strip burst; roadblocks far behind are cleared.
    fn roadblocks(&mut self, sim: &mut Sim, m: &Moment) {
        // Spikes burst the tyres of whatever rolls over them.
        for sp in &self.spikes {
            let across = Vec3::new(-sp.dir.z, 0.0, sp.dir.x);
            let hits: Vec<EntityId> = sim
                .state
                .entities
                .iter()
                .filter(|e| e.vehicle.is_some() && !self.units.iter().any(|u| u.vehicle == e.id))
                .filter(|e| {
                    let d = e.pos - sp.at;
                    d.dot(sp.dir).abs() < 1.2 && d.dot(across).abs() < sp.half + 0.8 && d.y.abs() < 2.0
                })
                .map(|e| e.id)
                .collect();
            for id in hits {
                for w in 0..4 {
                    sim.burst_tyre(id, w);
                }
            }
        }
        // Old roadblocks go once far and out of sight.
        let cam = m.camera;
        let mut keep = Vec::new();
        for b in std::mem::take(&mut self.blocks) {
            if b.at.distance(m.player) > 200.0 && !crate::traffic::seen(&cam, b.at) {
                for c in &b.cars {
                    let crew: Vec<EntityId> = self.officers.iter().filter(|o| o.vehicle == *c).map(|o| o.id).collect();
                    self.officers.retain(|o| o.vehicle != *c);
                    for id in crew {
                        sim.despawn(id);
                    }
                    sim.despawn(*c);
                }
                self.spikes.retain(|s| s.at.distance(b.at) > 30.0);
            } else {
                keep.push(b);
            }
        }
        self.blocks = keep;
        self.block_t -= m.dt;
        if m.stars < 3 || m.player_car.is_none() || self.block_t > 0.0 || !m.dispatch {
            return;
        }
        self.block_t = BLOCK_EVERY;
        // Follow the road the player drives along, ahead, for a spot hidden from the camera.
        let g = &m.city.lanes;
        let fwd = m.player_vel.normalize_or(Vec3::ZERO);
        if fwd == Vec3::ZERO {
            return;
        }
        let keep = |l: &pav_world::lanes::Lane| !l.closed && l.kind != LaneKind::Channel;
        let Some((mut lane, mut along, _)) = g.nearest(m.player, keep) else { return };
        if g.lanes[lane as usize].at(along).1.dot(fwd) < 0.3 {
            return;
        }
        let mut gone = 0.0;
        for _ in 0..40 {
            let l = &g.lanes[lane as usize];
            let left = l.length - along;
            if gone + left >= BLOCK_AHEAD[0] && l.kind != LaneKind::Connector {
                let d = (BLOCK_AHEAD[0] - gone).max(0.0) + 5.0;
                let (at, dir) = l.at(along + d.min(left));
                let hidden = sight_blocked(sim, cam.eye, at + Vec3::Y);
                let across = Vec3::new(-dir.z, 0.0, dir.x) * 2.6;
                let loaded = ground_loaded(sim, at + across) && ground_loaded(sim, at - across);
                if (hidden || gone + d > BLOCK_AHEAD[1]) && at.distance(m.player) > 90.0 && loaded {
                    self.set_block(sim, m, at, dir);
                    return;
                }
            }
            gone += left;
            if gone > BLOCK_AHEAD[1] {
                return;
            }
            // Straight on where possible.
            let next = l.next.iter().copied().filter(|&n| !g.lanes[n as usize].closed).max_by(|&a, &b| {
                let da = g.lanes[a as usize].at(1.0).1.dot(fwd);
                let db = g.lanes[b as usize].at(1.0).1.dot(fwd);
                da.total_cmp(&db)
            });
            let Some(n) = next else { return };
            lane = n;
            along = 0.0;
        }
    }

    /// Two cars across the road at `at` (the road running along `dir`), crews out; a spike
    /// strip 12 m before them from 4 stars.
    fn set_block(&mut self, sim: &mut Sim, m: &Moment, at: Vec3, dir: Vec3) {
        let across = Vec3::new(-dir.z, 0.0, dir.x);
        let yaw = across.x.atan2(across.z);
        let mut cars = Vec::new();
        for side in [-1.0f32, 1.0] {
            let pos = at + across * side * 2.6;
            let Ok(car) = crate::vehicles::spawn(
                sim,
                "sppd_interceptor",
                pos + Vec3::Y * 0.05,
                yaw + if side > 0.0 { std::f32::consts::PI } else { 0.0 },
                None,
            ) else {
                continue;
            };
            for seat in 0..2 {
                let look = uniform(sim);
                let stand = pos - dir * 2.5 + across * (seat as f32 - 0.5) * 1.6;
                let id = sim.spawn_npc("cop:officer", stand, dir.x.atan2(dir.z) + std::f32::consts::PI, look, None, None);
                crate::world::ambient(sim, id);
                self.officers.push(Officer { id, vehicle: car, seat, duty: Duty::Chase, cooldown: 1.5 });
            }
            crate::world::ambient(sim, car);
            sim.set_vehicle_lights(car, true);
            cars.push(car);
        }
        if m.stars >= 4 {
            self.spikes.push(Spike { at: at - dir * 12.0, dir, half: 4.0 });
        }
        self.blocks.push(Roadblock { at, cars });
    }

    /// The units on the radar: police blips (flickering red and blue at `t`).
    pub fn blips(&self, sim: &Sim, t: f32) -> Vec<Blip> {
        let red = (t * 4.0).floor() as i64 % 2 == 0;
        let mut out = Vec::new();
        for u in &self.units {
            let Some(e) = sim.state.entities.get(u.vehicle) else { continue };
            out.push(Blip {
                pos: Vec2::new(e.pos.x, e.pos.z),
                icon: if u.kind == Kind::Air { BlipIcon::Helicopter } else { BlipIcon::Police },
                color: Color::hex(if red { "#ff3b3b" } else { "#3b7bff" }),
                label: None,
                clamp: true,
                height: 0,
                pulse: false,
            });
        }
        for o in self.officers.iter().filter(|o| o.duty != Duty::Seated) {
            let Some(f) = sim.feet(o.id) else { continue };
            out.push(Blip {
                pos: Vec2::new(f.x, f.z),
                icon: BlipIcon::Dot,
                color: Color::hex(if red { "#ff3b3b" } else { "#3b7bff" }),
                label: None,
                clamp: false,
                height: 0,
                pulse: false,
            });
        }
        out
    }

    /// Vision cones on the radar while searching (radar headings: 0 north, east positive).
    pub fn cones(&self, sim: &Sim) -> Vec<ConeBlip> {
        let mut out = Vec::new();
        let color = Color::hex("#ff4d4d");
        for u in self.units.iter().filter(|u| u.kind != Kind::Air) {
            if let Some(c) = Car::read(sim, u.vehicle) {
                out.push(ConeBlip {
                    pos: Vec2::new(c.center.x, c.center.z),
                    dir: wrap(std::f32::consts::PI - c.yaw),
                    angle: 2.4,
                    range: 60.0,
                    color,
                });
            }
        }
        for o in self.officers.iter().filter(|o| o.duty != Duty::Seated) {
            let (Some(f), Some(facing)) =
                (sim.feet(o.id), sim.state.entities.get(o.id).and_then(|e| e.character.as_ref()).map(|c| c.facing))
            else {
                continue;
            };
            out.push(ConeBlip {
                pos: Vec2::new(f.x, f.z),
                dir: wrap(std::f32::consts::PI - facing),
                angle: 2.1,
                range: POLICE_SIGHT,
                color,
            });
        }
        out
    }

    /// Spike strips, and searchlights at night.
    pub fn draw(&self, sim: &Sim, night: bool, draw: &mut DrawList) {
        for sp in &self.spikes {
            let across = Vec3::new(-sp.dir.z, 0.0, sp.dir.x);
            draw.prims.push(pav_core::present::PrimDraw {
                shape: pav_core::shape::Shape::Box { half: Vec3::new(sp.half, 0.04, 0.25) },
                pos: sp.at + Vec3::Y * 0.04,
                rot: glam::Quat::from_rotation_y(across.x.atan2(across.z) - std::f32::consts::FRAC_PI_2),
                color: Color::hex("#22252a"),
                emissive: 0.0,
                look: pav_core::shape::Look::Lit,
                shadow: false,
                glow: Default::default(),
            });
        }
        if !night {
            return;
        }
        for u in self.units.iter().filter(|u| u.kind == Kind::Air) {
            let Some(e) = sim.state.entities.get(u.vehicle) else { continue };
            // From the searchlight under its nose (the body prop's `head_search` lens, on its
            // right: 0.74 m out, 0.3 m up and 1.38 m ahead of the ground under its centre).
            let base = e.vehicle.as_ref().map_or(0.85, |v| v.base);
            let from = e.pos + e.rot * Vec3::new(-0.74, 0.3 - base, 1.38);
            let dir = (u.light - from).normalize_or(Vec3::NEG_Y);
            draw.spots.push(SpotDraw { pos: from, dir, angle: 0.13, range: 140.0, color: Color::hex("#fff4d6"), intensity: 3.0 });
        }
    }
}
