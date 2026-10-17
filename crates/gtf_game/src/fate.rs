//! Wasted and Busted (PLAN.md §5.6.5). At no health the world slows to 0.3×, the picture drains
//! to grey and WASTED holds over a slow orbit of the player; then the player wakes outside
//! St. Brigid Medical, $250 lighter. Caught at 1–2 stars (an officer at arm's length of a player
//! on foot who is barely moving, for two seconds): BUSTED, and the player walks out of SPPD
//! Central, fined a tenth of their cash (at most $500) and stripped of all ammunition but a
//! magazine. Either way the heat ends. Explosions (vehicles, grenades) hurt everyone near them
//! and kill whoever sits in the vehicle that blew up.

use glam::Vec3;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::sim::Sim;
use serde::{Deserialize, Serialize};

/// Seconds the banner holds before the player respawns (real time).
pub const LENGTH: f32 = 5.0;
/// The world's speed while it holds.
pub const SLOW: f32 = 0.3;
pub const WASTED_FEE: i64 = 250;
/// Seconds of arm's-length contact that make an arrest.
pub const CUFF_TIME: f32 = 2.0;

/// Whether the player is alive and free.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Fate {
    #[default]
    Alive,
    /// Seconds since it happened.
    Wasted {
        t: f32,
    },
    Busted {
        t: f32,
    },
}

impl Fate {
    pub fn alive(&self) -> bool {
        *self == Fate::Alive
    }

    /// Seconds since it happened (0 while alive).
    pub fn since(&self) -> f32 {
        match *self {
            Fate::Alive => 0.0,
            Fate::Wasted { t } | Fate::Busted { t } => t,
        }
    }
}

/// The busted fine: a tenth of the cash, at most $500.
pub fn busted_fee(money: i64) -> i64 {
    (money / 10).clamp(0, 500)
}

/// What an explosion did: (who, damage, direction to push them), and whether it was the
/// player's vehicle (the player dies).
#[derive(Clone, Debug, Default)]
pub struct Blast {
    pub hurt: Vec<(EntityId, f32, Vec3)>,
    pub player_dies: bool,
    pub player_hurt: f32,
}

/// This tick's explosions: everyone on foot within the radius takes up to 250 (people) or 180
/// (the player) by distance; riders of the vehicle at the blast's centre die.
pub fn blasts(sim: &Sim, events: &[SimEvent], player: Option<EntityId>) -> Blast {
    let mut out = Blast::default();
    for ev in events {
        let SimEvent::Explosion { pos, radius } = ev else { continue };
        let r = radius.max(1.0);
        for e in sim.state.entities.iter() {
            let Some(ch) = e.character.as_ref() else { continue };
            if e.name.starts_with("dead:") {
                continue;
            }
            // Riders: those in the vehicle that blew up die; others ride it out.
            if let Some(v) = ch.riding {
                let at_centre = sim.state.entities.get(v).is_some_and(|ve| ve.pos.distance(*pos) < 1.5);
                if at_centre {
                    if Some(e.id) == player {
                        out.player_dies = true;
                    } else {
                        out.hurt.push((e.id, 1000.0, Vec3::Y));
                    }
                }
                continue;
            }
            let d = e.pos.distance(*pos);
            if d > r * 1.2 {
                continue;
            }
            let k = (1.0 - d / (r * 1.2)).clamp(0.0, 1.0);
            let dir = (e.pos - *pos).normalize_or(Vec3::Y);
            if Some(e.id) == player {
                out.player_hurt += 180.0 * k;
            } else {
                out.hurt.push((e.id, 250.0 * k, dir));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fines() {
        assert_eq!(busted_fee(12_480), 500);
        assert_eq!(busted_fee(1_200), 120);
        assert_eq!(busted_fee(-5), 0);
        assert!(Fate::Alive.alive() && !Fate::Wasted { t: 0.0 }.alive());
    }
}
