//! A helicopter's tick: an arcade flyer. The rotor spins up while someone flies it (a game's
//! input counts); then it hovers, climbs and descends (throttle / brake, or the demo's jump and
//! crouch), turns with steer and flies along its heading with pitch (the demo: toward the
//! stick), tilting into its motion. Wrecked or drowned, the rotor stops and it falls.

use glam::{Quat, Vec2, Vec3};
use rapier::prelude::*;

use super::{Vehicle, VehicleInput, wrap};
use crate::input::{InputFrame, buttons};
use crate::physics::PhysicsState;

pub(super) fn tick(
    ph: &mut PhysicsState,
    body: RigidBodyHandle,
    v: &mut Vehicle,
    explicit: Option<VehicleInput>,
    legacy: Option<&InputFrame>,
    dt: f32,
) {
    let Some(b) = ph.bodies.get_mut(body) else { return };
    let s = &mut v.state;
    if let Some(vi) = explicit {
        s.horn = vi.horn;
        s.siren_on = vi.siren && v.def.siren;
        if !s.wrecked && !s.flooded {
            s.engine_on = true;
        }
    }
    let crewed = v.driver.is_some() || explicit.is_some();
    let target_rotor = if crewed && !s.wrecked && !s.flooded { 1.0 } else { 0.0 };
    v.rotor_speed += (target_rotor - v.rotor_speed) * (1.0 - (-1.5 * dt).exp());
    let flying = v.rotor_speed > 0.7;
    b.set_gravity_scale(if flying { 0.0 } else { 1.0 }, true);
    let idle = InputFrame::default();
    let inp = legacy.unwrap_or(&idle);
    if flying {
        let vel = b.linvel();
        let wish = match explicit {
            // Turn with steer, fly along the heading with pitch.
            Some(vi) => {
                v.yaw -= vi.steer.clamp(-1.0, 1.0) * 1.6 * dt;
                Vec3::new(v.yaw.sin(), 0.0, v.yaw.cos()) * vi.pitch.clamp(-1.0, 1.0)
            }
            None => Vec3::new(inp.move_dir.x, 0.0, inp.move_dir.y),
        };
        let wish = wish.clamp_length_max(1.0) * v.def.max_speed;
        let dvh = (wish - Vec3::new(vel.x, 0.0, vel.z)).clamp_length_max(v.def.power * dt);
        // Climbing eases off toward the ceiling.
        let room = v.ground + v.def.ceiling - (b.translation().y - v.base);
        let up = match explicit {
            Some(vi) => (vi.throttle - vi.brake).clamp(-1.0, 1.0),
            None => inp.down(buttons::JUMP) as i32 as f32 - inp.down(buttons::CROUCH) as i32 as f32,
        };
        let climb = if up > 0.0 { v.def.climb * room.clamp(0.0, 1.0) * up } else { v.def.climb * up };
        let vy = vel.y + (climb - vel.y) * (1.0 - (-4.0 * dt).exp());
        let nv = Vec3::new(vel.x + dvh.x, vy, vel.z + dvh.z);
        b.set_linvel(nv, true);
        // Lean into the motion, turn toward where it is going.
        let h = Vec2::new(nv.x, nv.z);
        if h.length() > 1.0 && explicit.is_none() {
            let want = h.x.atan2(h.y);
            v.yaw += wrap(want - v.yaw) * (1.0 - (-2.5 * dt).exp());
        }
        let accel = dvh / dt.max(1e-4) / v.def.power.max(0.1);
        let (sn, cs) = v.yaw.sin_cos();
        let local = Vec2::new(accel.x * cs - accel.z * sn, accel.x * sn + accel.z * cs);
        let speed_tilt = h.length() / v.def.max_speed.max(0.1);
        let target = Vec2::new(local.y * 0.25 + speed_tilt * 0.18, -local.x * 0.25);
        v.tilt += (target - v.tilt) * (1.0 - (-5.0 * dt).exp());
        s.speed = h.length();
    } else {
        v.tilt *= (-3.0 * dt).exp();
        s.speed = 0.0;
    }
    let rot = Quat::from_rotation_y(v.yaw) * Quat::from_rotation_x(v.tilt.x) * Quat::from_rotation_z(v.tilt.y);
    b.set_rotation(rot, true);
    v.rotor = (v.rotor + v.rotor_speed * 28.0 * dt).rem_euclid(std::f32::consts::TAU);
    s.rpm = v.rotor_speed;
    s.gear = if flying { 1 } else { 0 };
}
