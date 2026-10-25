//! Activities (PLAN.md §5.6.11). The Pretzel Run and the two hold-ups are missions marked
//! `activity = true` (`content/freeway/missions/`). The stunt jumps, the Lost Shields,
//! vigilante and the random events run here, from `content/freeway/activities.toml`. Each one
//! finished is kept by id in `done` (`jump:<id>`, `shield:<id>`, `vigilante`, `event:<id>`),
//! saved with the game, and counts toward completion (`Freeway::completion`).

pub mod events;
pub mod jumps;
pub mod shields;
pub mod vigilante;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use pav_core::InputFrame;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::present::{DrawList, UiModel};
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use crate::Freeway;

/// `activities.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivitiesDef {
    #[serde(default)]
    pub jump: Vec<jumps::JumpDef>,
    #[serde(default)]
    pub shield: Vec<shields::ShieldDef>,
    #[serde(default)]
    pub vigilante: Option<vigilante::VigilanteDef>,
    #[serde(default)]
    pub event: Vec<events::EventDef>,
}

impl ActivitiesDef {
    /// The ids completion counts here (activity missions are counted with the missions).
    pub fn units(&self) -> Vec<String> {
        let mut out: Vec<String> = self.jump.iter().map(|j| format!("jump:{}", j.id)).collect();
        out.extend(self.vigilante.iter().map(|_| "vigilante".to_string()));
        out.extend(self.event.iter().map(|e| format!("event:{}", e.id)));
        out
    }

    /// The collectibles' ids.
    pub fn shields(&self) -> Vec<String> {
        self.shield.iter().map(|s| format!("shield:{}", s.id)).collect()
    }
}

/// The activities file (re-read when it changes on disk).
pub fn def() -> Result<Arc<ActivitiesDef>> {
    static CACHE: Mutex<Option<(String, Arc<ActivitiesDef>)>> = Mutex::new(None);
    let text = crate::content::text("activities.toml", include_str!("../../../../content/freeway/activities.toml"));
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, d)) = cache.as_ref()
        && *t == *text
    {
        return Ok(d.clone());
    }
    let d: ActivitiesDef = toml::from_str(&text).map_err(|e| anyhow!("content/freeway/activities.toml: {e}"))?;
    let d = Arc::new(d);
    *cache = Some((text.into_owned(), d.clone()));
    Ok(d)
}

/// The activities' state (part of the game's state).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Activities {
    /// Finished (kept in the save).
    pub done: BTreeSet<String>,
    pub jumps: jumps::JumpState,
    pub shields: shields::ShieldState,
    pub vigilante: vigilante::VigilanteState,
    pub events: events::EventState,
}

impl Freeway {
    /// Whether the activities run: in Paloma Bay, not in a mission (a mission's world is its
    /// own) nor the Proving Grounds (the places are Paloma Bay's).
    fn activities_on(&self) -> bool {
        self.missions.run.is_none() && self.mode != "dev"
    }

    /// The activities' tick, while playing. The moves of the people they drive go into `out`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn activities_tick(
        &mut self,
        sim: &mut Sim,
        input: &InputFrame,
        dt: f32,
        real: f32,
        events: &mut Vec<SimEvent>,
        out: &mut BTreeMap<EntityId, InputFrame>,
    ) {
        if !self.activities_on() {
            // (Vigilante and a random event end when a mission begins.)
            self.street_end(sim);
            return;
        }
        let Ok(d) = def() else { return };
        self.jumps_tick(sim, &d, dt, real, events);
        self.shields_tick(sim, &d, dt, events);
        self.vigilante_tick(sim, &d, input, dt, events, out);
        self.events_tick(sim, &d, input, dt, events, out);
    }

    /// What the activities show: the stunt banner, the shields.
    pub(crate) fn activities_ui(&self, sim: &Sim, ui: &mut UiModel, draw: &mut DrawList) {
        if !self.activities_on() {
            return;
        }
        self.jumps_ui(ui);
        self.shields_draw(sim, draw);
    }

    /// An activity finished (the first time it is new for completion).
    pub(crate) fn activity_done(&mut self, id: &str) {
        self.activities.done.insert(id.to_string());
    }
}
