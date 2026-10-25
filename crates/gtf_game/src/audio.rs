//! The vehicles' sounds (PLAN.md §5.12): engine loops by revs and load (each archetype's
//! `sound` family), tyre squeal by slide, sirens, horns, helicopter rotors and car alarms for
//! the vehicles nearest the listener, and crash one-shots by impulse. The host places loops
//! for the listener (distance and pan); this module says what sounds and how hard.

use std::collections::BTreeMap;

use glam::Vec3;
use pav_core::entity::EntityId;
use pav_core::frame::SimEvent;
use pav_core::present::LoopSource;
use pav_core::sim::Sim;
use pav_core::vehicle::VehicleKind;

/// The radio under the engines and the effects (PLAN.md §5.12: −6 dB), and how far the radio
/// and the ambience beds duck (dB): under subtitled dialogue, in menus.
pub const RADIO: f32 = 0.501;
pub const DUCK_DIALOGUE: f32 = 8.0;
pub const DUCK_MENU: f32 = 12.0;

/// The police scanner's voice loop (an id clear of the vehicles' and the beds') and its level
/// (under the radio).
const SCANNER: u64 = 0x5ca7_0000_0001;
const SCANNER_GAIN: f32 = 0.3;

/// The dispatcher's babble while their line is on air (after the squelch, before the closing
/// one).
pub fn scanner_loop(c: &crate::radio::Chatter, def: &crate::radio::RadioDef) -> Option<LoopSource> {
    if c.age < crate::radio::CHATTER_LEAD || c.age >= c.seconds - 0.1 {
        return None;
    }
    let s = &def.scanner;
    Some(LoopSource {
        id: SCANNER,
        cue: "voice.radio".into(),
        pos: None,
        gain: SCANNER_GAIN,
        pitch: if s.pitch > 0.0 { s.pitch } else { 1.0 },
        param: s.energy,
    })
}

/// Vehicles further than this from the listener are not heard (m), and at most this many.
pub const HEAR: f32 = 80.0;
const MAX_HEARD: usize = 8;

/// Crash impulses (N·s): below `LIGHT` nothing, above `HEAVY` the heavy thump and glass.
const LIGHT: f32 = 1500.0;
const HEAVY: f32 = 7000.0;

/// The loops of the vehicles near `ear`: `driving` is the player's (louder), `alarms` the cars
/// whose alarms are sounding.
pub fn vehicle_loops(
    sim: &Sim,
    ear: Vec3,
    driving: Option<EntityId>,
    alarms: &BTreeMap<EntityId, f32>,
    out: &mut Vec<LoopSource>,
) {
    let mut near: Vec<(f32, EntityId)> = sim
        .state
        .entities
        .iter()
        .filter(|e| e.vehicle.is_some())
        .map(|e| (e.pos.distance(ear), e.id))
        .filter(|(d, id)| *d < HEAR || Some(*id) == driving)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, id) in near.into_iter().take(MAX_HEARD) {
        let Some(e) = sim.state.entities.get(id) else { continue };
        let Some(v) = e.vehicle.as_ref() else { continue };
        let s = v.status();
        let mine = Some(id) == driving;
        let base = id.0 as u64 * 16;
        let pos = Some(e.pos);
        let push = |out: &mut Vec<LoopSource>, k: u64, cue: &str, gain: f32, pitch: f32, param: f32| {
            out.push(LoopSource { id: base + k, cue: cue.into(), pos, gain, pitch, param });
        };
        if v.def.kind == VehicleKind::Helicopter {
            if v.rotor_speed > 0.05 {
                push(out, 0, "rotor", if mine { 0.9 } else { 0.7 } * v.rotor_speed.min(1.0), v.rotor_speed.max(0.2), 0.0);
            }
        } else if s.engine_on {
            let family = crate::vehicles::get(&e.name).map(|a| a.sound).unwrap_or_default();
            let cue = if family.is_empty() { "engine.sedan".to_string() } else { family };
            // Idle at 1, the redline near 3.4; the filter opens with the throttle.
            let pitch = 1.0 + s.rpm.clamp(0.0, 1.0) * 2.4;
            let gain = if mine { 0.55 + 0.3 * s.throttle } else { 0.3 + 0.15 * s.throttle };
            push(out, 0, &cue, gain, pitch, s.throttle.clamp(0.0, 1.0));
        }
        if s.slip > 1.5 && s.wheels_down > 0 {
            push(out, 1, "tyre", if mine { 0.8 } else { 0.5 }, 1.0, ((s.slip - 1.5) / 6.0).clamp(0.0, 1.0));
        }
        // (A helicopter's light bar flashes without a wail.)
        if s.siren_on && v.def.kind != VehicleKind::Helicopter {
            push(out, 2, "siren.wail", 0.9, 1.0, 1.0);
        }
        if s.horn {
            push(out, 3, "horn", 0.8, 1.0, 1.0);
        }
        if alarms.contains_key(&id) {
            push(out, 4, "alarm", 0.7, 1.0, 1.0);
        }
    }
}

/// Crash one-shots for this tick's `SimEvent::Crash`es (pushed onto `events`).
pub fn crash_sounds(events: &mut Vec<SimEvent>) {
    let mut add = Vec::new();
    for ev in events.iter() {
        if let SimEvent::Crash { impulse, pos, .. } = ev {
            if *impulse < LIGHT {
                continue;
            }
            let k = (*impulse / HEAVY).clamp(0.3, 1.5);
            let cue = if *impulse >= HEAVY { "impact.heavy" } else { "impact.light" };
            add.push(SimEvent::Sound { cue: cue.into(), pos: Some(*pos), gain: k, pitch: 1.0 });
            if *impulse >= HEAVY * 1.5 {
                add.push(SimEvent::Sound { cue: "glass".into(), pos: Some(*pos), gain: 0.7, pitch: 1.0 });
            }
        }
    }
    events.extend(add);
}
