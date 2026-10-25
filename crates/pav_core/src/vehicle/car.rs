//! A car's tick. Rapier's raycast vehicle springs the four wheels (its own friction turned off);
//! then each wheel on the ground turns its load into tyre forces: a slip-angle curve sideways
//! (peak grip at 8°, a little less sliding beyond), the engine (with traction control), brakes
//! and rolling resistance along it, all inside a friction ellipse; rear wheels locked by the
//! handbrake slide against their slip at `drift_grip`. Forces act nearer the centre of mass's
//! height than the contact patches, so a car leans and dips without tipping over. Anti-roll
//! bars, aerodynamics, damped rotation and light control in the air, righting after three
//! seconds on its side or roof, and floating then sinking in water complete it.

use glam::{Vec2, Vec3};
use rapier::prelude::*;

use super::{Vehicle, VehicleDef, VehicleInput};
use crate::physics::PhysicsState;

/// Longitudinal grip as a multiple of the sideways grip.
const LONG_GRIP: f32 = 1.4;
/// Traction control: the share of a wheel's grip the engine may use.
const TRACTION: f32 = 0.92;
/// Slip angle of peak sideways grip (radians), and the grip left when sliding far past it.
const ALPHA_PEAK: f32 = 0.14;
const SLIDE: f32 = 0.85;
/// Below this rolling speed slip angles blend into a static hold (no creeping when parked).
const V_LOW: f32 = 3.0;
/// How much of the contact patch's depth below the centre of mass sideways and lengthwise
/// forces keep as a lever (less roll and pitch than real tyres give, so cars stay upright).
const ROLL_INFLUENCE: f32 = 0.6;
const PITCH_INFLUENCE: f32 = 0.4;
/// Rolling resistance coefficient, engine braking off the throttle (share of the weight) and
/// the reverse gear's share of the force.
const ROLLING: f32 = 0.012;
const COAST: f32 = 0.04;
const REVERSE: f32 = 0.5;
/// Share of the brake force at the front axle.
const BRAKE_FRONT: f32 = 0.6;
/// A burst tyre: grip and radius left (on the rim).
pub(super) const BURST_GRIP: f32 = 0.4;
const BURST_RADIUS: f32 = 0.78;
/// In the air: pitch and roll damping (1/s), control (rad/s² at full stick) and how firmly it
/// eases toward level roll and toward its flight path in pitch.
const AIR_DAMP: f32 = 1.2;
const AIR_PITCH: f32 = 2.5;
const AIR_ROLL: f32 = 2.0;
const AIR_LEVEL: f32 = 1.5;
/// Seconds on its side or roof before it rights itself, and how fast it turns back.
pub(super) const RIGHT_AFTER: f32 = 3.0;
const RIGHT_SPEED: f32 = 3.5;
/// Gear changes as shares of top speed.
const SHIFT: [f32; 6] = [0.0, 0.17, 0.34, 0.52, 0.72, 1.0];

