//! Grand Theft Freeway-BX — the game's rules and content runtime on the Pavilion engine's game
//! hook (`pav_core::games`). Simulation only: no graphics, window or audio code (PLAN.md §6.2).
//! The binaries in `gtf_app` make it known to the engine with [`register`].

use anyhow::{Result, anyhow};
use pav_core::frame::SimEvent;
use pav_core::games::{Entry, Game, GameBox, entry};
use pav_core::room::RoomDef;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub mod content;

/// The game's scene key: `load scene=freeway` (the frontend), `freeway/roam` (straight into
/// free roam), `freeway/dev` (the Proving Grounds), `freeway/attract`.
pub const KEY: &str = "freeway";

/// The game's line in the engine's game list.
pub static ENTRY: Entry = entry::<Freeway>(
    "Grand Theft Freeway-BX: an open-world crime caper in Paloma Bay (freeway/roam, freeway/dev, freeway/attract).",
    build,
);

/// Makes the game known to this process: call it before building a scene or loading a snapshot.
pub fn register() {
    pav_core::games::register(&ENTRY);
}

/// The game's state (everything that changes during play lives here or in entities).
#[derive(Clone, Serialize, Deserialize)]
pub struct Freeway {
    /// The sub-scene this run was built for (`roam`, `dev`, `attract`; empty = the frontend).
    pub mode: String,
    /// Seconds of game time since the scene was built.
    pub time: f32,
}

/// Builds a scene of the game. M0: a placeholder lot; the generated district arrives in M2.
pub fn build(sim: &mut Sim, rest: Option<&str>) -> Result<()> {
    let mode = rest.unwrap_or("").to_string();
    let text = content::text("m0_lot.toml", include_str!("../../../content/freeway/m0_lot.toml"));
    let def = RoomDef::parse(&text).map_err(|e| anyhow!("content/freeway/m0_lot.toml: {e}"))?;
    pav_core::scenes::build_standalone_room(sim, KEY, def);
    sim.state.play = Some(GameBox::new(Freeway { mode, time: 0.0 }));
    Ok(())
}

impl Game for Freeway {
    const KEY: &'static str = KEY;

    fn post(&mut self, _sim: &mut Sim, dt: f32, _events: &mut Vec<SimEvent>) {
        self.time += dt;
    }

    fn hud(&self, _sim: &Sim) -> Vec<String> {
        let mode = if self.mode.is_empty() { "frontend" } else { self.mode.as_str() };
        vec!["Grand Theft Freeway-BX".into(), format!("M0 bootstrap ({mode})")]
    }

    fn status(&self, sim: &Sim) -> serde_json::Value {
        let player = sim.state.player.and_then(|id| sim.feet(id)).map(|p| [round(p.x), round(p.z)]);
        json!({ "mode": self.mode, "time": round(self.time), "player": player })
    }
}

fn round(x: f32) -> f64 {
    (x as f64 * 100.0).round() / 100.0
}
