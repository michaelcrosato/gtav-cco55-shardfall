//! Random events (PLAN.md §5.6.11): the Purse Snatcher and Getaway Needed, met on the street.
//! To be built.

use pav_core::frame::SimEvent;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use super::ActivitiesDef;
use crate::Freeway;

/// A random event (`[[event]]` in `activities.toml`).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct EventDef {
    pub id: String,
}

/// An event under way.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EventState {}

impl Freeway {
    pub(crate) fn events_tick(&mut self, _sim: &mut Sim, _d: &ActivitiesDef, _dt: f32, _events: &mut Vec<SimEvent>) {}
}
