//! M7 (PLAN §7.7, §7.8): determinism. Two minutes of roam with heat and traffic, recorded, replay
//! into a fresh session to the same state hash; a snapshot taken mid-chase, written to a file and
//! loaded into a fresh session, plays the next 600 ticks exactly as the run that was never
//! interrupted. (The city being the same whatever the seed, people and traffic aside, is
//! `district.rs`; snapshots, rewind and a short replay on foot are `bootstrap.rs`.)

use gtf_game::Freeway;
use pav_core::{InputFrame, Sim};

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn cmd(c: &str) -> InputFrame {
    InputFrame { cmds: vec![c.into()], ..Default::default() }
}

/// Free roam at midday in a fast car on autopilot, two stars on it: the police come, the bot
/// runs from them through the traffic.
fn chase(seed: u64) -> Sim {
    gtf_game::register();
    let mut s = Sim::new("freeway/roam", seed).unwrap();
    s.step(&cmd("time 13:00"));
    s.run(120, &InputFrame::default());
    s.step(&cmd("drive okami_kaze"));
    s.step(&cmd("autopilot on"));
    s.step(&cmd("wanted 2"));
    s
}

/// The game's own bot plays `ticks` ticks (turning to escape once the police have the car).
fn bot(s: &mut Sim, ticks: usize) {
    for _ in 0..ticks {
        let g = game(s);
        if g.heat.stars > 0 && g.heat.unseen <= 0.0 && !g.heat.searching && !g.bot.escape {
            s.step(&cmd("autopilot escape"));
            continue;
        }
        let f = s.bot_input();
        s.step(&f);
        s.drain_events();
    }
}

#[test]
fn two_minutes_of_roam_with_heat_and_traffic_replay_exactly() {
    let mut s = chase(5);
    bot(&mut s, 120 * 60);
    let g = game(&s);
    let traffic = g.traffic.cars.len();
    eprintln!(
        "after {} ticks: {} police units came, {} stars at most, {traffic} cars about, escape {}",
        s.state.tick, g.police.spawned, g.heat.peak, g.bot.escape
    );
    assert!(g.police.spawned > 0 && g.heat.peak >= 2, "a chase happened");
    assert!(traffic > 10, "with traffic about");
    let hash = s.state_hash();
    // The recording (every input from the first tick, typed commands included) into a fresh
    // session of the same scene and seed.
    let mut again = Sim::new("freeway/roam", 5).unwrap();
    for f in s.recording.iter() {
        again.step(f);
    }
    assert_eq!(again.state.tick, s.state.tick);
    assert_eq!(format!("{:016x}", again.state_hash()), format!("{hash:016x}"), "the replay played out differently");
}

#[test]
fn a_snapshot_mid_chase_plays_on_exactly_in_a_fresh_session() {
    let mut s = chase(5);
    // Until the police have the car and the bot has been running from them for five seconds.
    let mut t = 0;
    while !game(&s).bot.escape && t < 120 * 60 {
        bot(&mut s, 1);
        t += 1;
    }
    bot(&mut s, 300);
    let g = game(&s);
    eprintln!(
        "snapshot at tick {}: {} stars, searching {}, {} police units",
        s.state.tick, g.heat.stars, g.heat.searching, g.police.spawned
    );
    assert!(g.bot.escape && g.heat.stars > 0 && g.police.spawned > 0, "mid-chase");
    let path = std::env::temp_dir().join(format!("gtf_replay_mid_chase_{}.state", std::process::id()));
    s.save_state(&path).unwrap();
    let from = s.recording.ticks() as usize;
    // The uninterrupted run, 600 ticks on.
    bot(&mut s, 600);
    let inputs: Vec<InputFrame> = s.recording.iter().skip(from).cloned().collect();
    assert_eq!(inputs.len(), 600);
    // A fresh session (another seed, even: the snapshot is the whole state) loads the file and
    // plays the same 600 inputs.
    let mut fresh = Sim::new("freeway/roam", 9).unwrap();
    fresh.load_state(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    for f in &inputs {
        fresh.step(f);
    }
    let (a, b) = (game(&s), game(&fresh));
    eprintln!(
        "tick {}: stars {} / {}, police {} / {}",
        s.state.tick, a.heat.stars, b.heat.stars, a.police.spawned, b.police.spawned
    );
    assert_eq!(fresh.state.tick, s.state.tick);
    assert_eq!(
        format!("{:016x}", fresh.state_hash()),
        format!("{:016x}", s.state_hash()),
        "the loaded snapshot played on differently"
    );
}
