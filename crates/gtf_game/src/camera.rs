//! The game's camera, decided in the simulation (deterministic, visible to captures and bots):
//! a third-person orbit over the player's right shoulder, turned by the right stick or the
//! mouse, that walls pull in, that swings back behind a moving player, widens when sprinting and
//! closes in over the shoulder to aim; and drone shots for the title screen. Builds the
//! `CameraIntent` the view draws (PLAN.md §5.6.2).

use glam::{Vec2, Vec3};
use pav_core::InputFrame;
use pav_core::input::pad;
use pav_core::present::CameraIntent;
use serde::{Deserialize, Serialize};

/// On foot: 3.8 m behind and 1.5 m above the shoulders, 0.45 m to the right of them.
pub const FOOT_DISTANCE: f32 = 3.8;
pub const FOOT_ELEVATION: f32 = 0.376;
pub const FOOT_SHOULDER: f32 = 0.45;
pub const FOOT_FOV: f32 = 55.0;
/// Aiming (L2): close over the shoulder at head height, narrower.
pub const AIM_DISTANCE: f32 = 2.0;
pub const AIM_SHOULDER: f32 = 0.65;
pub const AIM_FOV: f32 = 45.0;
/// How far above the feet the camera's target sits on foot and aiming (m).
pub const FOOT_LIFT: f32 = 0.65;
pub const AIM_LIFT: f32 = 1.2;
/// Sprinting widens the view by this much (degrees).
pub const SPRINT_FOV: f32 = 5.0;
/// Seconds without look input before the camera swings back behind a moving player.
pub const RECENTRE_AFTER: f32 = 2.0;
/// The closest walls may pull the camera in, and the gap it keeps from them (m).
const MIN_REACH: f32 = 0.5;
const SKIN: f32 = 0.25;

/// Third person on foot: behind and above the player's shoulders.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OrbitCam {
    /// Heading (radians): 0 looks north (-Z), positive turns toward the east (+X).
    pub yaw: f32,
    /// Height of the eye above the target (radians above the horizon).
    pub elevation: f32,
    /// The distance it holds (easing between on foot and aiming) and how far walls let it be.
    pub distance: f32,
    pub reach: f32,
    pub fov: f32,
    /// Metres to the right of the player (negative: the left shoulder), and which shoulder the
    /// player chose (R3 swaps it while aiming).
    pub shoulder: f32,
    pub side: f32,
    /// How far above the feet the target sits (easing between on foot and aiming).
    pub lift: f32,
    /// The smoothed point it looks at.
    pub target: Vec3,
    /// Seconds since the player last turned the camera.
    pub idle: f32,
    /// False until the first update (the first one snaps to the player).
    placed: bool,
}

impl Default for OrbitCam {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            elevation: FOOT_ELEVATION,
            distance: FOOT_DISTANCE,
            reach: FOOT_DISTANCE,
            fov: FOOT_FOV,
            shoulder: FOOT_SHOULDER,
            side: 1.0,
            lift: FOOT_LIFT,
            target: Vec3::ZERO,
            idle: 0.0,
            placed: false,
        }
    }
}

/// How the player turns the camera (from the game's settings).
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub sensitivity: f32,
    pub invert_y: bool,
    /// Swing back behind the player's movement after a while without look input.
    pub recentre: bool,
}

impl Default for Look {
    fn default() -> Self {
        Look { sensitivity: 1.0, invert_y: false, recentre: true }
    }
}

/// What the player is doing, as the camera needs it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Moving {
    /// The direction the player moves on the ground (x, z; length up to 1), or zero.
    pub dir: Vec2,
    pub sprinting: bool,
    pub aiming: bool,
}

