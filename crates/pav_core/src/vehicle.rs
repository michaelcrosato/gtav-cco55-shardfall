//! Vehicles (vehicles v2). A car is a dynamic body on four raycast wheels: rapier's raycast
//! vehicle springs each wheel, and this module's tyre model (`car.rs`) turns the wheel loads into
//! grip (slip angles, a friction ellipse, rear wheels that lock and slide under the handbrake),
//! brakes, the engine's push and the stabilisers that keep a car with free pitch and roll on its
//! wheels: anti-roll bars, a low centre of mass, forces applied near the centre of mass's height,
//! damped rotation and light control in the air, and righting itself after three seconds on its
//! side or roof. A helicopter is an arcade flyer (`heli.rs`).
//!
//! Any vehicle is driven by a [`VehicleInput`]: the game's `Game::vehicles` map first, then a
//! tool's held input ([`Sim::set_vehicle_input`]), then the engine demo's player keys (steer
//! toward the stick), else it idles (parked when nobody sits in it). Crashes damage it and
//! become [`crate::frame::SimEvent::Crash`] (`damage.rs`); [`Vehicle::status`] is what a HUD and
//! the audio read; [`VehicleView`] is what the view draws. [`feel`] measures a definition on its
//! own test track. Data: [`VehicleDef`] (a room's `vehicle = { .. }`, a game's archetypes).
//!
//! Positions in a definition (seats, door hinges, lights, wheels) are in the **body frame**:
//! metres, the origin on the ground under the vehicle's centre at rest, +y up, +z the front and
//! +x the vehicle's own left (the driver's side). A body asset is authored in the same frame.

use glam::{Quat, Vec2, Vec3};
use rapier::control::{DynamicRayCastVehicleController, WheelTuning};
use rapier::prelude::*;
use serde::{Deserialize, Serialize};

use crate::choice_enum;
use crate::entity::{BodyKind, EntityId, Spawn};
use crate::input::{InputFrame, buttons};
use crate::params::{ParamVisitor, Tunable};
use crate::physics::PhysicsState;
use crate::shape::{Shape, Visual};
use crate::sim::Sim;

mod car;
mod damage;
pub mod feel;
mod heli;

pub use damage::{EXPLODE_AFTER, EXPLOSION_RADIUS, FIRE_BELOW, SMOKE_BELOW};

choice_enum! {
    #[derive(Default)]
    pub enum VehicleKind {
        #[default]
        Car => "car",
        Helicopter => "helicopter",
    }
}

choice_enum! {
    /// Which wheels the engine drives.
    #[derive(Default)]
    pub enum Drive {
        Fwd => "fwd",
        #[default]
        Rwd => "rwd",
        Awd => "awd",
    }
}

choice_enum! {
    /// A door's side: the vehicle's own left (+x, the driver's side) or right.
    #[derive(Default)]
    pub enum Side {
        #[default]
        Left => "left",
        Right => "right",
    }
}

/// Where the wheels are (body frame, metres).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct WheelGeom {
    pub radius: f32,
    /// Each wheel centre's distance from the centre line (half the track).
    pub track: f32,
    /// The front and rear axles' z.
    pub front_z: f32,
    pub rear_z: f32,
}

/// The springs at each wheel.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Suspension {
    /// Spring rate per kilogram of vehicle at each wheel (s⁻²): 30 bounces at about 1.75 Hz
    /// and sags 8 cm.
    pub stiffness: f32,
    /// Damping per kilogram at each wheel (s⁻¹).
    pub damping: f32,
    /// How far a wheel moves either way from where it rests (m).
    pub travel: f32,
    /// Anti-roll bars: the share of the spring rate that resists the body leaning.
    pub anti_roll: f32,
}

impl Default for Suspension {
    fn default() -> Self {
        Self { stiffness: 30.0, damping: 3.5, travel: 0.3, anti_roll: 0.3 }
    }
}

/// A door: the body asset's part that swings, about the vertical through its hinge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoorDef {
    /// The part's name in the body asset (`door_l`).
    pub part: String,
    /// The hinge (body frame). Omitted: the part's front edge, on its outer face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hinge: Option<Vec3>,
    pub side: Side,
}

/// Lamp positions (body frame): drawn as lamps on a box body, and where a game puts beams. A
/// body asset's own lamps are its parts named `head_l`, `head_r`, `tail_l`, `tail_r` (also
/// `rev_*` reverse lamps and `siren_*` light-bar lamps).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightsDef {
    #[serde(default)]
    pub head: Vec<Vec3>,
    #[serde(default)]
    pub tail: Vec<Vec3>,
}

