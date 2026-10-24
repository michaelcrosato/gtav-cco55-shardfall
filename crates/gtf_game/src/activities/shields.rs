//! The Lost Shields (PLAN.md §5.6.11): 15 Route 404 shield signs hidden round the district, each
//! with a highway-trivia card. To be built.

use pav_core::frame::SimEvent;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use super::ActivitiesDef;
use crate::Freeway;

/// A shield (`[[shield]]` in `activities.toml`).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ShieldDef {
    pub id: String,
}

/// What the shields are doing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ShieldState {}

impl Freeway {
    pub(crate) fn shields_tick(&mut self, _sim: &mut Sim, _d: &ActivitiesDef, _dt: f32, _events: &mut Vec<SimEvent>) {}
}
