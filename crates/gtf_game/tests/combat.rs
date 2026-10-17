//! M5: on-foot combat — a pistol drops someone in front of the camera in a few shots (the aim
//! assist finds them on L2), the magazine empties and reloads, fists land a combo, and the
//! people nearby run from the gunfire.

use glam::Vec3;
use gtf_game::Freeway;
use gtf_game::peds::Doing;
use pav_core::input::pad;
use pav_core::{InputFrame, Sim};

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn cmd(sim: &mut Sim, c: &str) {
    sim.step(&InputFrame { cmds: vec![c.into()], ..Default::default() });
}

/// A stranger standing `ahead` metres along the camera's view.
fn target(sim: &mut Sim, ahead: f32) -> pav_core::entity::EntityId {
    let cam = game(sim).cam.intent();
    let f = (cam.target - cam.eye).normalize();
    let flat = Vec3::new(f.x, 0.0, f.z).normalize();
    let feet = sim.feet(sim.state.player.unwrap()).unwrap() + flat * ahead;
    sim.spawn_npc("target", feet, 0.0, pav_core::puppet::PuppetDef::default(), None, None)
}

#[test]
fn a_pistol_drops_someone_and_reloads_and_the_crowd_runs() {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", 5).unwrap();
    cmd(&mut sim, "time 12:30");
    for _ in 0..120 {
        sim.step(&InputFrame::default());
    }
    cmd(&mut sim, "weapon pistol");
    let who = target(&mut sim, 9.0);
    for _ in 0..20 {
        sim.step(&InputFrame::default());
    }
    // Hold L2 (the assist snaps on the press), pull R2 every 15 ticks.
    let mut fired = 0;
    for t in 0..400 {
        let mut f = InputFrame::default();
        f.pad.held = pad::L2;
        if t == 0 {
            f.pad.pressed = pad::L2;
        }
        if t % 15 == 5 {
            f.pad.held |= pad::R2;
            f.pad.pressed |= pad::R2;
            fired += 1;
        }
        sim.step(&f);
        if sim.state.entities.get(who).is_some_and(|e| e.name.starts_with("dead:")) {
            break;
        }
    }
    let g = game(&sim);
    eprintln!(
        "fired {fired}: {} shots, {} hits, {} kills; mag {:?}",
        g.combat.shots,
        g.combat.hits,
        g.combat.kills,
        g.combat.mag.get(1)
    );
    assert!(sim.state.entities.get(who).is_some_and(|e| e.name.starts_with("dead:")), "the target fell");
    assert!(g.combat.hits >= 2 && g.combat.kills >= 1);
    let fleeing = g.peds.list.iter().filter(|p| matches!(p.doing, Doing::Flee | Doing::Cower)).count();
    assert!(fleeing > 0 || g.peds.list.is_empty(), "the gunfire scattered the crowd");
    // Empty the magazine: it reloads from the reserve.
    let before = g.combat.reserve[1];
    for t in 0..600 {
        let mut f = InputFrame::default();
        if t % 15 == 5 {
            f.pad.held = pad::R2;
            f.pad.pressed = pad::R2;
        }
        sim.step(&f);
    }
    let g = game(&sim);
    assert!(g.combat.reserve[1] < before, "a reload took rounds from the reserve: {} -> {}", before, g.combat.reserve[1]);
}

#[test]
fn fists_land_a_combo() {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", 6).unwrap();
    for _ in 0..30 {
        sim.step(&InputFrame::default());
    }
    cmd(&mut sim, "weapon fists");
    // Someone right in front of the player's body.
    let p = sim.state.player.unwrap();
    let facing = sim.state.entities.get(p).and_then(|e| e.character.as_ref()).map_or(0.0, |c| c.facing);
    let feet = sim.feet(p).unwrap() + Vec3::new(facing.sin(), 0.0, facing.cos()) * 1.0;
    let who = sim.spawn_npc("target", feet, facing + std::f32::consts::PI, pav_core::puppet::PuppetDef::default(), None, None);
    for t in 0..240 {
        let mut f = InputFrame::default();
        if t % 30 == 0 {
            f.pad.pressed = pad::EAST;
        }
        sim.step(&f);
    }
    let g = game(&sim);
    assert!(g.combat.hits >= 3, "{} hits", g.combat.hits);
    assert!(g.combat.hp.get(&who.0).is_none_or(|hp| *hp < 100.0), "the target was hurt");
}

#[test]
fn the_aim_assist_puts_the_reticle_on_someone_near_it() {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", 8).unwrap();
    for _ in 0..30 {
        sim.step(&InputFrame::default());
    }
    cmd(&mut sim, "weapon pistol");
    // Someone 12 m out, 6° left of the view.
    let cam = game(&sim).cam.intent();
    let f = (cam.target - cam.eye).normalize();
    let flat = Vec3::new(f.x, 0.0, f.z).normalize();
    let a = 6f32.to_radians();
    let dir = Vec3::new(flat.x * a.cos() - flat.z * a.sin(), 0.0, flat.x * a.sin() + flat.z * a.cos());
    let feet = sim.feet(sim.state.player.unwrap()).unwrap() + dir * 12.0;
    let who = sim.spawn_npc("target", feet, 0.0, pav_core::puppet::PuppetDef::default(), None, None);
    for _ in 0..5 {
        sim.step(&InputFrame::default());
    }
    let mut f = InputFrame::default();
    f.pad.held = pad::L2;
    f.pad.pressed = pad::L2;
    sim.step(&f);
    for _ in 0..40 {
        let mut f = InputFrame::default();
        f.pad.held = pad::L2;
        sim.step(&f);
    }
    let cam = game(&sim).cam.intent();
    let chest = sim.feet(who).unwrap() + Vec3::Y * 1.25;
    let ray = (cam.target - cam.eye).normalize();
    let to = chest - cam.eye;
    let miss = (to - ray * to.dot(ray)).length();
    assert!(miss < 0.35, "the view ray passes {miss:.2} m from the chest");
}