/// A vehicle's definition. Unset fields come from the kind's preset ([`VehicleDef::preset`]);
/// the wheels, clearance, centre of mass and drag left out follow from `size`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VehicleDef {
    pub kind: VehicleKind,
    /// What it is, for the game ("compact", "sports", "bus").
    pub class: String,
    pub color: String,
    pub accent: String,
    /// Body collider half extents (x width, y height, z length).
    pub size: Vec3,
    /// The collider's bottom above the ground at rest (m).
    pub clearance: f32,
    /// kg.
    pub mass: f32,
    /// Centre of mass, from the collider's centre (m): low (negative y) keeps it upright.
    pub com: Vec3,
    pub drive: Drive,
    /// Car: engine force per driven wheel at low speed (N). Helicopter: acceleration (m/s²).
    pub power: f32,
    /// The share of `max_speed` above which the engine's force falls as 1/speed (constant
    /// power); 1 keeps the full force up to the top.
    pub knee: f32,
    /// Top speed (m/s): the engine fades over its last 8%.
    pub max_speed: f32,
    /// Car: total brake force (N), 60% at the front.
    pub brake: f32,
    /// Car: tyre grip, a friction coefficient: about the g it corners at.
    pub grip: f32,
    /// Car: the rear tyres' grip while the handbrake locks them (low = long slides).
    pub drift_grip: f32,
    /// Car: steering lock at low speed (degrees): full below 8 m/s.
    pub steer: f32,
    /// Car: the share of the lock left at top speed.
    pub steer_at_top: f32,
    /// N per (m/s)², pressing the tyres down at speed.
    pub downforce: f32,
    /// Air drag, N per (m/s)².
    pub drag: f32,
    pub wheels: WheelGeom,
    pub suspension: Suspension,
    pub health: f32,
    /// A library prop drawn as the body (`LIB/gtf_vehicles/okami_kaze`); empty: a box.
    pub body_asset: String,
    /// A library prop drawn at each wheel (its axle along x, the hub toward +x); empty: plain.
    pub wheel_asset: String,
    pub doors: Vec<DoorDef>,
    /// Seat positions (body frame, the hips): the first is the driver's.
    pub seats: Vec<Vec3>,
    pub lights: LightsDef,
    /// Has a light bar and a siren.
    pub siren: bool,
    pub radio: bool,
    /// Horn pitch (1 = normal).
    pub horn: f32,
    /// Helicopter: climb speed (m/s).
    pub climb: f32,
    /// Helicopter: highest flying height above the ground it was placed on (m).
    pub ceiling: f32,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct WheelGeomRaw {
    radius: Option<f32>,
    track: Option<f32>,
    front_z: Option<f32>,
    rear_z: Option<f32>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct SuspensionRaw {
    stiffness: Option<f32>,
    damping: Option<f32>,
    travel: Option<f32>,
    anti_roll: Option<f32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VehicleDefRaw {
    kind: Option<VehicleKind>,
    class: Option<String>,
    color: Option<String>,
    accent: Option<String>,
    size: Option<Vec3>,
    clearance: Option<f32>,
    mass: Option<f32>,
    com: Option<Vec3>,
    drive: Option<Drive>,
    power: Option<f32>,
    knee: Option<f32>,
    max_speed: Option<f32>,
    brake: Option<f32>,
    grip: Option<f32>,
    drift_grip: Option<f32>,
    steer: Option<f32>,
    steer_at_top: Option<f32>,
    downforce: Option<f32>,
    drag: Option<f32>,
    wheels: Option<WheelGeomRaw>,
    suspension: Option<SuspensionRaw>,
    health: Option<f32>,
    #[serde(alias = "body")]
    body_asset: Option<String>,
    #[serde(alias = "wheel")]
    wheel_asset: Option<String>,
    doors: Option<Vec<DoorDef>>,
    seats: Option<Vec<Vec3>>,
    lights: Option<LightsDef>,
    siren: Option<bool>,
    radio: Option<bool>,
    horn: Option<f32>,
    climb: Option<f32>,
    ceiling: Option<f32>,
}

impl<'de> Deserialize<'de> for VehicleDef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = VehicleDefRaw::deserialize(d)?;
        let mut v = VehicleDef::preset(r.kind.unwrap_or_default());
        macro_rules! set {
            ($($f:ident),*) => { $( if let Some(x) = r.$f { v.$f = x; } )* };
        }
        set!(
            class,
            color,
            accent,
            mass,
            drive,
            power,
            knee,
            max_speed,
            brake,
            grip,
            drift_grip,
            steer,
            steer_at_top,
            downforce,
            health,
            body_asset,
            wheel_asset,
            doors,
            seats,
            lights,
            siren,
            radio,
            horn,
            climb,
            ceiling
        );
        if let Some(size) = r.size {
            v = v.with_size(size);
        }
        let w = r.wheels.unwrap_or_default();
        v.wheels = WheelGeom {
            radius: w.radius.unwrap_or(v.wheels.radius),
            track: w.track.unwrap_or(v.wheels.track),
            front_z: w.front_z.unwrap_or(v.wheels.front_z),
            rear_z: w.rear_z.unwrap_or(v.wheels.rear_z),
        };
        if w.radius.is_some() && r.clearance.is_none() {
            v.clearance = 0.9 * v.wheels.radius;
        }
        let s = r.suspension.unwrap_or_default();
        v.suspension = Suspension {
            stiffness: s.stiffness.unwrap_or(v.suspension.stiffness),
            damping: s.damping.unwrap_or(v.suspension.damping),
            travel: s.travel.unwrap_or(v.suspension.travel),
            anti_roll: s.anti_roll.unwrap_or(v.suspension.anti_roll),
        };
        set!(clearance, com, drag);
        Ok(v)
    }
}

impl Default for VehicleDef {
    fn default() -> Self {
        Self::preset(VehicleKind::Car)
    }
}

impl VehicleDef {
    pub fn preset(kind: VehicleKind) -> Self {
        let base = Self {
            kind,
            class: String::new(),
            color: "#e8443a".into(),
            accent: "#2b2f3a".into(),
            size: Vec3::ONE,
            clearance: 0.0,
            mass: 900.0,
            com: Vec3::ZERO,
            drive: Drive::Rwd,
            power: 2600.0,
            knee: 1.0,
            max_speed: 22.0,
            brake: 9600.0,
            grip: 3.0,
            drift_grip: 0.6,
            steer: 32.0,
            steer_at_top: 1.0,
            downforce: 0.0,
            drag: 0.0,
            wheels: WheelGeom { radius: 0.3, track: 0.8, front_z: 1.2, rear_z: -1.2 },
            suspension: Suspension::default(),
            health: 1000.0,
            body_asset: String::new(),
            wheel_asset: String::new(),
            doors: Vec::new(),
            seats: Vec::new(),
            lights: LightsDef::default(),
            siren: false,
            radio: false,
            horn: 1.0,
            climb: 0.0,
            ceiling: 0.0,
        };
        match kind {
            VehicleKind::Car => base.with_size(Vec3::new(0.85, 0.32, 1.9)),
            VehicleKind::Helicopter => Self {
                color: "#3a7be0".into(),
                accent: "#f2c14e".into(),
                mass: 600.0,
                power: 12.0,
                max_speed: 14.0,
                grip: 0.0,
                drift_grip: 0.0,
                steer: 0.0,
                climb: 5.0,
                ceiling: 14.0,
                ..base.with_size(Vec3::new(0.8, 0.7, 1.4))
            },
        }
    }

