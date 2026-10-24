//! Stunt jumps (PLAN.md §5.6.11): through a jump's gate in a car, fast enough and the right
//! way, then airborne: the world slows and a camera off to the side watches the flight; down
//! inside the landing zone, "UNIQUE STUNT BONUS" and the bonus (the first time counts toward
//! completion). The ramps are the district's `[[jump]]` wedges (or a road's broken end).

use glam::{Vec2, Vec3};
use pav_core::frame::SimEvent;
use pav_core::present::{Banner, BannerKind, CameraIntent, UiModel};
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use super::ActivitiesDef;
use crate::Freeway;

/// How near the gate's point a car must pass (m), and how far off the jump's heading it may
/// be (degrees).
const GATE: f32 = 7.0;
const SKEW: f32 = 35.0;
/// The world's speed while a jump is in the air.
pub const SLOW: f32 = 0.4;
/// Seconds the banner stays.
const BANNER: f32 = 3.5;

/// A stunt jump (`[[jump]]` in `activities.toml`). Places are world metres (x, z).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JumpDef {
    pub id: String,
    pub name: String,
    /// The point a car passes on its way off (the ramp's lip, a road's broken end).
    pub gate: [f32; 2],
    /// The way through (degrees from north, clockwise).
    pub heading: f32,
    /// The least speed through the gate (m/s).
    pub speed: f32,
    /// The landing zone: its middle and radius (m).
    pub land: [f32; 2],
    pub radius: f32,
    /// Cash for landing it.
    #[serde(default)]
    pub reward: i64,
    /// Where a run at it starts (world metres; for bots, the attract mode and tests).
    pub runup: [f32; 3],
}

impl JumpDef {
    pub fn gate(&self) -> Vec2 {
        Vec2::from(self.gate)
    }

    pub fn land(&self) -> Vec2 {
        Vec2::from(self.land)
    }

    /// The heading as a direction on the ground (x, z).
    pub fn dir(&self) -> Vec2 {
        let a = self.heading.to_radians();
        Vec2::new(a.sin(), -a.cos())
    }
}

/// A jump under way: which, seconds since the gate, in the air yet (and for how long), seconds
/// back down, where it first touched down and the height it left the ground at.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Flight {
    pub jump: usize,
    pub t: f32,
    pub air: bool,
    pub aloft: f32,
    pub down: f32,
    pub touch: Option<Vec2>,
    pub lip: f32,
}

/// A run at a jump for the bot (`stunt <id>`: tests, the attract mode): set up at the run-up
/// in an Okami Kaze, then driven flat out at the gate.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Run {
    pub jump: usize,
    /// Seated and pointed at the gate (else waiting for the ground at the run-up).
    pub driving: bool,
    pub launched: bool,
    pub t: f32,
}

/// The jumps' state.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JumpState {
    pub flight: Option<Flight>,
    pub run: Option<Run>,
    /// The last result on screen: title, line, seconds shown (real time).
    pub banner: Option<(String, String, f32)>,
    /// Landed (any jump, any number of times).
    pub landed: u32,
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

/// Where the stunt camera stands for a jump left from `lip` (m up): beside the flight's middle,
/// out by a third of its length (12 to 24 m) and above the lip and the ground there; of the two
/// sides the one it can see the flight's middle from (anything fixed in between pulls it in).
fn stunt_eye(sim: &Sim, j: &JumpDef, lip: f32) -> Vec3 {
    let mid = (j.gate() + j.land()) * 0.5;
    let from = Vec3::new(mid.x, lip + 1.5, mid.y);
    let out = (j.gate().distance(j.land()) * 0.35 + 6.0).clamp(12.0, 24.0);
    let city = crate::world::city().ok();
    let side = |s: f32| {
        let p = mid + Vec2::new(-j.dir().y, j.dir().x) * (s * out);
        let ground = city.as_ref().map_or(lip, |c| c.ground_y(p));
        let to = Vec3::new(p.x, (lip + 3.0).max(ground + 2.0), p.y) - from;
        let len = to.length();
        let clear = sim.raycast_static(from, to / len, len).map_or(len, |h| (h.point - from).length() - 1.0);
        (from + to / len * clear.max(4.0), clear)
    };
    let (left, a) = side(1.0);
    let (right, b) = side(-1.0);
    if b > a + 0.5 { right } else { left }
}

