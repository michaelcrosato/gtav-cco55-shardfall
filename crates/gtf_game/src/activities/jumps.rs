//! Stunt jumps (PLAN.md §5.6.11): airborne through a jump's gate above its speed, a slow-motion
//! camera; down inside its landing zone, "UNIQUE STUNT BONUS". To be built.

use pav_core::frame::SimEvent;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use super::ActivitiesDef;
use crate::Freeway;

/// A stunt jump (`[[jump]]` in `activities.toml`).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct JumpDef {
    pub id: String,
}

/// The jumps under way.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct JumpState {}

impl Freeway {
    pub(crate) fn jumps_tick(&mut self, _sim: &mut Sim, _d: &ActivitiesDef, _dt: f32, _real: f32, _events: &mut Vec<SimEvent>) {}
}
