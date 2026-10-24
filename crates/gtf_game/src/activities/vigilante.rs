//! Vigilante (PLAN.md §5.6.11): in a police car, D-pad right (or the phone's Jobs app) starts
//! it; take down fleeing suspects, level by level. To be built.

use pav_core::InputFrame;
use pav_core::frame::SimEvent;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

use super::ActivitiesDef;
use crate::Freeway;

/// `[vigilante]` in `activities.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct VigilanteDef {}

/// A vigilante run under way.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VigilanteState {}

impl Freeway {
    pub(crate) fn vigilante_tick(
        &mut self,
        _sim: &mut Sim,
        _d: &ActivitiesDef,
        _input: &InputFrame,
        _dt: f32,
        _events: &mut Vec<SimEvent>,
    ) {
    }
}
