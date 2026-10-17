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

/// The chase camera lets go of a right-stick look after this long (s).
pub const CHASE_RETURN: f32 = 1.5;
/// Field of view from standing still to 40 m/s and up (degrees).
pub const CHASE_FOV: [f32; 2] = [60.0, 75.0];

/// What the chase camera follows.
#[derive(Clone, Copy, Debug)]
pub struct Ride {
    /// The body's centre, heading (0 faces +z, π/2 faces +x), velocity (m/s), half size and
    /// sideways slide (m/s).
    pub center: Vec3,
    pub yaw: f32,
    pub vel: Vec3,
    pub half: Vec3,
    pub drift: f32,
    pub heli: bool,
}

impl Ride {
    fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos())
    }

    /// Distance behind and height above the target, from the vehicle's size (§5.6.2: 5.5–9 m
    /// and 1.8–2.6 m; a helicopter sits further back and higher).
    pub fn reach(&self) -> (f32, f32) {
        if self.heli {
            return (11.0, 3.6);
        }
        ((1.9 * self.half.z + 1.6).clamp(5.5, 9.0), (1.15 + 0.9 * self.half.y).clamp(1.8, 2.6))
    }
}

/// Behind a vehicle (§5.6.2): a yaw spring that lags the heading and looks along the velocity,
/// FOV widening with speed, a little roll in drifts, walls pulling it in; the right stick looks
/// around and lets go after 1.5 s, R3 looks behind; holding ○ cuts between cinematic shots
/// (bumper, low tracking, side dolly, aerial chase, roadside tripod) every 3–6 s.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ChaseCam {
    /// The heading it trails (vehicle convention), and the right stick's look (yaw, pitch).
    pub yaw: f32,
    pub look: f32,
    pub pitch: f32,
    pub idle: f32,
    pub fov: f32,
    pub roll: f32,
    /// How far walls let it be, and the smoothed point it looks at.
    pub reach: f32,
    pub target: Vec3,
    pub behind: bool,
    /// Seconds the cinematic button has been held (negative: not held), and the roadside
    /// tripod's spot for the current tripod shot.
    pub cine: f32,
    pub tripod: Option<Vec3>,
    placed: bool,
}

impl Default for ChaseCam {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            look: 0.0,
            pitch: 0.0,
            idle: 0.0,
            fov: CHASE_FOV[0],
            roll: 0.0,
            reach: 6.0,
            target: Vec3::ZERO,
            behind: false,
            cine: -1.0,
            tripod: None,
            placed: false,
        }
    }
}

/// The cinematic shots, in the order they cycle, and how long each holds (s).
pub const SHOTS: [(&str, f32); 5] =
    [("bumper", 3.5), ("low_tracking", 4.0), ("side_dolly", 3.0), ("aerial", 5.0), ("tripod", 6.0)];

impl ChaseCam {
    /// Starts behind a vehicle (getting in): snaps on the next update.
    pub fn place_behind(&mut self, yaw: f32) {
        *self = ChaseCam { yaw, ..Default::default() };
    }

    /// The current cinematic shot and seconds into it, while ○ is held.
    pub fn shot(&self) -> Option<(usize, f32)> {
        if self.cine < 0.0 {
            return None;
        }
        let total: f32 = SHOTS.iter().map(|s| s.1).sum();
        let mut t = self.cine % total;
        for (i, (_, len)) in SHOTS.iter().enumerate() {
            if t < *len {
                return Some((i, t));
            }
            t -= len;
        }
        Some((0, 0.0))
    }

    /// From the target toward the eye (unit).
    fn back(&self, ride: &Ride) -> Vec3 {
        let (dist, height) = ride.reach();
        let yaw = self.yaw + self.look + if self.behind { std::f32::consts::PI } else { 0.0 };
        let elev = (height / dist).atan() + self.pitch;
        let flat = -Vec3::new(yaw.sin(), 0.0, yaw.cos());
        (flat * elev.cos() + Vec3::Y * elev.sin()).normalize()
    }