    /// The same definition with another body size, and the wheels, clearance, centre of mass
    /// and drag that follow from it.
    pub fn with_size(mut self, size: Vec3) -> Self {
        self.size = size;
        let radius = (size.y * 1.1).max(0.25);
        self.wheels = WheelGeom { radius, track: size.x - 0.08, front_z: size.z * 0.68, rear_z: -size.z * 0.68 };
        self.clearance = 0.9 * radius;
        self.com = Vec3::new(0.0, -0.4 * size.y, 0.0);
        // About 0.35 × the frontal area, at sea level.
        self.drag = 0.5 * 1.225 * 0.35 * 0.85 * (2.0 * size.x) * (2.0 * size.y + self.clearance);
        self
    }

    /// The collider centre's height above the ground at rest.
    pub fn base(&self) -> f32 {
        match self.kind {
            VehicleKind::Car => self.clearance + self.size.y,
            VehicleKind::Helicopter => self.size.y,
        }
    }

    /// Wheels the engine drives (front left, front right, rear left, rear right).
    pub fn driven(&self) -> [bool; 4] {
        match self.drive {
            Drive::Fwd => [true, true, false, false],
            Drive::Rwd => [false, false, true, true],
            Drive::Awd => [true; 4],
        }
    }
}

impl Tunable for VehicleDef {
    fn visit(&mut self, v: &mut dyn ParamVisitor) {
        v.float("power", &mut self.power, 0.0, 40000.0, "Engine force per driven wheel (car) / acceleration (helicopter)");
        v.float("knee", &mut self.knee, 0.05, 1.0, "Share of top speed where the engine turns to constant power");
        v.float("max_speed", &mut self.max_speed, 1.0, 80.0, "Top speed (m/s)");
        v.float("brake", &mut self.brake, 0.0, 200000.0, "Total brake force (N)");
        v.float("grip", &mut self.grip, 0.1, 10.0, "Tyre grip (friction coefficient)");
        v.float("drift_grip", &mut self.drift_grip, 0.05, 5.0, "Rear grip with the handbrake (drift)");
        v.float("steer", &mut self.steer, 5.0, 60.0, "Steering lock (degrees)");
        v.float("steer_at_top", &mut self.steer_at_top, 0.1, 1.0, "Share of the lock at top speed");
        v.float("downforce", &mut self.downforce, 0.0, 10.0, "Downforce (N per (m/s)²)");
        v.float("climb", &mut self.climb, 0.5, 15.0, "Helicopter climb speed (m/s)");
        v.float("ceiling", &mut self.ceiling, 1.0, 200.0, "Helicopter: highest flying height (m)");
    }
}

/// A door's swing: how open it is and where it is going (0 shut .. 1 open).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DoorState {
    pub open: f32,
    pub target: f32,
}

/// What changes about a vehicle while it is driven and crashed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VehicleState {
    pub health: f32,
    /// Dents, 0..1: front, back, left, right.
    pub crumple: [f32; 4],
    /// Burst tyres: front left, front right, rear left, rear right.
    pub burst: [bool; 4],
    /// Broken lamps: head left, head right, tail left, tail right.
    pub lights_broken: [bool; 4],
    pub burning: bool,
    /// Seconds it has burned (it explodes at `EXPLODE_AFTER`).
    pub burn_time: f32,
    pub wrecked: bool,
    pub engine_on: bool,
    /// Headlights (the game turns them on at dusk).
    pub lights_on: bool,
    pub siren_on: bool,
    pub horn: bool,
    pub braking: bool,
    pub reversing: bool,
    pub doors: Vec<DoorState>,
    /// Moved by the game (traffic far away), not by its wheels.
    pub kinematic: bool,
    /// m/s along its heading (negative backing up).
    pub speed: f32,
    /// Engine speed 0..1 within the gear.
    pub rpm: f32,
    /// -1 reverse, 0 neutral, 1.. forward.
    pub gear: i8,
    /// How fast the most-sliding tyre slides (m/s): tyre squeal.
    pub slip: f32,
    pub wheels_down: u8,
    /// Seconds without a wheel on the ground.
    pub air_time: f32,
    /// Seconds on its side or roof (it rights itself at 3).
    pub upside: f32,
    pub righting: bool,
    /// Share of the body under water (0..1), seconds in deep water, and whether the engine
    /// drowned.
    pub water: f32,
    pub flood_time: f32,
    pub flooded: bool,
    /// A tool's held input (used when the game gives none).
    pub hold: Option<VehicleInput>,
    /// Seconds since the last crash event.
    pub since_crash: f32,
    /// The pose a game last moved it to while kinematic (centre), for its wheels' spin.
    pub kinematic_from: Option<Vec3>,
}

impl Default for VehicleState {
    fn default() -> Self {
        Self {
            health: 1000.0,
            crumple: [0.0; 4],
            burst: [false; 4],
            lights_broken: [false; 4],
            burning: false,
            burn_time: 0.0,
            wrecked: false,
            engine_on: false,
            lights_on: false,
            siren_on: false,
            horn: false,
            braking: false,
            reversing: false,
            doors: Vec::new(),
            kinematic: false,
            speed: 0.0,
            rpm: 0.0,
            gear: 0,
            slip: 0.0,
            wheels_down: 0,
            air_time: 0.0,
            upside: 0.0,
            righting: false,
            water: 0.0,
            flood_time: 0.0,
            flooded: false,
            hold: None,
            since_crash: 99.0,
            kinematic_from: None,
        }
    }
}

