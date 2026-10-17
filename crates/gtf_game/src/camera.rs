//! The game's camera, decided in the simulation (deterministic, visible to captures and bots):
//! a third-person orbit around the player turned by the right stick or the mouse, and drone
//! shots for the title screen. Builds the `CameraIntent` the view draws (PLAN.md §5.6.2).

use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::present::CameraIntent;
use serde::{Deserialize, Serialize};

/// Third person on foot: behind and above the player's shoulders.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrbitCam {
    /// Heading (radians): 0 looks north (-Z), positive turns toward the east (+X).
    pub yaw: f32,
    /// Height of the eye above the target (radians above the horizon).
    pub elevation: f32,
    pub distance: f32,
    pub fov: f32,
    /// The smoothed point it looks at.
    pub target: Vec3,
    /// False until the first update (the first one snaps to the player).
    placed: bool,
}

impl Default for OrbitCam {
    fn default() -> Self {
        Self { yaw: 0.0, elevation: 0.32, distance: 5.2, fov: 55.0, target: Vec3::ZERO, placed: false }
    }
}

/// How the player turns the camera (from the game's settings).
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub sensitivity: f32,
    pub invert_y: bool,
}

impl OrbitCam {
    /// Horizontal forward and right of the camera.
    pub fn axes(&self) -> (Vec3, Vec3) {
        let (s, c) = self.yaw.sin_cos();
        (Vec3::new(s, 0.0, -c), Vec3::new(c, 0.0, s))
    }

    /// A stick direction (x right, y forward) as a world direction on the ground (x, z).
    pub fn world_move(&self, stick: Vec2) -> Vec2 {
        let (f, r) = self.axes();
        let w = r * stick.x + f * stick.y;
        Vec2::new(w.x, w.z)
    }

    /// Turns with the right stick (`dt` real seconds) and the mouse, and follows `focus`.
    pub fn update(&mut self, input: &InputFrame, focus: Vec3, look: Look, dt: f32) {
        let inv = if look.invert_y { -1.0 } else { 1.0 };
        let r = input.pad.stick_r;
        // A stick turns at up to 150°/s, faster near full tilt (aim curve).
        let curve = |v: f32| v.signum() * v.abs().powf(1.6);
        self.yaw += curve(r.x) * 2.6 * look.sensitivity * dt;
        self.elevation -= curve(r.y) * 1.7 * look.sensitivity * dt * inv;
        let m = input.mouse_delta;
        self.yaw += m.x * 0.0024 * look.sensitivity;
        self.elevation += m.y * 0.0019 * look.sensitivity * inv;
        self.yaw = (self.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        self.elevation = self.elevation.clamp(-0.15, 1.25);
        let aim = focus + Vec3::Y * 0.65;
        if !self.placed {
            self.target = aim;
            self.placed = true;
        } else {
            self.target += (aim - self.target) * (1.0 - (-14.0 * dt).exp());
        }
    }

    pub fn intent(&self) -> CameraIntent {
        let (f, _) = self.axes();
        let (s, c) = self.elevation.sin_cos();
        let eye = self.target - f * (self.distance * c) + Vec3::Y * (self.distance * s);
        // Look a little over the shoulder line rather than at the head.
        CameraIntent {
            eye: Vec3::new(eye.x, eye.y.max(self.target.y - 0.9), eye.z),
            target: self.target + Vec3::Y * 0.35,
            up: Vec3::Y,
            fov: self.fov,
            near: 0.1,
            far: 1500.0,
            cut: false,
            shake: 0.0,
        }
    }

    /// Faces the camera along a heading behind the player (on spawning).
    pub fn face(&mut self, yaw: f32) {
        self.yaw = yaw;
        self.placed = false;
    }
}

/// A slow drone orbit around `center` (title screen, idle shots): `t` seconds in.
pub fn drone(center: Vec3, t: f32, radius: f32, height: f32) -> CameraIntent {
    let a = t * 0.06 + 0.8;
    let eye = center + Vec3::new(a.sin() * radius, height + (t * 0.21).sin() * 0.6, a.cos() * radius);
    CameraIntent {
        eye,
        target: center + Vec3::Y * (height * 0.3),
        up: Vec3::Y,
        fov: 42.0,
        near: 0.5,
        far: 2500.0,
        cut: false,
        shake: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stick_moves_relative_to_the_camera() {
        let mut cam = OrbitCam::default();
        // Looking north: forward is -Z, right is +X.
        assert!((cam.world_move(Vec2::Y) - Vec2::new(0.0, -1.0)).length() < 1e-5);
        assert!((cam.world_move(Vec2::X) - Vec2::new(1.0, 0.0)).length() < 1e-5);
        cam.yaw = std::f32::consts::FRAC_PI_2;
        // Looking east: forward is +X.
        assert!((cam.world_move(Vec2::Y) - Vec2::new(1.0, 0.0)).length() < 1e-5);
    }

    #[test]
    fn the_eye_sits_behind_and_above_the_target() {
        let mut cam = OrbitCam::default();
        cam.update(&InputFrame::default(), Vec3::new(3.0, 0.0, 4.0), Look { sensitivity: 1.0, invert_y: false }, 1.0 / 60.0);
        let c = cam.intent();
        assert!(c.eye.z > c.target.z, "behind (south of) a north-facing target");
        assert!(c.eye.y > c.target.y, "above");
    }
}
