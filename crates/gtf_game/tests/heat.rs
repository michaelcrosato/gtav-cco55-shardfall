//! M5 heat: civilians who see a crime phone it in and the stars rise; police arrive from out
//! of view and close in; at one star an officer arrests a player who stands still (BUSTED,
//! fined, out of SPPD Central); at no health the player is WASTED and wakes outside St.
//! Brigid's, $250 lighter; either way the heat ends.

use gtf_game::Freeway;
use gtf_game::fate::Fate;
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

fn roam(seed: u64) -> Sim {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", seed).unwrap();
    cmd(&mut sim, "time 13:00");
    steps(&mut sim, 120);
    sim
}

#[test]
fn witnesses_phone_in_a_crime_and_the_stars_rise() {
    let mut sim = roam(4);
    assert!(game(&sim).peds.list.len() > 10, "a crowd to witness it");
    cmd(&mut sim, "crime assault");
    steps(&mut sim, 2);
    let calls = game(&sim).heat.reports.len();
    assert!(calls > 0, "someone saw it and is calling");
    assert_eq!(game(&sim).heat.stars, 0, "no heat until a call goes through");
    let mut t = 0;
    while game(&sim).heat.stars == 0 && t < 600 {
        sim.step(&InputFrame::default());
        t += 1;
    }
    eprintln!("{calls} calls; one star after {:.1} s", t as f32 / 60.0);
    assert_eq!(game(&sim).heat.stars, 1);
    assert!(game(&sim).heat.searching, "they know where it happened, not where the player went");
}

#[test]
fn police_come_from_out_of_view_and_arrest_a_player_who_stands_still() {
    let mut sim = roam(6);
    let money = game(&sim).money;
    cmd(&mut sim, "wanted 1");
    let mut busted_at = None;
    for t in 0..(150 * 60) {
        sim.step(&InputFrame::default());
        if let Fate::Busted { .. } = game(&sim).fate {
            busted_at = Some(t);
            break;
        }
    }
    let g = game(&sim);
    eprintln!(
        "police spawned {}, in view {}; busted after {:?} s",
        g.police.spawned,
        g.police.spawn_in_view,
        busted_at.map(|t| t / 60)
    );
    assert!(g.police.spawned >= 1);
    assert_eq!(g.police.spawn_in_view, 0, "never spawned in view");
    assert!(busted_at.is_some(), "an officer arrested the player");
    steps(&mut sim, (gtf_game::fate::LENGTH * 60.0) as usize + 10);
    let g = game(&sim);
    assert!(g.fate.alive(), "respawned");
    assert_eq!(g.heat.stars, 0, "the heat ended");
    assert_eq!(g.money, money - gtf_game::fate::busted_fee(money), "fined");
    let city = gtf_game::world::city().unwrap();
    let station = gtf_game::world::door(&city, "police").unwrap();
    let feet = sim.feet(sim.state.player.unwrap()).unwrap();
    assert!(feet.distance(station) < 6.0, "out of SPPD Central: {feet} vs {station}");
}

#[test]
fn wasted_wakes_the_player_at_the_hospital() {
    let mut sim = roam(8);
    let money = game(&sim).money;
    cmd(&mut sim, "wanted 2");
    game_mut(&mut sim).combat.health = 0.0;
    steps(&mut sim, 3);
    assert!(matches!(game(&sim).fate, Fate::Wasted { .. }));
    steps(&mut sim, (gtf_game::fate::LENGTH * 60.0) as usize + 10);
    let g = game(&sim);
    assert!(g.fate.alive());
    assert_eq!(g.money, money - gtf_game::fate::WASTED_FEE);
    assert_eq!(g.heat.stars, 0);
    assert!(g.combat.health > 150.0, "healed");
    let city = gtf_game::world::city().unwrap();
    let door = gtf_game::world::door(&city, "hospital").unwrap();
    assert!(sim.feet(sim.state.player.unwrap()).unwrap().distance(door) < 6.0);
}
