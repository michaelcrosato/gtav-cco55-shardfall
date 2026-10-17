//! Feel checks for a vehicle definition, each on a test track built for it in an empty world: a
//! flat slab 4 km long and 400 m wide (acceleration, top speed, braking, skidpad, handbrake
//! slide), a 15% grade 400 m long, and a kicker like the Proving Grounds' (12 m long, 2.2 m
//! high). A driver script holds the pedals; every number comes from the fixed-tick simulation,
//! so a run repeats exactly. Games compare the numbers with their targets
//! (`gtf vehicle_feel`, `crates/gtf_game/tests/vehicles.rs`).

use glam::{Quat, Vec3};
use serde::Serialize;

use super::{VehicleDef, VehicleInput};
use crate::color::Color;
use crate::entity::EntityId;
use crate::input::InputFrame;
use crate::shape::{Look, Shape};
use crate::sim::Sim;
use crate::statics::{Block, Decor};

/// The checks [`run`] can make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Test {
    /// From rest at full throttle: the time to [`accel_target`], and the top speed.
    Accel,
    /// Full brakes from 100 km/h (or 95% of the top speed): the distance to a stop.
    Brake,
    /// Full steering at 25 m/s (or 85% of the top speed) holding the speed: lateral g.
    Skidpad,
    /// Handbrake and full steering at 20 m/s (or 80% of the top speed): the slide angle.
    Slide,
    /// From rest up a 15% grade: the speed it holds near the top.
    Climb,
    /// Over the kicker at 20 m/s (or 70% of the top speed): the flight and the landing.
    Jump,
}

impl Test {
    pub const ALL: [Test; 6] = [Test::Accel, Test::Brake, Test::Skidpad, Test::Slide, Test::Climb, Test::Jump];

    pub fn name(self) -> &'static str {
        match self {
            Test::Accel => "accel",
            Test::Brake => "brake",
            Test::Skidpad => "skidpad",
            Test::Slide => "slide",
            Test::Climb => "climb",
            Test::Jump => "jump",
        }
    }

    pub fn parse(name: &str) -> Option<Test> {
        Test::ALL.into_iter().find(|t| t.name() == name)
    }
}

/// The flight over the kicker.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct JumpFeel {
    /// Speed at the ramp (m/s) and seconds in the air.
    pub speed: f32,
    pub air_time: f32,
    /// Pitch as it left the ramp, the most nose-up / nose-down and the most roll in the air,
    /// and the pitch as it touched down (degrees; positive nose up).
    pub takeoff_pitch: f32,
    pub max_pitch: f32,
    pub min_pitch: f32,
    pub max_roll: f32,
    pub landing_pitch: f32,
    /// On its wheels and level two seconds after touching down.
    pub upright: bool,
}

/// What [`run`] measured (a test left out stays None).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Feel {
    /// The acceleration target (m/s): 100 km/h, or 80% of the top speed for slower vehicles.
    pub accel_to: f32,
    /// Seconds from rest to `accel_to`, and the top speed reached (m/s).
    pub accel_time: Option<f32>,
    pub top_speed: Option<f32>,
    /// Braking from `brake_from` (m/s): metres to a stop.
    pub brake_from: f32,
    pub brake_distance: Option<f32>,
    /// Steady cornering at `skidpad_speed`: lateral acceleration (g).
    pub skidpad_speed: f32,
    pub grip_g: Option<f32>,
    /// Handbrake turn at `slide_speed`: the largest angle between heading and motion (deg).
    pub slide_speed: f32,
    pub slide_deg: Option<f32>,
    /// After the slide, counter-steering straightened it out within 3 s.
    pub slide_recovered: Option<bool>,
    /// Climbing the 15% grade: the speed near its top (m/s) and as a share of `max_speed`.
    pub climb_speed: Option<f32>,
    pub climb_share: Option<f32>,
    pub jump: Option<JumpFeel>,
    /// Some flat-ground test ended on its side or roof.
    pub rolled_over: bool,
}

/// 100 km/h, or 80% of the top speed for vehicles slower than that.
pub fn accel_target(def: &VehicleDef) -> f32 {
    let kmh100 = 100.0 / 3.6;
    if def.max_speed > kmh100 * 1.02 { kmh100 } else { 0.8 * def.max_speed }
}

