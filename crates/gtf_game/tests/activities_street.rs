//! M7: the street activities with the bot (ignored, long): vigilante's five levels, the Purse
//! Snatcher and Getaway Needed, each on seeds 1-3. `activity <id>` sets the activity up where the
//! player stands (vigilante: into a police car) and plays it as a trial, won when it is done; the
//! bot (`Game::bot`) plays whichever street activity is on.

use gtf_game::Freeway;
use pav_core::games::Outcome;
use pav_core::{InputFrame, Sim};

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

/// Free roam on `seed`, then `activity <id>` and the bot for at most `secs` seconds; the outcome
/// and where the activity stood.
fn trial(id: &str, seed: u64, secs: u64) -> (Outcome, String) {
    gtf_game::register();
    let mut s = Sim::new("freeway/roam", seed).unwrap();
    for _ in 0..60 {
        s.step(&InputFrame::default());
    }
    s.step(&InputFrame { cmds: vec![format!("activity {id}")], ..Default::default() });
    for _ in 0..secs * 60 {
        if s.play().unwrap().outcome() != Outcome::Playing {
            break;
        }
        let input = s.bot_input();
        s.step(&input);
        s.drain_events();
    }
    let g = game(&s);
    let st = &g.activities;
    let line = format!("{id} seed {seed}: vigilante '{}', event '{}'", st.vigilante.last, st.events.last);
    (s.play().unwrap().outcome(), line)
}

fn three_seeds(id: &str, secs: u64) {
    let lost: Vec<String> = (1..=3)
        .filter_map(|seed| {
            let (o, line) = trial(id, seed, secs);
            eprintln!("{line}: {o:?}");
            (o != Outcome::Won).then_some(line)
        })
        .collect();
    assert!(lost.is_empty(), "{}", lost.join("\n"));
}

#[test]
#[ignore = "long: five vigilante levels, three seeds"]
fn the_bot_finishes_vigilante_on_three_seeds() {
    three_seeds("vigilante", 1800);
}

#[test]
#[ignore = "long: a random event, three seeds"]
fn the_bot_returns_the_purse_on_three_seeds() {
    three_seeds("purse_snatcher", 600);
}

#[test]
#[ignore = "long: a random event, three seeds"]
fn the_bot_drives_the_robber_away_on_three_seeds() {
    three_seeds("getaway_needed", 600);
}