impl Freeway {
    /// `stunt <id>`: a run at that jump for the bot.
    pub(crate) fn stunt_cmd(&mut self, id: &str) {
        match super::def().ok().and_then(|d| d.jump.iter().position(|j| j.id == id)) {
            Some(jump) => self.activities.jumps.run = Some(Run { jump, ..Default::default() }),
            None => log::warn!("stunt: no jump '{id}'"),
        }
    }

    /// The bot's run: to the run-up, into a Kaze facing the gate; done once it has landed and
    /// stopped (or given up).
    fn stunt_run(&mut self, sim: &mut Sim, d: &ActivitiesDef, dt: f32) {
        let Some(mut r) = self.activities.jumps.run.take() else { return };
        let Some(j) = d.jump.get(r.jump) else { return };
        let at = Vec3::from(j.runup);
        let yaw = j.dir().x.atan2(j.dir().y);
        r.t += dt;
        if !r.driving {
            if r.t <= dt * 1.5 {
                self.put_player(sim, at, yaw);
            }
            if crate::world::ground_loaded(sim, at) && r.t > 1.0 {
                if self.drive.vehicle().is_none() {
                    self.car_cmd(sim, "okami_kaze", Some(0), true);
                }
                if let Some(car) = self.drive.vehicle() {
                    sim.place_vehicle(car, at, yaw);
                    r.driving = true;
                    r.t = 0.0;
                }
            }
        } else {
            let stopped = self.drive.vehicle().and_then(|v| crate::drive::Car::read(sim, v)).is_none_or(|c| c.vel.length() < 0.5);
            r.launched |= self.activities.jumps.flight.as_ref().is_some_and(|f| f.air);
            let landed = r.launched && self.activities.jumps.flight.is_none();
            if (landed && stopped && r.t > 2.0) || r.t > 40.0 {
                return;
            }
        }
        self.activities.jumps.run = Some(r);
    }

    /// The run's pedals and wheel: at the gate's lip flat out, then the landing; hands off in
    /// the air; the brakes once down.
    pub(crate) fn stunt_frame(&self, sim: &Sim, f: &mut pav_core::InputFrame) {
        let Some(r) = self.activities.jumps.run.as_ref().filter(|r| r.driving) else { return };
        let (Ok(d), Some(car)) = (super::def(), self.drive.vehicle().and_then(|v| crate::drive::Car::read(sim, v))) else {
            return;
        };
        let Some(j) = d.jump.get(r.jump) else { return };
        let air = self.drive.vehicle().and_then(|v| sim.vehicle_status(v)).is_some_and(|s| s.airborne);
        let at = flat(car.feet());
        let past = (at - j.gate()).dot(j.dir()) > 0.0;
        let aim = if past { j.land() } else { j.gate() + j.dir() * 2.0 };
        let want = (aim - at).normalize_or_zero();
        let fwd = flat(car.forward()).normalize_or_zero();
        if !air {
            f.pad.stick_l.x = ((fwd.x * want.y - fwd.y * want.x) * 2.5).clamp(-1.0, 1.0);
        }
        let launched = r.launched || air;
        if !launched {
            f.pad.r2 = 1.0;
        } else if !air {
            f.pad.l2 = 1.0;
        }
    }