/// A grade of 15%.
const GRADE: f32 = 0.15;
const SLOPE_LEN: f32 = 400.0;
const KICKER_LEN: f32 = 12.0;
const KICKER_HIGH: f32 = 2.2;

/// An empty world with the flat slab, plus the grade or the kicker, and the vehicle standing at
/// the origin facing +z.
fn track(def: &VehicleDef, extra: Option<Test>) -> (Sim, EntityId) {
    let mut sim = Sim::empty(1);
    sim.history.enabled = false;
    let st = &mut sim.state;
    st.statics
        .add(&mut st.physics, Block::new(Vec3::new(-200.0, -1.0, -100.0), Vec3::new(200.0, 0.0, 4000.0), Color::hex("#4a4d52")));
    // A slab tilted up along +z whose top rises from the ground at `z0`.
    let mut ramp = |z0: f32, len: f32, angle: f32, width: f32| {
        let thick = 1.0;
        let (s, c) = angle.sin_cos();
        // The top face's back edge, rotated, sits on the ground at z0.
        let back = Vec3::new(0.0, thick * 0.5 * c - len * 0.5 * s, -thick * 0.5 * s - len * 0.5 * c);
        let pos = Vec3::new(0.0, -back.y, z0 - back.z);
        let rot = Quat::from_rotation_x(-angle);
        st.statics.add_decor(
            &mut st.physics,
            crate::statics::RegionKey::chunk_of(pos),
            Decor {
                shape: Shape::Box { half: Vec3::new(width * 0.5, thick * 0.5, len * 0.5) },
                pos,
                rot,
                color: Color::hex("#b8b2a4"),
                look: Look::Cel,
                emissive: 0.0,
                solid: true,
                collider: None,
                sway: Default::default(),
                material: None,
                glow: Default::default(),
            },
        );
    };
    match extra {
        Some(Test::Climb) => ramp(6.0, SLOPE_LEN, GRADE.atan(), 30.0),
        Some(Test::Jump) => ramp(150.0, KICKER_LEN, (KICKER_HIGH / KICKER_LEN).atan(), 6.0),
        _ => {}
    }
    let id = sim.spawn_vehicle("feel", def.clone(), Vec3::ZERO, Quat::IDENTITY, None);
    sim.run(30, &InputFrame::default());
    (sim, id)
}

struct Probe {
    pos: Vec3,
    vel: Vec3,
    rot: Quat,
    wheels_down: u8,
}

fn probe(sim: &Sim, id: EntityId) -> Probe {
    let e = sim.state.entities.get(id).expect("the feel vehicle exists");
    let vel = e.body.and_then(|b| sim.state.physics.bodies.get(b)).map(|b| b.linvel()).unwrap_or_default();
    let wheels_down = e.vehicle.as_ref().map(|v| v.state.wheels_down).unwrap_or(0);
    Probe { pos: e.pos, vel, rot: e.rot, wheels_down }
}

fn drive(sim: &mut Sim, id: EntityId, vi: VehicleInput) -> Probe {
    sim.set_vehicle_input(id, Some(vi));
    sim.step(&InputFrame::default());
    sim.drain_events();
    probe(sim, id)
}

/// Pedals that hold `target` speed (m/s) going forward.
fn cruise(speed: f32, target: f32) -> VehicleInput {
    let err = target - speed;
    if err < -1.0 {
        VehicleInput { brake: (-err * 0.3).min(1.0), ..Default::default() }
    } else {
        VehicleInput { throttle: (0.35 + err * 0.4).clamp(0.0, 1.0), ..Default::default() }
    }
}

fn pitch_deg(rot: Quat) -> f32 {
    (rot * Vec3::Z).y.clamp(-1.0, 1.0).asin().to_degrees()
}

fn roll_deg(rot: Quat) -> f32 {
    (rot * Vec3::X).y.clamp(-1.0, 1.0).asin().to_degrees()
}

/// Gets up to `target` speed going straight; false if it never does within `seconds`.
fn reach(sim: &mut Sim, id: EntityId, target: f32, seconds: f32, low: &mut f32) -> bool {
    let ticks = (seconds / sim.dt()) as usize;
    for _ in 0..ticks {
        let p = drive(sim, id, VehicleInput { throttle: 1.0, ..Default::default() });
        *low = low.min((p.rot * Vec3::Y).y);
        if p.vel.dot(p.rot * Vec3::Z) >= target {
            return true;
        }
    }
    false
}

