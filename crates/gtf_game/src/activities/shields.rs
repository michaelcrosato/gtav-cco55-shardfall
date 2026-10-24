//! The Lost Shields (PLAN.md §5.6.11): fifteen Route 404 shield signs hidden round the
//! district (an alley, a roof, under the freeway, the pier's end). Walking or driving up to one
//! takes it and shows its highway-trivia card; all fifteen pay $10,000 and leave a gold Okami
//! Kaze at Dez's place (PLAN.md §5.6.13).

use glam::{Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::frame::SimEvent;
use pav_core::present::{DrawList, PrimDraw};
use pav_core::shape::{Look, Shape};
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use super::ActivitiesDef;
use crate::Freeway;

/// How near one must come to take a shield (m: on foot, in a car), and how far they are drawn.
const TAKE_FOOT: f32 = 2.2;
const TAKE_CAR: f32 = 4.0;
const DRAWN: f32 = 150.0;
/// All of them: the cash.
pub const ALL_BONUS: i64 = 10_000;

/// A shield (`[[shield]]` in `activities.toml`): where its post stands (world metres, the
/// ground or roof it is on) and its card.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShieldDef {
    pub id: String,
    pub at: [f32; 3],
    /// Which way the sign faces (degrees from north).
    #[serde(default)]
    pub yaw: f32,
    pub trivia: String,
}

impl ShieldDef {
    pub fn pos(&self) -> Vec3 {
        Vec3::from(self.at)
    }
}

/// What the shields are doing (the taken ones are in `Activities::done`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ShieldState {
    /// The all-fifteen reward was paid.
    pub rewarded: bool,
}

impl Freeway {
    pub(crate) fn shields_tick(&mut self, sim: &mut Sim, d: &ActivitiesDef, _dt: f32, events: &mut Vec<SimEvent>) {
        let me = Self::feet(sim);
        let reach = if self.drive.vehicle().is_some() { TAKE_CAR } else { TAKE_FOOT };
        let total = d.shield.len();
        for s in &d.shield {
            let id = format!("shield:{}", s.id);
            if self.activities.done.contains(&id) {
                continue;
            }
            let p = s.pos();
            if Vec2::new(p.x - me.x, p.z - me.z).length() < reach && (p.y - me.y).abs() < 3.0 {
                self.activity_done(&id);
                let n = d.shields().iter().filter(|x| self.activities.done.contains(*x)).count();
                self.note("trophy", &format!("Lost Shield {n} of {total}"), &s.trivia);
                events.push(SimEvent::Sound { cue: "ui.pickup".into(), pos: None, gain: 1.0, pitch: 1.0 });
            }
        }
        // All of them: the bonus and the car.
        if total > 0 && !self.activities.shields.rewarded && d.shields().iter().all(|x| self.activities.done.contains(x)) {
            self.activities.shields.rewarded = true;
            self.pay(ALL_BONUS);
            self.text_message(
                "dez",
                "Fifteen shields. I looked up what they're worth: nothing. I had them gold-plate a Kaze instead. It's at my place. - D",
            );
        }
    }

    /// The shields not taken yet near the player: a blue Route 404 shield on a post.
    pub(crate) fn shields_draw(&self, sim: &Sim, draw: &mut DrawList) {
        let Ok(d) = super::def() else { return };
        let me = Self::feet(sim);
        for s in &d.shield {
            let p = s.pos();
            if p.distance(me) > DRAWN || self.activities.done.contains(&format!("shield:{}", s.id)) {
                continue;
            }
            let rot = Quat::from_rotation_y(std::f32::consts::PI - s.yaw.to_radians());
            let glow = 0.35 + 0.15 * (self.ui_time * 2.5).sin();
            let prim = |shape, pos: Vec3, color, emissive| PrimDraw {
                shape,
                pos,
                rot,
                color,
                emissive,
                look: Look::Cel,
                shadow: true,
                glow: Default::default(),
            };
            draw.prims.push(prim(
                Shape::Cylinder { half_height: 0.7, radius: 0.04 },
                p + Vec3::Y * 0.7,
                Color::hex("#8c949b"),
                0.0,
            ));
            draw.prims.push(prim(
                Shape::Box { half: Vec3::new(0.36, 0.38, 0.03) },
                p + Vec3::Y * 1.62,
                Color::hex("#1f4fa8"),
                glow,
            ));
            draw.prims.push(prim(
                Shape::Box { half: Vec3::new(0.38, 0.07, 0.035) },
                p + Vec3::Y * 2.02,
                Color::hex("#c8312b"),
                glow,
            ));
            draw.prims.push(prim(
                Shape::Box { half: Vec3::new(0.22, 0.12, 0.036) },
                p + Vec3::Y * 1.58,
                Color::hex("#f4f1ea"),
                glow,
            ));
        }
    }
}
