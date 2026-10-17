//! M5: armed opponents — a gang takes cover facing the player and trades fire from it; a player
//! who stays in one cover gets flanked; hurt gang members fall back; guards lob a grenade at a
//! player dug in behind cover.

use glam::{Vec2, Vec3};
use gtf_game::Freeway;
use gtf_game::enemies::Act;
use pav_core::input::pad;
use pav_core::{InputFrame, Sim};

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn game_mut(sim: &mut Sim) -> &mut Freeway {
    sim.state.play.as_mut().unwrap().get_mut::<Freeway>().unwrap()
}

fn cmd(sim: &mut Sim, c: &str) {
    sim.step(&InputFrame { cmds: vec![c.into()], ..Default::default() });
}

fn steps(sim: &mut Sim, n: usize) {
    for _ in 0..n {
        sim.step(&InputFrame::default());
    }
}

fn start(seed: u64) -> Sim {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", seed).unwrap();
    cmd(&mut sim, "time 13:00");
    steps(&mut sim, 60);
    sim
}

/// Behind a car parked in the street, as a player would take it (R1, looking across it): the
/// spot and the direction away from the car.
fn take_cover(sim: &mut Sim) -> (Vec3, Vec2) {
    cmd(sim, "car daimon_titan");
    steps(sim, 30);
    let pid = sim.state.player.unwrap();
    let feet = sim.feet(pid).unwrap();
    let car = sim.state.entities.iter().filter(|e| e.name == "daimon_titan").map(|e| e.id).max().expect("the car");
    let (spot, normal) = gtf_game::cover::behind_car(sim, car, feet, 0.55).expect("a stopped car");
    sim.set_position(pid, spot + Vec3::new(normal.x, 0.0, normal.y) * 1.0 + Vec3::Y * 0.1);
    game_mut(sim).cam.face((-normal.x).atan2(normal.y));
    steps(sim, 30);
    let mut r1 = InputFrame::default();
    r1.pad.held = pad::R1;
    r1.pad.pressed = pad::R1;
    sim.step(&r1);
    steps(sim, 30);
    let spot = game(sim).cover.at.expect("in cover");
    assert_eq!(spot.car, Some(car), "behind the car");
    (spot.pos, spot.normal)
}

/// `n` of `side` beyond the player's wall (the threat side), `dist` metres off, spread along it.
fn beyond(sim: &mut Sim, at: Vec3, normal: Vec2, n: usize, side: &str, weapon: &str, dist: f32) {
    let city = gtf_game::world::city().unwrap();
    let mut en = std::mem::take(&mut game_mut(sim).enemies);
    let along = Vec2::new(normal.y, -normal.x);
    for i in 0..n {
        let off = (i as f32 - (n as f32 - 1.0) * 0.5) * 5.0;
        let want = Vec2::new(at.x, at.z) - normal * dist + along * off;
        let p = city.walk.nearest_walkable(want, 6).unwrap_or(want);
        let feet = Vec3::new(p.x, at.y, p.y);
        let face = (at.x - feet.x).atan2(at.z - feet.z);
        en.spawn(sim, side, feet, face, Some(weapon)).expect("spawned");
    }
    game_mut(sim).enemies = en;
    game_mut(sim).enemies.alert();
}

fn in_cover(a: &Act) -> bool {
    matches!(a, Act::Hide { at, .. } | Act::Peek { at, .. } if at.normal != Vec2::ZERO)
}

/// A parking-lot fight: cars parked 1.5 m in front of where each of `n` opponents stands (between
/// them and the player), the opponents spread along a line `dist` metres out beyond the player's
/// cover (`at`, `normal`).
fn arena(sim: &mut Sim, at: Vec3, normal: Vec2, n: usize, side: &str, weapon: &str, dist: f32) {
    let city = gtf_game::world::city().unwrap();
    let along = Vec2::new(normal.y, -normal.x);
    for i in 0..n {
        let off = (i as f32 - (n as f32 - 1.0) * 0.5) * 6.0;
        let want = Vec2::new(at.x, at.z) - normal * dist + along * off;
        let p = city.walk.nearest_walkable(want, 6).unwrap_or(want);
        let car_at = p + normal * 2.6;
        let yaw = along.x.atan2(along.y);
        gtf_game::vehicles::spawn(sim, "zephyr_wagon", Vec3::new(car_at.x, at.y + 0.05, car_at.y), yaw, None).expect("a car");
    }
    steps(sim, 20);
    beyond(sim, at, normal, n, side, weapon, dist);
}