/// A drivable vehicle (an entity component).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vehicle {
    pub def: VehicleDef,
    /// Height of the body centre above the ground it stands on at rest.
    pub base: f32,
    /// Height of the ground it was placed on (the helicopter's ceiling counts from it).
    #[serde(default)]
    pub ground: f32,
    pub car: Option<DynamicRayCastVehicleController>,
    /// Who sits in the driver's seat.
    pub driver: Option<EntityId>,
    /// Who sits in the other seats (`def.seats[1..]`).
    #[serde(default)]
    pub passengers: Vec<Option<EntityId>>,
    /// Smoothed steering, -1 full left .. 1 full right.
    pub steer: f32,
    /// Smoothed engine demand, -1 (reverse) .. 1.
    pub throttle: f32,
    /// Smoothed sideways slide (m/s), for tyre smoke.
    pub drift: f32,
    pub rotor: f32,
    pub rotor_speed: f32,
    /// Helicopter: smoothed tilt (pitch, roll) and heading.
    pub tilt: Vec2,
    pub yaw: f32,
    #[serde(default)]
    pub state: VehicleState,
}

/// One tick of driving for any vehicle, from a game (`Game::vehicles`): the player's car from
/// the controller, traffic and police from their drivers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VehicleInput {
    /// Accelerate, 0..1 (R2).
    pub throttle: f32,
    /// Brake, 0..1 (L2); reverses once (nearly) stopped.
    pub brake: f32,
    /// -1 full left .. 1 full right (helicopter: turn).
    pub steer: f32,
    /// Helicopter: -1 back .. 1 forward. A car in the air: nose down (1) or up (-1).
    #[serde(default)]
    pub pitch: f32,
    /// Locks the rear wheels (cars slide).
    #[serde(default)]
    pub handbrake: bool,
    /// Sounds the horn while held.
    #[serde(default)]
    pub horn: bool,
    /// The siren and light bar (vehicles with `siren`), on while true.
    #[serde(default)]
    pub siren: bool,
}

/// A vehicle's state for a HUD, the audio and tools ([`Vehicle::status`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct VehicleStatus {
    /// m/s along its heading (negative backing up), and km/h (unsigned).
    pub speed: f32,
    pub kmh: f32,
    /// Engine speed 0..1 within the gear, the gear (-1 reverse, 0 neutral) and the throttle.
    pub rpm: f32,
    pub gear: i8,
    pub throttle: f32,
    /// The most-sliding tyre's slide (m/s) and the smoothed sideways drift.
    pub slip: f32,
    pub drift: f32,
    /// Health as a share of the definition's (0..1).
    pub health: f32,
    pub smoking: bool,
    pub burning: bool,
    pub wrecked: bool,
    pub crumple: [f32; 4],
    pub burst: [bool; 4],
    pub lights_broken: [bool; 4],
    pub engine_on: bool,
    pub lights_on: bool,
    pub braking: bool,
    pub reversing: bool,
    pub siren_on: bool,
    pub horn: bool,
    pub wheels_down: u8,
    pub airborne: bool,
    pub upside_down: bool,
    pub in_water: bool,
    pub flooded: bool,
    pub kinematic: bool,
    /// Who sits where (the driver first).
    pub occupants: Vec<Option<EntityId>>,
}

/// A wheel as drawn (body frame).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelView {
    pub center: Vec3,
    /// Spin about its axle and steering angle (radians; positive turns left).
    pub spin: f32,
    pub steer: f32,
    pub radius: f32,
    /// On the vehicle's left (+x).
    pub left: bool,
    pub burst: bool,
}

/// A door as drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct DoorView {
    pub part: String,
    /// Body frame; None: the part's front edge.
    pub hinge: Option<Vec3>,
    pub side: Side,
    pub open: f32,
}

/// The lamps' states.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LampView {
    pub head: bool,
    pub brake: bool,
    pub reverse: bool,
    /// Head left, head right, tail left, tail right.
    pub broken: [bool; 4],
    /// Has a light bar, and it is on.
    pub siren: bool,
    pub siren_on: bool,
}

/// What the view needs to draw a vehicle. Positions are in the body frame (the ground under the
/// centre at rest is the origin): the entity's position is at `(0, base, 0)`.
#[derive(Clone, Debug, PartialEq)]
pub struct VehicleView {
    pub kind: VehicleKind,
    pub half: Vec3,
    pub base: f32,
    pub color: String,
    pub accent: String,
    pub body_asset: Option<String>,
    pub wheel_asset: Option<String>,
    pub wheels: Vec<WheelView>,
    pub rotor: f32,
    pub drift: f32,
    pub driven: bool,
    pub doors: Vec<DoorView>,
    pub head: Vec<Vec3>,
    pub tail: Vec<Vec3>,
    pub lamps: LampView,
    pub crumple: [f32; 4],
    /// Seats someone sits in.
    pub seated: Vec<Vec3>,
    /// Health share (0..1), on fire, burnt out.
    pub health: f32,
    pub burning: bool,
    pub wrecked: bool,
}

impl Vehicle {
    fn new(def: VehicleDef, base: f32, ground: f32, car: Option<DynamicRayCastVehicleController>, yaw: f32) -> Self {
        let seats = def.seats.len().saturating_sub(1);
        let doors = vec![DoorState::default(); def.doors.len()];
        let health = def.health;
        Self {
            base,
            ground,
            def,
            car,
            driver: None,
            passengers: vec![None; seats],
            steer: 0.0,
            throttle: 0.0,
            drift: 0.0,
            rotor: 0.0,
            rotor_speed: 0.0,
            tilt: Vec2::ZERO,
            yaw,
            state: VehicleState { health, doors, ..Default::default() },
        }
    }

    /// Who sits where: the driver, then the other seats.
    pub fn occupants(&self) -> Vec<Option<EntityId>> {
        std::iter::once(self.driver).chain(self.passengers.iter().copied()).collect()
    }

    /// The seat `who` sits in.
    pub fn seat_of(&self, who: EntityId) -> Option<usize> {
        self.occupants().iter().position(|o| *o == Some(who))
    }

    /// Health as a share of the definition's.
    pub fn health_share(&self) -> f32 {
        (self.state.health / self.def.health.max(1.0)).clamp(0.0, 1.0)
    }