/// Runs `tests` on `def`.
pub fn run(def: &VehicleDef, tests: &[Test]) -> Feel {
    let mut f = Feel {
        accel_to: accel_target(def),
        brake_from: (100.0 / 3.6f32).min(0.95 * def.max_speed),
        skidpad_speed: 25.0f32.min(0.85 * def.max_speed),
        slide_speed: 20.0f32.min(0.8 * def.max_speed),
        ..Default::default()
    };
    let mut low = 1.0f32;
    for &t in tests {
        match t {
            Test::Accel => accel(def, &mut f, &mut low),
            Test::Brake => brake(def, &mut f, &mut low),
            Test::Skidpad => skidpad(def, &mut f, &mut low),
            Test::Slide => slide(def, &mut f, &mut low),
            Test::Climb => climb(def, &mut f),
            Test::Jump => f.jump = Some(jump(def)),
        }
    }
    f.rolled_over = low < 0.3;
    f
}

fn accel(def: &VehicleDef, f: &mut Feel, low: &mut f32) {
    let (mut sim, id) = track(def, None);
    let dt = sim.dt();
    let (mut t, mut top, mut best_at) = (0.0f32, 0.0f32, 0.0f32);
    while t < 90.0 {
        let p = drive(&mut sim, id, VehicleInput { throttle: 1.0, ..Default::default() });
        t += dt;
        *low = low.min((p.rot * Vec3::Y).y);
        let v = p.vel.length();
        if f.accel_time.is_none() && v >= f.accel_to {
            f.accel_time = Some(t);
        }
        if v > top + 0.02 {
            top = v;
            best_at = t;
        }
        // Done once it has gained nothing for three seconds.
        if t - best_at > 3.0 {
            break;
        }
    }
    f.top_speed = Some(top);
}

fn brake(def: &VehicleDef, f: &mut Feel, low: &mut f32) {
    let (mut sim, id) = track(def, None);
    if !reach(&mut sim, id, f.brake_from, 90.0, low) {
        return;
    }
    let start = probe(&sim, id).pos;
    for _ in 0..(30.0 / sim.dt()) as usize {
        let p = drive(&mut sim, id, VehicleInput { brake: 1.0, ..Default::default() });
        *low = low.min((p.rot * Vec3::Y).y);
        if p.vel.dot(p.rot * Vec3::Z) < 0.3 {
            f.brake_distance = Some((p.pos - start).length());
            return;
        }
    }
}

/// Steers in slowly from straight to full lock over 4 s holding the speed, then holds it: the
/// most lateral acceleration sustained for half a second (g).
fn skidpad(def: &VehicleDef, f: &mut Feel, low: &mut f32) {
    let (mut sim, id) = track(def, None);
    let target = f.skidpad_speed;
    if !reach(&mut sim, id, target, 90.0, low) {
        return;
    }
    let dt = sim.dt();
    let mut prev = probe(&sim, id).vel;
    let mut window: std::collections::VecDeque<f32> = Default::default();
    let mut best = 0.0f32;
    let per = (0.5 / dt) as usize;
    for i in 0..(6.0 / dt) as usize {
        let ramp = (i as f32 * dt / 4.0).min(1.0);
        let p0 = probe(&sim, id);
        let speed = p0.vel.length();
        let vi = VehicleInput { steer: ramp, ..cruise(speed, target) };
        let p = drive(&mut sim, id, vi);
        *low = low.min((p.rot * Vec3::Y).y);
        // Lateral acceleration: the change of velocity across the motion, level.
        let accel = (p.vel - prev) / dt;
        prev = p.vel;
        let flat = p.vel.with_y(0.0).normalize_or_zero();
        let side = Vec3::Y.cross(flat);
        window.push_back(accel.dot(side).abs());
        if window.len() > per {
            window.pop_front();
        }
        if window.len() == per && speed > 0.8 * target {
            best = best.max(window.iter().sum::<f32>() / per as f32);
        }
    }
    f.grip_g = Some(best / sim.config.gravity);
}

/// The angle between where the vehicle points and where it goes (degrees, level).
fn slide_angle(p: &Probe) -> f32 {
    let flat = p.vel.with_y(0.0);
    if flat.length() < 3.0 {
        return 0.0;
    }
    let heading = (p.rot * Vec3::Z).with_y(0.0).normalize_or_zero();
    heading.angle_between(flat.normalize()).to_degrees()
}

