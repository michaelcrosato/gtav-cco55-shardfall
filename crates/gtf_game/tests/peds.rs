//! M4: pedestrians fill the streets around the player, keep to walkable ground, step onto a
//! signalled crossing only on its walk light, rarely get stuck, and replay exactly.

use std::collections::BTreeMap;

use glam::Vec2;
use gtf_game::Freeway;
use gtf_game::peds::Doing;
use pav_core::{InputFrame, Sim};
use pav_world::walk::Walk;

fn noon(sim: &mut Sim) {
    sim.step(&InputFrame { cmds: vec!["time 12:30".into()], ..Default::default() });
}

#[test]
fn people_walk_the_sidewalks_and_cross_on_the_walk_light() {
    gtf_game::register();
    let mut sim = Sim::new("freeway/view/downtown_street", 7).unwrap();
    noon(&mut sim);
    let city = gtf_game::world::city().unwrap();
    let walk = &city.walk;
    let mut on_crossing: BTreeMap<u32, bool> = BTreeMap::new();
    let (mut entered, mut red, mut waiting, mut samples, mut off, mut most) = (0, 0, 0, 0, 0, 0);
    for _ in 0..7200 {
        sim.step(&InputFrame::default());
        let g = sim.play().unwrap().get::<Freeway>().unwrap();
        most = most.max(g.peds.list.len());
        for p in &g.peds.list {
            let Some(feet) = sim.feet(p.id) else { continue };
            let at = Vec2::new(feet.x, feet.z);
            samples += 1;
            waiting += (p.doing == Doing::Wait) as usize;
            // Off walkable ground: the tile and its four neighbours all blocked.
            let t = walk.tile_of(at);
            let near = [glam::IVec2::ZERO, glam::IVec2::X, -glam::IVec2::X, glam::IVec2::Y, -glam::IVec2::Y];
            if near.iter().all(|d| !walk.walkable(t + *d)) {
                off += 1;
            }
            let crossing = walk.at(at);
            let now = matches!(crossing, Walk::Crossing { .. });
            if let Walk::Crossing { junction, ns } = crossing {
                if on_crossing.get(&p.id.0) == Some(&false) {
                    entered += 1;
                    // Stepping off the kerb with the walk light long gone (it ends 2 s before the
                    // cross traffic's green; a second's grace for the last stride).
                    let plan = city.lanes.junctions[junction as usize].signal;
                    if plan.is_some_and(|s| s.walk(ns, g.time).is_none() && s.walk(ns, g.time - 1.0).is_none()) {
                        red += 1;
                    }
                }
            }
            on_crossing.insert(p.id.0, now);
        }
    }
    let g = sim.play().unwrap().get::<Freeway>().unwrap();
    eprintln!(
        "peds: at most {most} out, {} spawned, {} left, {} stuck; {entered} crossings entered ({red} on red), {waiting} waiting samples, {off}/{samples} off walkable ground",
        g.peds.spawned, g.peds.despawned, g.peds.stuck
    );
    assert!(most >= 30, "a downtown crowd at midday: {most}");
    assert!(entered >= 5, "people cross the streets: {entered}");
    assert!(waiting > 0, "someone waited for the walk light");
    assert!(red * 10 <= entered, "{red} of {entered} crossings entered on red");
    assert!(off * 200 <= samples, "{off} of {samples} samples off walkable ground");
    assert!(g.peds.stuck * 4 <= g.peds.spawned.max(4), "{} stuck of {} spawned", g.peds.stuck, g.peds.spawned);
}

#[test]
fn the_crowd_replays_exactly() {
    gtf_game::register();
    let run = || {
        let mut sim = Sim::new("freeway/roam", 11).unwrap();
        noon(&mut sim);
        for _ in 0..600 {
            sim.step(&InputFrame::default());
        }
        let n = sim.play().unwrap().get::<Freeway>().unwrap().peds.list.len();
        (sim.state_hash(), n)
    };
    let (a, n) = run();
    assert!(n > 5, "{n} people out");
    assert_eq!(run(), (a, n));
}