    pub(crate) fn jumps_tick(&mut self, sim: &mut Sim, d: &ActivitiesDef, dt: f32, real: f32, events: &mut Vec<SimEvent>) {
        self.stunt_run(sim, d, dt);
        if let Some(b) = &mut self.activities.jumps.banner {
            b.2 += real;
            if b.2 > BANNER {
                self.activities.jumps.banner = None;
            }
        }
        let Some(car) = self.drive.vehicle().and_then(|v| crate::drive::Car::read(sim, v)) else {
            self.activities.jumps.flight = None;
            return;
        };
        let airborne = self.drive.vehicle().and_then(|v| sim.vehicle_status(v)).is_some_and(|s| s.airborne);
        let at = flat(car.feet());
        let vel = flat(car.vel);
        match self.activities.jumps.flight.take() {
            None => {
                // Through a gate, fast enough and the right way.
                let gate = d.jump.iter().position(|j| {
                    at.distance(j.gate()) < GATE
                        && vel.length() >= j.speed
                        && vel.normalize_or_zero().dot(j.dir()) > SKEW.to_radians().cos()
                });
                if let Some(i) = gate {
                    self.activities.jumps.flight = Some(Flight { jump: i, lip: car.feet().y, ..Default::default() });
                }
            }
            Some(mut f) => {
                let Some(j) = d.jump.get(f.jump) else { return };
                f.t += dt;
                if airborne {
                    f.air = true;
                    f.aloft += dt;
                    f.down = 0.0;
                    f.touch = None;
                } else if !f.air {
                    // Still on the ramp: its lip is the highest it gets.
                    f.lip = f.lip.max(car.feet().y);
                } else {
                    f.down += dt;
                    f.touch.get_or_insert(at);
                }
                if f.air && f.down > 0.3 {
                    // Down (where it touched, and on its wheels) inside the zone or not.
                    let upright = (car.rot * Vec3::Y).y > 0.5;
                    if f.touch.unwrap_or(at).distance(j.land()) < j.radius && upright {
                        let first = !self.activities.done.contains(&format!("jump:{}", j.id));
                        self.activity_done(&format!("jump:{}", j.id));
                        self.activities.jumps.landed += 1;
                        self.pay(j.reward);
                        let line = if first {
                            format!("{} · ${}", j.name, j.reward)
                        } else {
                            format!("{} · again · ${}", j.name, j.reward)
                        };
                        self.activities.jumps.banner = Some(("UNIQUE STUNT BONUS".into(), line, 0.0));
                        events.push(SimEvent::Sound { cue: "mission.pass".into(), pos: None, gain: 0.8, pitch: 1.2 });
                    } else {
                        self.activities.jumps.banner = Some(("STUNT FAILED".into(), j.name.clone(), 0.0));
                    }
                } else if (!f.air && f.t < 2.0) || (f.air && f.t < 12.0) {
                    // (Through the gate but never off the ground, or a flight that never ends:
                    // forgotten.)
                    self.activities.jumps.flight = Some(f);
                }
            }
        }
    }

    /// Slow motion while a jump is in the air.
    pub(crate) fn stunt_slowmo(&self) -> bool {
        self.activities.jumps.flight.as_ref().is_some_and(|f| f.air)
    }

    /// The stunt camera: still, beside the middle of the flight a little above the lip (on the
    /// side it sees the flight from, pulled in from anything in between), on the car; it cuts in
    /// as the car leaves the ground.
    pub(crate) fn stunt_camera(&self, sim: &Sim) -> Option<CameraIntent> {
        let f = self.activities.jumps.flight.as_ref().filter(|f| f.air)?;
        let d = super::def().ok()?;
        let j = d.jump.get(f.jump)?;
        let car = self.drive.vehicle().and_then(|v| crate::drive::Car::read(sim, v))?;
        Some(CameraIntent {
            eye: stunt_eye(sim, j, f.lip),
            target: car.center,
            fov: 50.0,
            cut: f.aloft <= 1.5 / 60.0,
            ..self.cam.intent()
        })
    }

    /// The flight as a cutscene (bars, no HUD), and the result banner.
    pub(crate) fn jumps_ui(&self, ui: &mut UiModel) {
        if let Some(f) = self.activities.jumps.flight.as_ref().filter(|f| f.air) {
            ui.overlay.letterbox = (f.aloft / 0.15).clamp(0.0, 1.0);
            if let Some(h) = &mut ui.hud {
                h.opacity = 0.0;
            }
            ui.help = None;
            ui.prompts.clear();
            ui.markers.clear();
        }
        if let Some((title, line, age)) = &self.activities.jumps.banner {
            let kind = if title.starts_with("UNIQUE") { BannerKind::Stunt } else { BannerKind::Info };
            ui.banners.push(Banner { kind, title: title.clone(), subtitle: Some(line.clone()), age: *age, duration: BANNER });
        }
    }
}