    pub fn status(&self) -> VehicleStatus {
        let s = &self.state;
        let health = self.health_share();
        VehicleStatus {
            speed: s.speed,
            kmh: s.speed.abs() * 3.6,
            rpm: s.rpm,
            gear: s.gear,
            throttle: self.throttle,
            slip: s.slip,
            drift: self.drift,
            health,
            smoking: !s.wrecked && health < SMOKE_BELOW,
            burning: s.burning,
            wrecked: s.wrecked,
            crumple: s.crumple,
            burst: s.burst,
            lights_broken: s.lights_broken,
            engine_on: s.engine_on,
            lights_on: s.lights_on,
            braking: s.braking,
            reversing: s.reversing,
            siren_on: s.siren_on,
            horn: s.horn,
            wheels_down: s.wheels_down,
            airborne: self.def.kind == VehicleKind::Car && s.wheels_down == 0 && s.air_time > 0.1,
            upside_down: s.upside > 0.0,
            in_water: s.water > 0.05,
            flooded: s.flooded,
            kinematic: s.kinematic,
            occupants: self.occupants(),
        }
    }

    /// What the view draws, for the entity at `pos` turned `rot`.
    pub fn view(&self, pos: Vec3, rot: Quat) -> VehicleView {
        let to_body = |world: Vec3| rot.inverse() * (world - pos) + Vec3::Y * self.base;
        let wheels = self
            .car
            .as_ref()
            .map(|c| {
                c.wheels()
                    .iter()
                    .enumerate()
                    .map(|(i, w)| WheelView {
                        center: if self.state.kinematic { self.rest_wheel(i) } else { to_body(w.center()) },
                        spin: w.rotation as f32,
                        steer: w.steering as f32,
                        radius: w.radius as f32,
                        left: w.chassis_connection_point_cs.x > 0.0,
                        burst: self.state.burst.get(i).copied().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let seated = self
            .occupants()
            .iter()
            .enumerate()
            .filter(|(_, o)| o.is_some())
            .filter_map(|(i, _)| self.def.seats.get(i).copied())
            .collect();
        let s = &self.state;
        VehicleView {
            kind: self.def.kind,
            half: self.def.size,
            base: self.base,
            color: self.def.color.clone(),
            accent: self.def.accent.clone(),
            body_asset: (!self.def.body_asset.is_empty()).then(|| self.def.body_asset.clone()),
            wheel_asset: (!self.def.wheel_asset.is_empty()).then(|| self.def.wheel_asset.clone()),
            wheels,
            rotor: self.rotor,
            drift: self.drift,
            driven: self.driver.is_some(),
            doors: self
                .def
                .doors
                .iter()
                .zip(s.doors.iter().copied().chain(std::iter::repeat(DoorState::default())))
                .map(|(d, st)| DoorView { part: d.part.clone(), hinge: d.hinge, side: d.side, open: st.open })
                .collect(),
            head: self.def.lights.head.clone(),
            tail: self.def.lights.tail.clone(),
            lamps: LampView {
                head: s.lights_on && !s.wrecked,
                brake: s.braking,
                reverse: s.reversing,
                broken: s.lights_broken,
                siren: self.def.siren,
                siren_on: self.def.siren && s.siren_on && !s.wrecked,
            },
            crumple: s.crumple,
            seated,
            health: self.health_share(),
            burning: s.burning,
            wrecked: s.wrecked,
        }
    }

    /// Wheel `i`'s centre at rest (body frame).
    fn rest_wheel(&self, i: usize) -> Vec3 {
        let w = &self.def.wheels;
        let x = if i.is_multiple_of(2) { w.track } else { -w.track };
        let z = if i < 2 { w.front_z } else { w.rear_z };
        Vec3::new(x, w.radius, z)
    }

    /// Where a rider of height `height` in seat `seat` is (capsule centre), for the vehicle at
    /// `pos` turned `rot`. Without seats: on top of the body's bottom, as the engine demo's cars
    /// always did.
    pub fn rider_center(&self, pos: Vec3, rot: Quat, seat: usize, height: f32) -> Vec3 {
        match self.def.seats.get(seat).or(self.def.seats.first()) {
            Some(s) => pos + rot * (*s - Vec3::Y * self.base) + Vec3::Y * 0.1,
            None => pos + Vec3::Y * (height * 0.5 - self.def.size.y),
        }
    }
}

fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Rapier's raycast vehicle for `def`: four wheels (front left, front right, rear left, rear
/// right) whose springs hold the body at `def.base()` when it rests on flat ground.
fn wheel_controller(def: &VehicleDef, body: RigidBodyHandle, gravity: f32) -> DynamicRayCastVehicleController {
    let mut c = DynamicRayCastVehicleController::new(body);
    c.index_forward_axis = 2;
    let s = &def.suspension;
    let w = &def.wheels;
    let rest = (s.travel + 0.05).max(0.1);
    let tuning = WheelTuning {
        suspension_stiffness: s.stiffness,
        suspension_compression: s.damping * 0.95,
        suspension_damping: s.damping,
        max_suspension_travel: s.travel,
        side_friction_stiffness: 0.0,
        friction_slip: 1.0,
        // Plenty for hard landings: twice the vehicle's weight at each wheel.
        max_suspension_force: def.mass * gravity * 2.0,
    };
    let base = def.base();
    // Each axle's share of the weight (the centre of mass between them), and the sag it gives.
    let span = (w.front_z - w.rear_z).max(0.1);
    let front_share = ((def.com.z - w.rear_z) / span).clamp(0.1, 0.9);
    for (x, z, share) in [
        (w.track, w.front_z, front_share),
        (-w.track, w.front_z, front_share),
        (w.track, w.rear_z, 1.0 - front_share),
        (-w.track, w.rear_z, 1.0 - front_share),
    ] {
        let sag = (share * gravity / (2.0 * s.stiffness.max(1.0))).min(s.travel * 0.9);
        let y = w.radius - base + rest - sag;
        c.add_wheel(Vec3::new(x, y, z), Vec3::NEG_Y, Vec3::NEG_X, rest, w.radius, &tuning);
    }
    c
}

/// Sets a vehicle body's mass, centre of mass, inertia, damping, rotations and crash events
/// (when it is made, and again when its region wakes up), and makes it kinematic if asked.
pub(crate) fn configure_body(ph: &mut PhysicsState, body: RigidBodyHandle, def: &VehicleDef, kinematic: bool) {
    let heli = def.kind == VehicleKind::Helicopter;
    let colliders: Vec<ColliderHandle> = ph.bodies.get(body).map(|b| b.colliders().to_vec()).unwrap_or_default();
    let h = def.size;
    let m = def.mass.max(1.0);
    // A uniform box's inertia; a vehicle's mass sits nearer its middle (engine, people), so it
    // turns a little more readily than that.
    let inertia =
        Vec3::new(m / 3.0 * (h.y * h.y + h.z * h.z), 0.8 * m / 3.0 * (h.x * h.x + h.z * h.z), m / 3.0 * (h.x * h.x + h.y * h.y));
    for (i, c) in colliders.iter().enumerate() {
        if let Some(col) = ph.colliders.get_mut(*c) {
            if i == 0 {
                col.set_mass_properties(MassProperties::new(if heli { Vec3::ZERO } else { def.com }, m, inertia));
            }
            col.set_active_events(ActiveEvents::CONTACT_FORCE_EVENTS);
            // Bumps a sound would follow: half a metre per second of change in one tick.
            col.set_contact_force_event_threshold((m * 30.0) as Real);
        }
    }
    if let Some(b) = ph.bodies.get_mut(body) {
        // Helicopters are posed by their controller; cars pitch and roll freely.
        b.set_enabled_rotations(!heli, !heli, !heli, true);
        b.set_angular_damping(if heli { 2.0 } else { 0.4 });
        b.set_linear_damping(if heli { 0.05 } else { 0.0 });
        if kinematic {
            b.set_body_type(RigidBodyType::KinematicPositionBased, true);
        }
    }
}

impl Sim {
    /// Creates a vehicle at `pos` (the ground under its centre) facing `rot`.
    pub fn spawn_vehicle(
        &mut self,
        name: &str,
        def: VehicleDef,
        pos: Vec3,
        rot: Quat,
        region: Option<crate::statics::RegionKey>,
    ) -> EntityId {
        let half = def.size;
        let base = def.base();
        let center = pos + Vec3::Y * base;
        let volume = 8.0 * half.x * half.y * half.z;
        let mut v =
            Visual::new(Shape::RoundedBox { half, radius: half.y.min(half.x) * 0.45 }, crate::color::Color::hex(&def.color));
        v.look = crate::shape::Look::Cel;
        let mut sp = Spawn::new(name, center).visual(v).body(BodyKind::Dynamic).rot(rot).density(def.mass / volume).friction(0.4);
        sp.region = region;
        let id = self.spawn(sp);
        let Some(body) = self.state.entities.get(id).and_then(|e| e.body) else { return id };
        configure_body(&mut self.state.physics, body, &def, false);
        let car = (def.kind == VehicleKind::Car).then(|| wheel_controller(&def, body, self.config.gravity));
        let yaw = {
            let f = rot * Vec3::Z;
            f.x.atan2(f.z)
        };
        if let Some(e) = self.state.entities.get_mut(id) {
            e.vehicle = Some(Box::new(Vehicle::new(def, base, pos.y, car, yaw)));
        }
        id
    }

    /// Puts a vehicle down at `feet` facing `yaw`, stopped (respawns while driving).
    pub fn place_vehicle(&mut self, vid: EntityId, feet: Vec3, yaw: f32) {
        let Some(e) = self.state.entities.get_mut(vid) else { return };
        let Some(v) = e.vehicle.as_mut() else { return };
        v.yaw = yaw;
        v.tilt = Vec2::ZERO;
        v.state.upside = 0.0;
        v.state.righting = false;
        let center = feet + Vec3::Y * v.base;
        let rot = Quat::from_rotation_y(yaw);
        e.pos = center;
        e.rot = rot;
        if let Some(b) = e.body.and_then(|h| self.state.physics.bodies.get_mut(h)) {
            b.set_position(Pose::from_parts(center, rot), true);
            if b.is_kinematic() {
                b.set_next_kinematic_position(Pose::from_parts(center, rot));
            }
            b.set_linvel(Vec3::ZERO, true);
            b.set_angvel(Vec3::ZERO, true);
        }
    }

    /// Riders sit in their vehicles (after the physics step moved them).
    pub(crate) fn seat_riders(&mut self) {
        let seats: Vec<(EntityId, Vec3)> = self
            .state
            .entities
            .iter()
            .filter_map(|e| {
                let ch = e.character.as_ref()?;
                let ve = self.state.entities.get(ch.riding?)?;
                let v = ve.vehicle.as_ref()?;
                let seat = v.seat_of(e.id).unwrap_or(0);
                Some((e.id, v.rider_center(ve.pos, ve.rot, seat, ch.height())))
            })
            .collect();
        for (id, seat) in seats {
            if let Some(e) = self.state.entities.get_mut(id) {
                e.pos = seat;
            }
        }
    }

    /// Gets the player in / out of the nearest vehicle (interact).
    pub(crate) fn vehicle_interact(&mut self, input: &InputFrame) {
        if !input.just(buttons::INTERACT) {
            return;
        }
        let Some(pid) = self.state.player else { return };
        let riding = self.state.entities.get(pid).and_then(|e| e.character.as_ref()).and_then(|c| c.riding);
        match riding {
            Some(vid) => self.exit_vehicle(pid, vid),
            None => {
                let p = self.state.entities.get(pid).map(|e| e.pos).unwrap_or_default();
                let near = self
                    .state
                    .entities
                    .iter()
                    .filter(|e| e.vehicle.as_ref().is_some_and(|v| v.driver.is_none()))
                    .map(|e| (e.id, e.pos.distance(p)))
                    .filter(|(_, d)| *d < 3.2)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((vid, _)) = near {
                    self.enter_vehicle(pid, vid);
                }
            }
        }
    }

    /// Puts `who` in the driver's seat of `vid` (the engine starts).
    pub fn enter_vehicle(&mut self, who: EntityId, vid: EntityId) {
        self.enter_vehicle_seat(who, vid, 0);
    }

    /// Puts `who` in seat `seat` of `vid` (0 drives; the engine starts). False when the seat is
    /// taken or missing (a vehicle without seats has the driver's only).
    pub fn enter_vehicle_seat(&mut self, who: EntityId, vid: EntityId, seat: usize) -> bool {
        let Some(v) = self.state.entities.get_mut(vid).and_then(|e| e.vehicle.as_mut()) else { return false };
        match seat {
            0 if v.driver.is_none() || v.driver == Some(who) => {
                v.driver = Some(who);
                if !v.state.wrecked && !v.state.flooded {
                    v.state.engine_on = true;
                }
            }
            0 => return false,
            n => match v.passengers.get_mut(n - 1) {
                Some(slot) if slot.is_none() => *slot = Some(who),
                _ => return false,
            },
        }
        if let Some(b) = self.state.entities.get(vid).and_then(|e| e.body).and_then(|h| self.state.physics.bodies.get_mut(h)) {
            b.wake_up(true);
        }
        let body = self.state.entities.get(who).and_then(|e| e.body);
        if let Some(ch) = self.state.entities.get_mut(who).and_then(|e| e.character.as_mut()) {
            ch.riding = Some(vid);
            ch.vel = Vec3::ZERO;
        }
        // The rider's capsule stops colliding while seated.
        if let Some(b) = body.and_then(|h| self.state.physics.bodies.get(h)) {
            for c in b.colliders().to_vec() {
                if let Some(col) = self.state.physics.colliders.get_mut(c) {
                    col.set_sensor(true);
                }
            }
        }
        self.events.push(crate::frame::SimEvent::Pad { pos: self.state.entities.get(vid).map(|e| e.pos).unwrap_or_default() });
        true
    }

    /// Takes `who` out of `vid`, standing beside its seat's door on the ground.
    pub fn exit_vehicle(&mut self, who: EntityId, vid: EntityId) {
        let (pos, rot, half, base, seat) = match self.state.entities.get(vid) {
            Some(e) => match e.vehicle.as_ref() {
                Some(v) => {
                    let seat = v.seat_of(who).and_then(|s| v.def.seats.get(s).copied());
                    (e.pos, e.rot, v.def.size, v.base, seat)
                }
                None => (e.pos, e.rot, Vec3::ONE, 1.0, None),
            },
            None => (Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, 1.0, None),
        };
        if let Some(v) = self.state.entities.get_mut(vid).and_then(|e| e.vehicle.as_mut()) {
            if v.driver == Some(who) {
                v.driver = None;
                v.throttle = 0.0;
                v.state.engine_on = false;
            }
            for p in v.passengers.iter_mut().filter(|p| **p == Some(who)) {
                *p = None;
            }
        }
        let body = self.state.entities.get(who).and_then(|e| e.body);
        if let Some(b) = body.and_then(|h| self.state.physics.bodies.get(h)) {
            for c in b.colliders().to_vec() {
                if let Some(col) = self.state.physics.colliders.get_mut(c) {
                    col.set_sensor(false);
                }
            }
        }
        if let Some(ch) = self.state.entities.get_mut(who).and_then(|e| e.character.as_mut()) {
            ch.riding = None;
        }
        // Out of the seat's side (the demo's cars without seats: their right), on the ground.
        let side = match seat {
            Some(s) if s.x >= 0.0 => rot * Vec3::X,
            Some(_) => rot * Vec3::NEG_X,
            None => rot * Vec3::NEG_X,
        };
        let side = Vec3::new(side.x, 0.0, side.z).normalize_or(Vec3::X);
        let out = pos + side * (half.x + 0.7) - Vec3::Y * if seat.is_some() { base } else { half.y };
        self.set_position(who, Vec3::new(out.x, out.y.max(pos.y - base - 1.0), out.z));
    }

    /// Switches a vehicle between its wheels (dynamic) and positions the game sets
    /// ([`Sim::move_kinematic_vehicle`]: far-away traffic). Coming back to its wheels it keeps
    /// the speed it was moved at.
    pub fn set_vehicle_kinematic(&mut self, vid: EntityId, on: bool) -> bool {
        let Some(e) = self.state.entities.get_mut(vid) else { return false };
        let (Some(body), Some(v)) = (e.body, e.vehicle.as_mut()) else { return false };
        if v.state.kinematic == on {
            return true;
        }
        v.state.kinematic = on;
        v.state.kinematic_from = None;
        let speed = v.state.speed;
        let Some(b) = self.state.physics.bodies.get_mut(body) else { return false };
        if on {
            b.set_body_type(RigidBodyType::KinematicPositionBased, true);
            let pose = *b.position();
            b.set_next_kinematic_position(pose);
        } else {
            // It leaves at the speed the game moved it.
            let fwd = *b.rotation() * Vec3::Z;
            b.set_body_type(RigidBodyType::Dynamic, true);
            b.set_linvel(fwd * speed, true);
            b.set_angvel(Vec3::ZERO, true);
        }
        true
    }

    /// Moves a kinematic vehicle ([`Sim::set_vehicle_kinematic`]) to stand at `feet` facing
    /// `yaw` after the next physics step.
    pub fn move_kinematic_vehicle(&mut self, vid: EntityId, feet: Vec3, yaw: f32) -> bool {
        let Some(e) = self.state.entities.get_mut(vid) else { return false };
        let (Some(body), Some(v)) = (e.body, e.vehicle.as_mut()) else { return false };
        if !v.state.kinematic {
            return false;
        }
        let center = feet + Vec3::Y * v.base;
        v.yaw = yaw;
        match self.state.physics.bodies.get_mut(body) {
            Some(b) => {
                b.set_next_kinematic_position(Pose::from_parts(center, Quat::from_rotation_y(yaw)));
                true
            }
            None => false,
        }
    }

    fn vehicle_mut(&mut self, vid: EntityId) -> Option<&mut Vehicle> {
        self.state.entities.get_mut(vid).and_then(|e| e.vehicle.as_deref_mut())
    }

    /// Swings door `door` toward `open` (0 shut .. 1 open) over a third of a second.
    pub fn set_vehicle_door(&mut self, vid: EntityId, door: usize, open: f32) -> bool {
        match self.vehicle_mut(vid).and_then(|v| v.state.doors.get_mut(door)) {
            Some(d) => {
                d.target = open.clamp(0.0, 1.0);
                true
            }
            None => false,
        }
    }

    /// Headlights on or off.
    pub fn set_vehicle_lights(&mut self, vid: EntityId, on: bool) -> bool {
        self.vehicle_mut(vid).map(|v| v.state.lights_on = on).is_some()
    }

    /// Starts or stops the engine (a wrecked or drowned one stays off).
    pub fn set_vehicle_engine(&mut self, vid: EntityId, on: bool) -> bool {
        self.vehicle_mut(vid).map(|v| v.state.engine_on = on && !v.state.wrecked && !v.state.flooded).is_some()
    }

    /// Holds an input on a vehicle until cleared (`None`), for tools and tests: used on ticks
    /// the game gives the vehicle none.
    pub fn set_vehicle_input(&mut self, vid: EntityId, input: Option<VehicleInput>) -> bool {
        let found = self.vehicle_mut(vid).map(|v| v.state.hold = input).is_some();
        if found && input.is_some() {
            if let Some(b) = self.state.entities.get(vid).and_then(|e| e.body).and_then(|h| self.state.physics.bodies.get_mut(h))
            {
                b.wake_up(true);
            }
        }
        found
    }

    /// A vehicle's state for a HUD and the audio.
    pub fn vehicle_status(&self, vid: EntityId) -> Option<VehicleStatus> {
        self.state.entities.get(vid)?.vehicle.as_ref().map(|v| v.status())
    }

    /// The water surface's height where a vehicle's centre is, if it is over a water zone.
    fn water_surface(&self, p: Vec3) -> Option<f32> {
        self.state
            .statics
            .zones_at(p)
            .filter(|(_, z)| z.kind == crate::zones::ZoneKind::Water)
            .map(|(_, z)| z.max.y)
            .reduce(f32::max)
    }

    /// Drives every vehicle one tick (before the physics step): from the game's `given` inputs
    /// first, then a tool's held input, else the player's car from the raw input (`legacy`: the
    /// engine demo's steer-toward-the-stick), else idle.
    pub(crate) fn drive_vehicles(
        &mut self,
        input: &InputFrame,
        given: &std::collections::BTreeMap<EntityId, VehicleInput>,
        legacy: bool,
        dt: f32,
    ) {
        let ids: Vec<EntityId> = self.state.entities.iter().filter(|e| e.vehicle.is_some()).map(|e| e.id).collect();
        let player = self.state.player;
        let gravity = self.config.gravity;
        for id in ids {
            // Water under the body's bottom (the surface may be above or below its centre).
            let water = self.state.entities.get(id).and_then(|e| {
                let below = e.vehicle.as_ref().map(|v| v.def.size.y + 0.05).unwrap_or(0.5);
                self.water_surface(e.pos - Vec3::Y * below)
            });
            let Some(e) = self.state.entities.map.get_mut(&id) else { continue };
            let (Some(body), Some(mut v)) = (e.body, e.vehicle.take()) else { continue };
            for d in &mut v.state.doors {
                d.open += (d.target - d.open).clamp(-3.0 * dt, 3.0 * dt);
            }
            v.state.since_crash += dt;
            let explicit = given.get(&id).copied().or(v.state.hold);
            let by_player = v.driver.is_some() && v.driver == player && legacy;
            let ph = &mut self.state.physics;
            match v.def.kind {
                VehicleKind::Car if v.state.kinematic => car::coast_kinematic(ph, body, &mut v, dt),
                VehicleKind::Car => {
                    let speed = ph.bodies.get(body).map(|b| b.linvel().dot(*b.rotation() * Vec3::Z)).unwrap_or(0.0);
                    let fwd = ph.bodies.get(body).map(|b| *b.rotation() * Vec3::Z).unwrap_or(Vec3::Z);
                    let vi = match explicit {
                        Some(vi) => Some(vi),
                        None if by_player => Some(legacy_input(input, fwd, speed, v.def.steer)),
                        None => None,
                    };
                    car::tick(ph, body, &mut v, vi, water, gravity, dt);
                }
                VehicleKind::Helicopter => {
                    let legacy_inp = if by_player && explicit.is_none() { Some(input) } else { None };
                    heli::tick(ph, body, &mut v, explicit, legacy_inp, dt);
                }
            }
            if let Some(e) = self.state.entities.map.get_mut(&id) {
                e.vehicle = Some(v);
            }
        }
    }
}

/// The engine demo's controls as a vehicle input: steer toward the stick's direction, pull back
/// to brake and then reverse; jump is the handbrake, crouch brakes.
fn legacy_input(inp: &InputFrame, fwd: Vec3, speed: f32, steer_deg: f32) -> VehicleInput {
    let wish = Vec2::new(inp.move_dir.x, inp.move_dir.y);
    let mut vi = VehicleInput { handbrake: inp.down(buttons::JUMP), ..Default::default() };
    if wish.length() > 0.1 {
        let lock = steer_deg.to_radians().max(0.1);
        let push = wish.length().min(1.0);
        let yaw = fwd.x.atan2(fwd.z);
        let diff = wrap(wish.x.atan2(wish.y) - yaw);
        if diff.abs() < 1.9 || speed > 3.0 {
            if diff.abs() < 1.9 {
                vi.throttle = push;
            } else {
                vi.brake = 0.4 * push;
            }
            // A positive difference is to the left (toward +yaw).
            vi.steer = -(diff / lock).clamp(-1.0, 1.0);
        } else {
            vi.brake = 0.6 * push;
            vi.steer = (wrap(diff - std::f32::consts::PI) / lock).clamp(-1.0, 1.0);
        }
    }
    if inp.down(buttons::CROUCH) {
        vi.brake = 1.0;
    }
    vi
}
