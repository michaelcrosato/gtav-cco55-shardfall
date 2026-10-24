//! Breakable glass (PLAN.md §5.3: Luxe Motors' showroom, which mission S3 leaves through): the
//! district's breakable panes (`City::panes`) shatter as a vehicle reaches them — a moment before
//! it would touch, so it keeps its speed — and when a shot strikes them. Explosions break any
//! glass on their own (panes are destructible blocks). A broken pane stays broken: the engine
//! keeps a changed chunk's blocks while it sleeps.

use glam::Vec3;
use pav_core::frame::SimEvent;
use pav_core::sim::Sim;
use pav_world::City;

/// How far ahead of a vehicle's front its glass breaks: seconds of travel, and metres on top.
const AHEAD_S: f32 = 0.2;
const AHEAD_M: f32 = 0.6;
/// Slower than this a vehicle only nudges the glass (it holds).
const MIN_SPEED: f32 = 3.0;

fn middle(p: &pav_world::Pane) -> Vec3 {
    (p.min + p.max) * 0.5
}

/// Breaks pane `i` (when anything of it is left) with the crash and the tinkle.
fn smash(sim: &mut Sim, city: &City, i: usize, from: Vec3, events: &mut Vec<SimEvent>) {
    let p = &city.panes[i];
    if sim.break_blocks_in(p.min, p.max, from) == 0 {
        return;
    }
    let at = middle(p);
    events.push(SimEvent::Break { pos: at });
    let pitch = 0.9 + 0.2 * sim.state.rng.f32();
    events.push(SimEvent::Sound { cue: "glass.shatter".into(), pos: Some(at), gain: 1.0, pitch });
}

/// Vehicles about to go through a breakable pane break it: the box a vehicle sweeps over the
/// next moment (its width, from its middle to past its front along the way it moves) against
/// each pane near it.
pub fn vehicles(sim: &mut Sim, city: &City, events: &mut Vec<SimEvent>) {
    if city.panes.is_empty() {
        return;
    }
    let mut hits: Vec<(usize, Vec3)> = Vec::new();
    for e in sim.state.entities.iter() {
        if e.vehicle.is_none() || !city.panes.iter().any(|p| middle(p).distance(e.pos) < 25.0) {
            continue;
        }
        let Some(c) = crate::drive::Car::read(sim, e.id) else { continue };
        let flat = Vec3::new(c.vel.x, 0.0, c.vel.z);
        let speed = flat.length();
        if speed < MIN_SPEED {
            continue;
        }
        let dir = flat / speed;
        let reach = c.half.z.max(c.half.x) + speed * AHEAD_S + AHEAD_M;
        let (low, high) = (c.center.y - c.half.y - 0.3, c.center.y + c.half.y);
        let grow = Vec3::new(c.half.x + 0.2, 0.0, c.half.x + 0.2);
        for (i, p) in city.panes.iter().enumerate() {
            if p.max.y < low || p.min.y > high {
                continue;
            }
            let (lo, hi) = (p.min - grow, p.max + grow);
            let steps = (reach / 0.25).ceil() as i32;
            let crosses = (0..=steps).any(|k| {
                let q = c.center + dir * (reach * k as f32 / steps as f32);
                q.x >= lo.x && q.x <= hi.x && q.z >= lo.z && q.z <= hi.z
            });
            if crosses {
                hits.push((i, c.center));
            }
        }
    }
    for (i, from) in hits {
        smash(sim, city, i, from, events);
    }
}

/// Shots that struck the fixed world (where, and from where): any breakable pane they hit
/// breaks.
pub fn shots(sim: &mut Sim, city: &City, walls: &[(Vec3, Vec3)], events: &mut Vec<SimEvent>) {
    for &(at, from) in walls {
        let g = Vec3::splat(0.15);
        if let Some(i) = city.panes.iter().position(|p| at.cmpge(p.min - g).all() && at.cmple(p.max + g).all()) {
            smash(sim, city, i, from, events);
        }
    }
}