/// Wraps an angle into -π..π.
fn wrap(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
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

    /// The point the camera orbits: the target, moved out to the shoulder.
    fn pivot(&self) -> Vec3 {
        self.target + self.axes().1 * self.shoulder
    }

    /// From the pivot toward the eye (unit).
    fn back(&self) -> Vec3 {
        let (f, _) = self.axes();
        let (s, c) = self.elevation.sin_cos();
        (Vec3::Y * s - f * c).normalize()
    }

    /// Turns with the right stick (`dt` real seconds) and the mouse, follows `focus`, and keeps
    /// out of walls: `hit(origin, dir, max)` is the distance to the first surface along a ray.
    pub fn update(
        &mut self,
        input: &InputFrame,
        focus: Vec3,
        look: Look,
        moving: Moving,
        dt: f32,
        hit: &dyn Fn(Vec3, Vec3, f32) -> Option<f32>,
    ) {
        let inv = if look.invert_y { -1.0 } else { 1.0 };
        let r = input.pad.stick_r;
        let m = input.mouse_delta;
        // A stick turns at up to 150°/s, faster near full tilt (aim curve).
        let curve = |v: f32| v.signum() * v.abs().powf(1.6);
        self.yaw += curve(r.x) * 2.6 * look.sensitivity * dt;
        self.elevation -= curve(r.y) * 1.7 * look.sensitivity * dt * inv;
        self.yaw += m.x * 0.0024 * look.sensitivity;
        self.elevation += m.y * 0.0019 * look.sensitivity * inv;
        let turned = r.length() > 0.12 || m != Vec2::ZERO;
        self.idle = if turned { 0.0 } else { self.idle + dt };
        // Back behind a moving player once the player has left the camera alone for a while.
        if look.recentre && self.idle > RECENTRE_AFTER && moving.dir.length() > 0.5 && !moving.aiming {
            let heading = moving.dir.x.atan2(-moving.dir.y);
            let k = 1.0 - (-1.6 * dt).exp();
            self.yaw += wrap(heading - self.yaw) * k;
            self.elevation += (FOOT_ELEVATION - self.elevation) * k;
        }
        self.yaw = wrap(self.yaw);
        self.elevation = self.elevation.clamp(-0.15, 1.25);
        // R3 swaps the shoulder while aiming.
        if moving.aiming && input.pad.pressed & pad::R3 != 0 {
            self.side = -self.side;
        }
        let (distance, shoulder, lift, fov) = if moving.aiming {
            (AIM_DISTANCE, AIM_SHOULDER, AIM_LIFT, AIM_FOV)
        } else {
            (FOOT_DISTANCE, FOOT_SHOULDER, FOOT_LIFT, FOOT_FOV + if moving.sprinting { SPRINT_FOV } else { 0.0 })
        };
        let k = 1.0 - (-10.0 * dt).exp();
        self.distance += (distance - self.distance) * k;
        self.shoulder += (shoulder * self.side - self.shoulder) * k;
        self.lift += (lift - self.lift) * k;
        self.fov += (fov - self.fov) * (1.0 - (-6.0 * dt).exp());
        let aim = focus + Vec3::Y * self.lift;
        if !self.placed {
            self.target = aim;
            self.reach = self.distance;
            self.placed = true;
        } else {
            self.target += (aim - self.target) * (1.0 - (-14.0 * dt).exp());
        }
        // Walls: rays from the pivot toward the eye (the middle and four around it, a stand-in
        // for a sphere cast) pull the camera in at once; it eases back out when they clear.
        let pivot = self.pivot();
        let back = self.back();
        let (_, right) = self.axes();
        let up = right.cross(back).normalize_or(Vec3::Y);
        let mut free = self.distance;
        for off in [Vec3::ZERO, right * 0.18, -right * 0.18, up * 0.18, -up * 0.18] {
            if let Some(d) = hit(pivot + off, back, self.distance + SKIN) {
                free = free.min(d - SKIN);
            }
        }
        let free = free.clamp(MIN_REACH.min(self.distance), self.distance);
        self.reach = if free < self.reach { free } else { self.reach + (free - self.reach) * (1.0 - (-3.0 * dt).exp()) };
    }

    pub fn intent(&self) -> CameraIntent {
        let pivot = self.pivot();
        let eye = pivot + self.back() * self.reach;
        // Look a little over the shoulder line rather than at the head.
        CameraIntent {
            eye: Vec3::new(eye.x, eye.y.max(self.target.y - 0.9), eye.z),
            target: pivot + Vec3::Y * 0.35,
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

    const DT: f32 = 1.0 / 60.0;

    fn open(_: Vec3, _: Vec3, _: f32) -> Option<f32> {
        None
    }

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
    fn the_eye_sits_behind_above_and_right_of_the_player() {
        let mut cam = OrbitCam::default();
        let feet = Vec3::new(3.0, 0.0, 4.0);
        cam.update(&InputFrame::default(), feet, Look::default(), Moving::default(), DT, &open);
        let c = cam.intent();
        assert!(c.eye.z > feet.z + 3.0, "behind (south of) a north-facing player: {c:?}");
        assert!(c.eye.y > feet.y + 1.5, "above");
        assert!(c.eye.x > feet.x + 0.3, "over the right shoulder");
    }

    #[test]
    fn a_wall_pulls_the_camera_in_and_it_eases_back_out() {
        let mut cam = OrbitCam::default();
        let feet = Vec3::ZERO;
        // A wall 1.5 m behind the pivot.
        let wall = |_: Vec3, _: Vec3, _: f32| Some(1.5);
        for _ in 0..30 {
            cam.update(&InputFrame::default(), feet, Look::default(), Moving::default(), DT, &wall);
        }
        assert!((cam.reach - 1.25).abs() < 0.01, "pulled in to the wall less its skin: {}", cam.reach);
        cam.update(&InputFrame::default(), feet, Look::default(), Moving::default(), DT, &open);
        assert!(cam.reach < 1.5, "eases out, does not jump: {}", cam.reach);
        for _ in 0..240 {
            cam.update(&InputFrame::default(), feet, Look::default(), Moving::default(), DT, &open);
        }
        assert!((cam.reach - FOOT_DISTANCE).abs() < 0.05, "back out: {}", cam.reach);
    }

    #[test]
    fn it_swings_behind_a_moving_player_after_two_seconds() {
        let mut cam = OrbitCam::default();
        // The player runs east while the camera looks north.
        let moving = Moving { dir: Vec2::X, ..Default::default() };
        for _ in 0..(RECENTRE_AFTER / DT) as usize - 10 {
            cam.update(&InputFrame::default(), Vec3::ZERO, Look::default(), moving, DT, &open);
        }
        assert!(cam.yaw.abs() < 1e-3, "holds for two seconds: {}", cam.yaw);
        for _ in 0..240 {
            cam.update(&InputFrame::default(), Vec3::ZERO, Look::default(), moving, DT, &open);
        }
        assert!((cam.yaw - std::f32::consts::FRAC_PI_2).abs() < 0.05, "behind the run: {}", cam.yaw);
        // Not when the setting is off.
        let mut still = OrbitCam::default();
        for _ in 0..400 {
            still.update(&InputFrame::default(), Vec3::ZERO, Look { recentre: false, ..Default::default() }, moving, DT, &open);
        }
        assert!(still.yaw.abs() < 1e-3);
    }

    #[test]
    fn aiming_closes_in_and_r3_swaps_the_shoulder() {
        let mut cam = OrbitCam::default();
        let aiming = Moving { aiming: true, ..Default::default() };
        for _ in 0..60 {
            cam.update(&InputFrame::default(), Vec3::ZERO, Look::default(), aiming, DT, &open);
        }
        assert!((cam.distance - AIM_DISTANCE).abs() < 0.05 && (cam.fov - AIM_FOV).abs() < 0.5, "{cam:?}");
        let mut r3 = InputFrame::default();
        r3.pad.pressed = pad::R3;
        cam.update(&r3, Vec3::ZERO, Look::default(), aiming, DT, &open);
        for _ in 0..60 {
            cam.update(&InputFrame::default(), Vec3::ZERO, Look::default(), aiming, DT, &open);
        }
        assert!((cam.shoulder + AIM_SHOULDER).abs() < 0.05, "left shoulder: {}", cam.shoulder);
        // Sprinting widens the view.
        let mut run = OrbitCam::default();
        for _ in 0..120 {
            run.update(
                &InputFrame::default(),
                Vec3::ZERO,
                Look::default(),
                Moving { sprinting: true, ..Default::default() },
                DT,
                &open,
            );
        }
        assert!((run.fov - FOOT_FOV - SPRINT_FOV).abs() < 0.2, "{}", run.fov);
    }
}