/// Handbrake and full lock for 0.7 s: the largest slide angle while held. Then the handbrake is
/// let go and a simple driver counter-steers: `slide_recovered` when it runs straight again.
fn slide(def: &VehicleDef, f: &mut Feel, low: &mut f32) {
    let (mut sim, id) = track(def, None);
    let target = f.slide_speed;
    if !reach(&mut sim, id, target, 90.0, low) {
        return;
    }
    let dt = sim.dt();
    let mut best = 0.0f32;
    let hold = (0.7 / dt) as usize;
    for _ in 0..hold {
        let p = drive(&mut sim, id, VehicleInput { steer: 1.0, handbrake: true, ..Default::default() });
        *low = low.min((p.rot * Vec3::Y).y);
        best = best.max(slide_angle(&p));
    }
    f.slide_deg = Some(best);
    let mut recovered = false;
    for _ in 0..(3.0 / dt) as usize {
        let p0 = probe(&sim, id);
        // Steer toward where it is going (counter-steer), proportionally.
        let heading = (p0.rot * Vec3::Z).with_y(0.0).normalize_or_zero();
        let flat = p0.vel.with_y(0.0).normalize_or_zero();
        let side = heading.cross(flat).y; // > 0: moving to the heading's left
        let steer = (-side * 2.0).clamp(-1.0, 1.0);
        let p = drive(&mut sim, id, VehicleInput { steer, ..Default::default() });
        *low = low.min((p.rot * Vec3::Y).y);
        if slide_angle(&p) < 8.0 {
            recovered = true;
            break;
        }
    }
    f.slide_recovered = Some(recovered);
}

fn climb(def: &VehicleDef, f: &mut Feel) {
    let (mut sim, id) = track(def, Some(Test::Climb));
    let dt = sim.dt();
    let run = SLOPE_LEN * (GRADE.atan()).cos();
    for _ in 0..(150.0 / dt) as usize {
        let p = drive(&mut sim, id, VehicleInput { throttle: 1.0, ..Default::default() });
        // Near the top: the speed it holds on the grade.
        if p.pos.z >= 6.0 + run * 0.95 {
            let v = p.vel.length();
            f.climb_speed = Some(v);
            f.climb_share = Some(v / def.max_speed.max(0.1));
            return;
        }
    }
    let v = probe(&sim, id).vel.length();
    f.climb_speed = Some(v);
    f.climb_share = Some(v / def.max_speed.max(0.1));
}

fn jump(def: &VehicleDef) -> JumpFeel {
    let (mut sim, id) = track(def, Some(Test::Jump));
    let dt = sim.dt();
    let target = 20.0f32.min(0.7 * def.max_speed);
    let mut j = JumpFeel { max_pitch: -90.0, min_pitch: 90.0, ..Default::default() };
    let (mut flying, mut landed_at) = (false, None::<f32>);
    let mut t = 0.0f32;
    let mut ramp_seen = false;
    while t < 40.0 {
        let p0 = probe(&sim, id);
        let speed = p0.vel.dot(p0.rot * Vec3::Z);
        // Hold the speed up to the ramp, then let go.
        let vi = if p0.pos.z < 150.0 { cruise(speed, target) } else { VehicleInput::default() };
        let p = drive(&mut sim, id, vi);
        t += dt;
        let (pitch, roll) = (pitch_deg(p.rot), roll_deg(p.rot));
        if p.pos.z > 140.0 && !ramp_seen {
            ramp_seen = true;
            j.speed = p.vel.length();
        }
        if ramp_seen && landed_at.is_none() {
            if !flying && p.wheels_down == 0 && p.pos.z > 150.0 {
                flying = true;
                j.takeoff_pitch = pitch;
            }
            if flying {
                j.air_time += dt;
                j.max_pitch = j.max_pitch.max(pitch);
                j.min_pitch = j.min_pitch.min(pitch);
                j.max_roll = j.max_roll.max(roll.abs());
                if p.wheels_down >= 2 && j.air_time > 0.1 {
                    landed_at = Some(t);
                    j.landing_pitch = pitch;
                }
            }
        }
        if let Some(at) = landed_at {
            if t - at >= 2.0 {
                j.upright = (p.rot * Vec3::Y).y > 0.9 && p.wheels_down >= 3;
                return j;
            }
        }
    }
    j
}