fn smooth01(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Steering lock (radians) at `speed`: full below 8 m/s, `steer_at_top` of it at top speed.
pub(super) fn steer_lock(def: &VehicleDef, speed: f32) -> f32 {
    let t = ((speed.abs() - 8.0) / (def.max_speed - 8.0).max(1.0)).clamp(0.0, 1.0);
    def.steer.to_radians() * (1.0 + (def.steer_at_top - 1.0) * t)
}

/// The engine's push (N, all driven wheels together) at forward speed `v` for demand `drive`
/// (-1 reverse .. 1).
pub(super) fn engine_force(def: &VehicleDef, v: f32, drive: f32) -> f32 {
    let n = def.driven().iter().filter(|d| **d).count() as f32;
    let full = def.power * n;
    if drive > 0.0 {
        let u = v.max(0.0) / def.max_speed.max(0.1);
        let curve = if u > def.knee { def.knee / u } else { 1.0 };
        let fade = ((1.0 - u) / 0.08).clamp(0.0, 1.0);
        drive * full * curve * fade
    } else if drive < 0.0 {
        let top = (def.max_speed * 0.3).min(10.0);
        let fade = ((top - (-v).max(0.0)) / 1.5).clamp(0.0, 1.0);
        drive * full * REVERSE * fade
    } else {
        0.0
    }
}

/// The gear (-1 reverse, 0 neutral) and engine speed (0..1) for the sounds and the HUD.
fn gearbox(def: &VehicleDef, speed: f32, throttle: f32) -> (i8, f32) {
    let u = (speed.abs() / def.max_speed.max(0.1)).min(1.0);
    if speed < -0.3 || throttle < -0.05 {
        return (-1, (0.3 + u / 0.3 * 0.6).min(1.0));
    }
    if speed.abs() < 0.3 && throttle.abs() < 0.05 {
        return (0, 0.12);
    }
    let g = (1..SHIFT.len()).find(|&g| u < SHIFT[g]).unwrap_or(SHIFT.len() - 1);
    let (lo, hi) = (SHIFT[g - 1], SHIFT[g]);
    (g as i8, 0.3 + 0.7 * ((u - lo) / (hi - lo)).clamp(0.0, 1.0))
}

/// One tick of a car on its wheels. `vi` is its driver's input (None: nobody drives; parked
/// when nobody sits in it), `water` the surface height of water it is over.
pub(super) fn tick(
    ph: &mut PhysicsState,
    body: RigidBodyHandle,
    v: &mut Vehicle,
    vi: Option<VehicleInput>,
    water: Option<f32>,
    gravity: f32,
    dt: f32,
) {
    let Some(b) = ph.bodies.get(body) else { return };
    let fwd0 = *b.rotation() * Vec3::Z;
    let v_fwd = b.linvel().dot(fwd0);
    let asleep = b.is_sleeping();
    let parked = vi.is_none() && v.driver.is_none();
    // Pedals: the brake reverses once (nearly) stopped; the throttle brakes while rolling back.
    let (mut drive, mut brake, mut steer, mut pitch, mut handbrake) = (0.0, 0.0, 0.0, 0.0, false);
    let s = &mut v.state;
    match vi {
        Some(vi) => {
            let (t, br) = (vi.throttle.clamp(0.0, 1.0), vi.brake.clamp(0.0, 1.0));
            drive = t;
            if br > 0.0 {
                if v_fwd > 1.0 {
                    brake = br;
                } else {
                    drive = -br;
                }
            }
            if t > 0.0 && v_fwd < -1.0 {
                brake = brake.max(t);
                drive = 0.0;
            }
            steer = vi.steer.clamp(-1.0, 1.0);
            pitch = vi.pitch.clamp(-1.0, 1.0);
            handbrake = vi.handbrake;
            s.horn = vi.horn;
            s.siren_on = vi.siren && v.def.siren;
            if (t > 0.0 || br > 0.0) && !s.wrecked && !s.flooded {
                s.engine_on = true;
            }
        }
        None => s.horn = false,
    }
    if !s.engine_on || s.wrecked || s.flooded {
        drive = 0.0;
    }
    v.throttle += (drive - v.throttle) * (1.0 - (-10.0 * dt).exp());
    if drive == 0.0 && v.throttle.abs() < 0.01 {
        v.throttle = 0.0;
    }
    v.steer += (steer - v.steer) * (1.0 - (-12.0 * dt).exp());
    s.braking = brake > 0.05 || (handbrake && v_fwd.abs() > 0.5);
    s.reversing = v.throttle < -0.05 && v_fwd < 0.5;
    let (gear, rpm) = gearbox(&v.def, v_fwd, v.throttle);
    s.gear = gear;
    s.rpm = rpm;
    s.speed = v_fwd;
    // A parked car that has gone to sleep stays asleep until something touches it.
    if asleep && vi.is_none() {
        s.slip = 0.0;
        return;
    }
    let Some(car) = v.car.as_mut() else { return };

    // The springs (rapier's raycast vehicle, its friction off).
    let lock = steer_lock(&v.def, v_fwd);
    for (i, w) in car.wheels_mut().iter_mut().enumerate() {
        w.steering = if i < 2 { (-v.steer * lock) as Real } else { 0.0 };
        w.engine_force = 0.0;
        w.brake = 0.0;
        w.side_friction_stiffness = 0.0;
        let burst = v.state.burst.get(i).copied().unwrap_or(false);
        w.radius = (v.def.wheels.radius * if burst { BURST_RADIUS } else { 1.0 }) as Real;
    }
    {
        let filter = QueryFilter::default().exclude_rigid_body(body).exclude_sensors();
        let qpm =
            ph.broad_phase.as_query_pipeline_mut(ph.narrow_phase.query_dispatcher(), &mut ph.bodies, &mut ph.colliders, filter);
        car.update_vehicle(dt as Real, qpm);
    }

    // The tyres.
    let Some(b) = ph.bodies.get(body) else { return };
    let (rot, com, mass, lin) = (*b.rotation(), b.center_of_mass(), b.mass(), b.linvel());
    let (fwd, up, left) = (rot * Vec3::Z, rot * Vec3::Y, rot * Vec3::X);
    let wheels = car.wheels();
    let down = wheels.iter().filter(|w| w.raycast_info().is_in_contact).count();
    let share = 0.8 * mass / down.max(1) as f32;
    let driven = v.def.driven();
    let n_driven = driven.iter().filter(|d| **d).count().max(1) as f32;
    let engine = engine_force(&v.def, v_fwd, v.throttle);
    let coast = if v.throttle.abs() < 0.05 && v.state.engine_on { COAST * mass * gravity / n_driven } else { 0.0 };
    // Water under the tyres takes grip away; the driver's input may add some (an ability).
    let traction = (1.0 - v.state.water.clamp(0.0, 0.9)) * (1.0 + vi.map_or(0.0, |vi| vi.grip.clamp(0.0, 2.0)));
    let mut kicks: Vec<(Vec3, Vec3)> = Vec::with_capacity(12);
    let mut slip = 0.0f32;
    for (i, w) in wheels.iter().enumerate() {
        let info = w.raycast_info();
        let load = (w.wheel_suspension_force as f32).clamp(0.0, w.max_suspension_force as f32);
        if !info.is_in_contact || load <= 1e-3 {
            continue;
        }
        let (n, p) = (info.contact_normal_ws, info.contact_point_ws);
        let axle = w.axle();
        let a = (axle - n * axle.dot(n)).normalize_or_zero();
        let f = n.cross(a).normalize_or_zero();
        let vel = b.velocity_at_point(p);
        let (vl, vs) = (vel.dot(f), vel.dot(a));
        let k = traction * if v.state.burst.get(i).copied().unwrap_or(false) { BURST_GRIP } else { 1.0 };
        let mu_s = v.def.grip * k;
        let mu_l = mu_s * LONG_GRIP;
        let (fl, fs) = if (handbrake && i >= 2) || parked {
            // Locked: the tyre slides against its slip (a parked car holds like a brake).
            let mu = if parked { mu_l } else { v.def.drift_grip * k };
            let slide = Vec2::new(vl, vs);
            let force = (-slide * share / dt).clamp_length_max(mu * load);
            if slide.length() > 0.5 {
                slip = slip.max(slide.length());
            }
            (force.x, force.y)
        } else {
            let cap = TRACTION * mu_l * load;
            let push = if driven[i] { (engine / n_driven).clamp(-cap, cap) } else { 0.0 };
            let mut resist = ROLLING * load;
            if brake > 0.0 {
                let axle_share = if i < 2 { BRAKE_FRONT } else { 1.0 - BRAKE_FRONT };
                resist += v.def.brake * brake * axle_share * 0.5;
            }
            if driven[i] {
                resist += coast;
            }
            let mut fl = push + (-vl * share / dt).clamp(-resist, resist);
            let alpha = vs.atan2(vl.abs().max(V_LOW));
            let x = alpha.abs() / ALPHA_PEAK;
            let curve = if x <= 1.0 { x } else { 1.0 - (1.0 - SLIDE) * smooth01((x - 1.0) / 2.0) };
            let slipping = -alpha.signum() * mu_s * load * curve;
            let hold = (-vs * share / dt).clamp(-mu_s * load, mu_s * load);
            let mut fs = hold + (slipping - hold) * (vl.abs() / V_LOW).min(1.0);
            let r = ((fl / (mu_l * load)).powi(2) + (fs / (mu_s * load)).powi(2)).sqrt();
            if r > 1.0 {
                fl /= r;
                fs /= r;
            }
            if x > 1.0 {
                slip = slip.max(vs.abs());
            }
            (fl, fs)
        };
        let h = up.dot(p - com);
        kicks.push((a * (fs * dt), p - up * (h * (1.0 - ROLL_INFLUENCE))));
        kicks.push((f * (fl * dt), p - up * (h * (1.0 - PITCH_INFLUENCE))));
    }
    // Anti-roll bars: the more compressed side is held up, the other pulled down.
    let bar = v.def.suspension.anti_roll * v.def.suspension.stiffness * mass;
    for (l, r) in [(0usize, 1usize), (2, 3)] {
        let (Some(wl), Some(wr)) = (wheels.get(l), wheels.get(r)) else { continue };
        let (il, ir) = (wl.raycast_info(), wr.raycast_info());
        if !(il.is_in_contact && ir.is_in_contact) {
            continue;
        }
        let squeeze = (wl.suspension_rest_length - il.suspension_length) - (wr.suspension_rest_length - ir.suspension_length);
        let force = bar * squeeze as f32;
        kicks.push((il.contact_normal_ws * (force * dt), il.contact_point_ws));
        kicks.push((-ir.contact_normal_ws * (force * dt), ir.contact_point_ws));
    }
    // Air: drag along the motion, downforce with speed.
    let aero = -lin * (lin.length() * v.def.drag) - up * (v.def.downforce * v_fwd * v_fwd);

    let Some(b) = ph.bodies.get_mut(body) else { return };
    if vi.is_some_and(|vi| vi.throttle > 0.0 || vi.brake > 0.0 || vi.steer != 0.0 || vi.handbrake) {
        b.wake_up(true);
    }
    for (j, p) in kicks {
        b.apply_impulse_at_point(j, p, false);
    }
    b.apply_impulse(aero * dt, false);

    let s = &mut v.state;
    s.wheels_down = down as u8;
    s.slip = slip;
    s.air_time = if down == 0 { s.air_time + dt } else { 0.0 };
    let lat = b.linvel().dot(left);
    v.drift += (lat.abs() - v.drift) * (1.0 - (-8.0 * dt).exp());

    // In the air: damped pitch and roll, light control, easing toward level and the flight path.
    if down == 0 && s.air_time > 0.05 && !s.righting {
        let local = rot.inverse() * b.angvel();
        let damp = (-AIR_DAMP * dt).exp();
        let mut l = Vec3::new(local.x * damp, local.y, local.z * damp);
        let roll = left.y.clamp(-1.0, 1.0).asin();
        let nose = fwd.y.clamp(-1.0, 1.0).asin();
        let vel = b.linvel();
        let path = (vel.y / vel.length().max(1.0)).clamp(-1.0, 1.0).asin().clamp(-0.35, 0.35);
        l.x += (pitch * AIR_PITCH + (nose - path) * AIR_LEVEL) * dt;
        l.z += (steer * AIR_ROLL - roll * AIR_LEVEL) * dt;
        b.set_angvel(rot * l, true);
    }

    // On its side or roof for three seconds (not burning): it turns back onto its wheels. (Wheels
    // touching something on its side are on a wall or a car it leans on: they count for nothing.)
    let speed = b.linvel().length();
    if s.righting {
        if up.y > 0.85 || s.wrecked {
            s.righting = false;
            s.upside = 0.0;
            let w = b.angvel() * 0.2;
            b.set_angvel(w, true);
        } else {
            let axis = up.cross(Vec3::Y).normalize_or(fwd);
            b.set_angvel(axis * RIGHT_SPEED, true);
        }
    } else if up.y < 0.35 && speed < 3.0 {
        s.upside += dt;
        if s.upside >= RIGHT_AFTER && !s.burning && !s.wrecked {
            s.righting = true;
            let mut vel = b.linvel();
            vel.y = vel.y.max(3.0);
            b.set_linvel(vel, true);
        }
    } else if up.y > 0.6 {
        s.upside = 0.0;
    }

    // Water: it floats for three seconds, then sinks; the engine drowns.
    match water {
        Some(surface) => {
            let centre = b.translation();
            let hy = v.def.size.y.max(0.1);
            let sub = ((surface - (centre.y - hy)) / (2.0 * hy)).clamp(0.0, 1.0);
            s.water = sub;
            if sub > 0.0 {
                if sub > 0.35 {
                    s.flood_time += dt;
                }
                if s.flood_time > 0.5 && !s.flooded {
                    s.flooded = true;
                    s.engine_on = false;
                }
                let float = if s.flood_time < 3.0 { 1.3 } else { (1.3 - (s.flood_time - 3.0) * 0.45).max(0.3) };
                let lift = Vec3::Y * (mass * gravity * sub * float * dt);
                // Buoyancy acts at the body's middle, above the centre of mass: it floats upright.
                b.apply_impulse_at_point(lift, centre, true);
                // Water drags hardest up and down.
                let v0 = b.linvel();
                let flat = Vec3::new(v0.x, 0.0, v0.z) * (-sub * dt).exp();
                let lv = flat + Vec3::Y * (v0.y * (-3.0 * sub * dt).exp());
                let av = b.angvel() * (-2.5 * sub * dt).exp();
                b.set_linvel(lv, true);
                b.set_angvel(av, true);
            }
        }
        None => s.water = 0.0,
    }
}

/// A kinematic car (moved by the game): its wheels roll with the distance it moved.
pub(super) fn coast_kinematic(ph: &mut PhysicsState, body: RigidBodyHandle, v: &mut Vehicle, dt: f32) {
    let Some(b) = ph.bodies.get(body) else { return };
    let (c, rot) = (b.translation(), *b.rotation());
    let moved = v.state.kinematic_from.map(|p| c - p).unwrap_or(Vec3::ZERO);
    v.state.kinematic_from = Some(c);
    let ds = moved.dot(rot * Vec3::Z);
    v.state.speed = ds / dt.max(1e-4);
    v.state.wheels_down = 4;
    let (gear, rpm) = gearbox(&v.def, v.state.speed, if v.state.speed.abs() > 0.3 { 0.3 } else { 0.0 });
    v.state.gear = gear;
    v.state.rpm = rpm;
    if let Some(car) = &mut v.car {
        for w in car.wheels_mut() {
            w.rotation += (ds / w.radius as f32) as Real;
        }
    }
}