#[test]
fn a_gang_takes_cover_and_trades_fire_from_it() {
    let mut sim = start(5);
    // The player in the open, in the middle of the street; the gang down the street behind cars.
    let city = gtf_game::world::city().unwrap();
    let feet = sim.feet(sim.state.player.unwrap()).unwrap();
    let keep = |l: &pav_world::lanes::Lane| !l.closed;
    let (lane, along, _) = city.lanes.nearest(feet, keep).unwrap();
    let (at, dir) = city.lanes.lanes[lane as usize].at(along);
    sim.set_position(sim.state.player.unwrap(), at + Vec3::Y * 0.1);
    steps(&mut sim, 20);
    let at = sim.feet(sim.state.player.unwrap()).unwrap();
    arena(&mut sim, at, Vec2::new(-dir.x, -dir.z), 3, "gang", "p9", 17.0);
    let mut fought_from_cover = std::collections::BTreeSet::new();
    for _ in 0..(20 * 60) {
        sim.step(&InputFrame::default());
        for e in &game(&sim).enemies.list {
            if in_cover(&e.act) {
                fought_from_cover.insert(e.id);
            }
        }
    }
    let g = game(&sim);
    eprintln!(
        "from cover {}/3, shots {}, hits {}, player health {:.0}",
        fought_from_cover.len(),
        g.enemies.shots,
        g.enemies.hits,
        g.combat.health
    );
    assert!(fought_from_cover.len() >= 2, "they fought from cover");
    assert!(g.enemies.shots >= 5 && g.enemies.hits >= 1, "and traded fire");
    assert!(g.fate.alive(), "not a massacre in 20 s");
}

#[test]
fn a_player_who_stays_in_one_cover_gets_flanked() {
    let mut sim = start(5);
    let (at, normal) = take_cover(&mut sim);
    arena(&mut sim, at, normal, 3, "gang", "p9", 17.0);
    // Someone gets round to the player's side of the car, with a line to them.
    let mut round = None;
    for t in 0..(30 * 60) {
        sim.step(&InputFrame::default());
        let g = game(&sim);
        for e in &g.enemies.list {
            let Some(f) = sim.feet(e.id) else { continue };
            let side = Vec2::new(f.x - at.x, f.z - at.z).dot(normal);
            let line = gtf_game::heat::sight(&sim, f + Vec3::Y * 1.5, at + Vec3::Y * 0.9);
            if side > 1.0 && line && round.is_none() {
                round = Some((t, e.id, side));
            }
        }
        if round.is_some() {
            break;
        }
    }
    let g = game(&sim);
    eprintln!(
        "player cover {:?}; flanks {}; got round {round:?} (tick, who, metres out)",
        g.cover.at.map(|s| s.car),
        g.enemies.flanks
    );
    assert!(g.cover.at.is_some(), "the player held the cover");
    assert!(g.enemies.flanks >= 1, "the camper got flanked");
    assert!(round.is_some(), "and someone got round the car");
}

#[test]
fn hurt_gang_members_fall_back() {
    let mut sim = start(5);
    cmd(&mut sim, "enemies 1 gang p9");
    cmd(&mut sim, "enemies alert");
    steps(&mut sim, 120);
    let id = game(&sim).enemies.list[0].id;
    let before = sim.feet(id).unwrap().distance(sim.feet(sim.state.player.unwrap()).unwrap());
    game_mut(&mut sim).combat.hp.insert(id.0, 25.0);
    steps(&mut sim, 5);
    assert_eq!(game(&sim).enemies.retreats, 1, "hurt, it falls back");
    steps(&mut sim, 6 * 60);
    let after = sim.feet(id).unwrap().distance(sim.feet(sim.state.player.unwrap()).unwrap());
    eprintln!("{before:.1} m off, then {after:.1} m after falling back");
    assert!(after > before + 5.0, "farther off");
}

#[test]
fn guards_lob_a_grenade_at_a_player_dug_in() {
    let mut sim = start(5);
    let (at, normal) = take_cover(&mut sim);
    arena(&mut sim, at, normal, 2, "guard", "c4r", 14.0);
    for _ in 0..(25 * 60) {
        sim.step(&InputFrame::default());
        if game(&sim).enemies.thrown > 0 {
            break;
        }
    }
    let g = game(&sim);
    eprintln!("grenades thrown {}, in flight {}", g.enemies.thrown, g.combat.grenades.len());
    assert!(g.enemies.thrown >= 1, "a guard lobbed one");
}
