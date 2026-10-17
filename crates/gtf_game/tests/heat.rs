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

/// One escape: cruising with the traffic (`rolling`) or sitting still at three stars until a
/// police unit spots the car, then on autopilot running from them. The seconds it took to clear
/// the heat (None: not within three minutes), and a line about it.
fn escape(seed: u64, rolling: bool) -> (Option<u32>, String) {
    let mut sim = roam(seed);
    cmd(&mut sim, "drive okami_kaze");
    if rolling {
        cmd(&mut sim, "autopilot on");
    }
    cmd(&mut sim, "wanted 3");
    let mut waited = 0;
    while waited < 120 * 60 && (game(&sim).heat.unseen > 0.0 || game(&sim).heat.searching) {
        let input = sim.bot_input();
        sim.step(&input);
        waited += 1;
    }
    assert!(waited < 120 * 60, "seed {seed}: the police found the car");
    cmd(&mut sim, "autopilot escape");
    let mut cleared = None;
    for t in 0..(180 * 60) {
        let input = sim.bot_input();
        sim.step(&input);
        sim.drain_events();
        let g = game(&sim);
        if g.heat.stars == 0 {
            cleared = Some(t / 60);
            break;
        }
        if !g.fate.alive() {
            break;
        }
    }
    let g = game(&sim);
    let line = format!(
        "seed {seed} ({}): spotted after {} s; cleared after {:?} s (fate {:?}, police {}, peak {}, cars left {}, taken {})",
        if rolling { "rolling" } else { "standing" },
        waited / 60,
        cleared,
        g.fate,
        g.police.spawned,
        g.heat.peak,
        g.bot.bailed,
        g.bot.taken
    );
    (cleared, line)
}

/// The heat-evasion bot (D13): cruising in a fast car at three stars until a police unit spots
/// it, it then runs on autopilot — leaving wrecked or boxed-in cars for others — and shakes the
/// police off within three minutes, on three seeds. (Over seeds 1–8 it escapes 7 of 8 times:
/// `evasion_rate`.)
#[test]
fn the_evasion_bot_escapes_three_stars() {
    for seed in [1, 2, 4] {
        let (cleared, line) = escape(seed, true);
        eprintln!("{line}");
        assert!(cleared.is_some(), "{line}");
    }
}

/// How often the bot gets away, over eight seeds, from a rolling start and from a standstill
/// with the police already on it (`cargo test -p gtf_game --test heat evasion_rate -- --ignored
/// --nocapture`).
#[test]
#[ignore]
fn evasion_rate() {
    for rolling in [true, false] {
        let mut got_away = 0;
        for seed in 1..=8 {
            let (cleared, line) = escape(seed, rolling);
            eprintln!("{line}");
            got_away += cleared.is_some() as u32;
        }
        eprintln!("{}: {got_away}/8 escaped", if rolling { "rolling" } else { "standing" });
    }
}

/// Paloma Customs: a car stopped at its door while the police are not looking comes out
/// resprayed and repaired, $200 lighter, with no heat; with no car in sight of them, a car they
/// have not seen drops a star.
#[test]
fn a_respray_clears_the_heat_and_a_fresh_car_drops_a_star() {
    let mut sim = roam(3);
    let city = gtf_game::world::city().unwrap();
    let door = gtf_game::world::door(&city, "customs").expect("Paloma Customs");
    sim.set_position(sim.state.player.unwrap(), door + glam::Vec3::Y * 0.1);
    steps(&mut sim, 20);
    cmd(&mut sim, "drive okami_kaze");
    let car = game(&sim).drive.vehicle().expect("in a car");
    let paint = sim.state.entities.get(car).and_then(|e| e.vehicle.as_ref()).map(|v| v.def.color.clone());
    game_mut(&mut sim).money = 1000;
    cmd(&mut sim, "wanted 2");
    let mut t = 0;
    while game(&sim).heat.stars > 0 && t < 300 {
        sim.step(&InputFrame::default());
        t += 1;
    }
    let g = game(&sim);
    eprintln!("respray after {t} ticks: stars {}, money {}, resprays {}", g.heat.stars, g.money, g.resprays);
    assert_eq!(g.heat.stars, 0, "the heat ended");
    assert_eq!(g.money, 800, "for $200");
    assert_eq!(g.resprays, 1);
    let now = sim.state.entities.get(car).and_then(|e| e.vehicle.as_ref()).map(|v| v.def.color.clone());
    assert_ne!(paint, now, "a new colour");

    // The car swap (away from the garage): wanted, searched for in one car, into another unseen.
    let mut sim = roam(3);
    cmd(&mut sim, "drive okami_kaze");
    let car = game(&sim).drive.vehicle().expect("in a car");
    cmd(&mut sim, "wanted 3");
    {
        let g = game_mut(&mut sim);
        g.heat.searching = true;
        g.heat.unseen = 5.0;
        g.heat.seen_car = Some(car);
    }
    cmd(&mut sim, "drive zephyr_wagon");
    let g = game(&sim);
    assert_ne!(g.drive.vehicle(), Some(car), "in another car");
    assert_eq!(g.heat.stars, 2, "a star off for the car they are not looking for");
}