    /// Follows `ride` (`dt` real seconds): the pad's right stick and the mouse look around, R3
    /// looks behind, ○ held runs the cinematic shots; `hit` is the first surface along a ray.
    pub fn update(&mut self, input: &InputFrame, ride: &Ride, look: Look, dt: f32, hit: &dyn Fn(Vec3, Vec3, f32) -> Option<f32>) {
        let fwd = ride.forward();
        let flat_vel = Vec3::new(ride.vel.x, 0.0, ride.vel.z);
        let speed = flat_vel.length();
        let ahead = ride.vel.dot(fwd);
        // Trail the heading, leaning toward where it is actually going when moving briskly.
        let goal = if ahead > 4.0 {
            let travel = flat_vel.x.atan2(flat_vel.z);
            ride.yaw + wrap(travel - ride.yaw) * 0.6
        } else {
            ride.yaw
        };
        if !self.placed {
            self.yaw = goal;
        }
        self.yaw = wrap(self.yaw + wrap(goal - self.yaw) * (1.0 - (-3.2 * dt).exp()));
        // Look around: the right stick (a helicopter turns with it instead) and the mouse.
        let inv = if look.invert_y { -1.0 } else { 1.0 };
        let r = input.pad.stick_r;
        let m = input.mouse_delta;
        let curve = |v: f32| v.signum() * v.abs().powf(1.6);
        if !ride.heli {
            self.look += curve(r.x) * 2.6 * look.sensitivity * dt;
        }
        self.pitch -= curve(r.y) * 1.2 * look.sensitivity * dt * inv;
        self.look += m.x * 0.0024 * look.sensitivity;
        self.pitch += m.y * 0.0019 * look.sensitivity * inv;
        let turned = (!ride.heli && r.x.abs() > 0.12) || r.y.abs() > 0.12 || m != glam::Vec2::ZERO;
        self.idle = if turned { 0.0 } else { self.idle + dt };
        if self.idle > CHASE_RETURN {
            let k = 1.0 - (-3.0 * dt).exp();
            self.look -= wrap(self.look) * k;
            self.pitch -= self.pitch * k;
        }
        self.look = wrap(self.look);
        self.pitch = self.pitch.clamp(-0.3, 0.8);
        self.behind = input.pad.held & pad::R3 != 0 && input.pad.held & pad::L3 == 0;
        let fov = CHASE_FOV[0] + (CHASE_FOV[1] - CHASE_FOV[0]) * (speed / 40.0).clamp(0.0, 1.0);
        self.fov += (fov - self.fov) * (1.0 - (-2.5 * dt).exp());
        let roll = (-ride.drift * 0.015).clamp(-0.1, 0.1);
        self.roll += (roll - self.roll) * (1.0 - (-4.0 * dt).exp());
        let goal_target = ride.center + Vec3::Y * (ride.half.y * 0.6 + 0.45) + (ride.vel * 0.12).clamp_length_max(4.0);
        if !self.placed {
            self.target = goal_target;
            self.reach = ride.reach().0;
            self.placed = true;
        } else {
            self.target += (goal_target - self.target) * (1.0 - (-12.0 * dt).exp());
        }
        // Walls pull it in at once; it eases back out.
        let (dist, _) = ride.reach();
        let back = self.back(ride);
        let free = hit(self.target, back, dist + 0.3).map_or(dist, |d| (d - 0.3).clamp(1.5, dist));
        self.reach = if free < self.reach { free } else { self.reach + (free - self.reach) * (1.0 - (-2.5 * dt).exp()) };
        // Cinematic: hold ○ (east).
        if input.pad.held & pad::EAST != 0 {
            let before = self.shot().map(|s| s.0);
            self.cine = self.cine.max(0.0) + dt;
            let now = self.shot().map(|s| s.0);
            let far = self.tripod.is_none_or(|p| p.distance(ride.center) > 60.0);
            if now == Some(4) && (before != now || far) {
                // A roadside spot ahead of the car, off to its right.
                let right = Vec3::new(-fwd.z, 0.0, fwd.x);
                let lead = (speed * 2.5).clamp(18.0, 45.0);
                self.tripod = Some(ride.center + fwd * lead + right * (ride.half.x + 5.0) + Vec3::Y * 0.8);
            }
        } else {
            self.cine = -1.0;
            self.tripod = None;
        }
    }

    pub fn intent(&self, ride: &Ride) -> CameraIntent {
        let fwd = ride.forward();
        let right = Vec3::new(-fwd.z, 0.0, fwd.x);
        let h = ride.half;
        let base = CameraIntent {
            eye: self.target + self.back(ride) * self.reach,
            target: self.target + fwd * 1.5,
            up: (Vec3::Y + right * self.roll).normalize(),
            fov: self.fov,
            near: 0.1,
            far: 1500.0,
            cut: false,
            shake: 0.0,
        };
        let Some((shot, t)) = self.shot() else { return base };
        let c = ride.center;
        let (eye, target, fov) = match SHOTS[shot].0 {
            "bumper" => (c + fwd * (h.z + 0.15) + Vec3::Y * 0.1, c + fwd * 20.0 + Vec3::Y * 0.3, 70.0),
            "low_tracking" => (c - fwd * (h.z * 0.7) + right * (h.x + 1.3) - Vec3::Y * (h.y * 0.5), c + fwd * 3.0, 50.0),
            "side_dolly" => (c - right * (h.x + 4.5 + t * 0.4) + Vec3::Y * 0.6, c, 45.0),
            "aerial" => (c - fwd * (14.0 + t) + Vec3::Y * 9.0, c + fwd * 6.0, 55.0),
            _ => (self.tripod.unwrap_or(c + right * 8.0), c, 40.0),
        };
        CameraIntent { eye, target, up: Vec3::Y, fov, cut: t < 0.02, ..base }
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
