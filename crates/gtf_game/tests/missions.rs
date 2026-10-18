//! M6: missions. Every mission file validates against its district; the bot finishes both test
//! missions (every step kind, both ways through the choice) on three seeds; a failed mission
//! retries from its checkpoint, offers Skip after three fails, and Skip moves on; a pass pays,
//! keeps the record and autosaves.

use gtf_game::Freeway;
use gtf_game::missions;
use pav_core::games::Outcome;
use pav_core::input::{PadState, pad};
use pav_core::{InputFrame, Sim};

fn sim(scene: &str, seed: u64) -> Sim {
    gtf_game::register();
    Sim::new(scene, seed).unwrap_or_else(|e| panic!("{scene}: {e:#}"))
}

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn cmd(c: &str) -> InputFrame {
    InputFrame { cmds: vec![c.into()], ..Default::default() }
}

fn press(bits: u32) -> InputFrame {
    InputFrame { pad: PadState { held: bits, pressed: bits, ..Default::default() }, ..Default::default() }
}

/// Plays the mission scene with its bot for at most `secs` seconds, reporting where it got to
/// every so often; returns the outcome and the last status.
fn bot_run(scene: &str, seed: u64, secs: u64, before: &[&str]) -> (Outcome, serde_json::Value) {
    let mut s = sim(scene, seed);
    for c in before {
        s.step(&cmd(c));
    }
    let hz = 60;
    let mut last_step = usize::MAX;
    for t in 0..secs * hz {
        let o = s.play().unwrap().outcome();
        if o != Outcome::Playing {
            return (o, game(&s).mission_status());
        }
        let input = s.bot_input();
        s.step(&input);
        let st = game(&s).mission_status();
        let step = st["step"].as_u64().unwrap_or(0) as usize;
        if step != last_step {
            last_step = step;
            eprintln!("{scene} seed {seed} t={:.1}s step {} {} — {}", t as f32 / hz as f32, step, st["kind"], st["objective"]);
        }
    }
    (s.play().unwrap().outcome(), game(&s).mission_status())
}

#[test]
fn every_mission_validates_against_its_district() {
    gtf_game::register();
    for (key, text) in missions::FILES {
        let d = missions::parse(text).unwrap_or_else(|e| panic!("missions/{key}.toml: {e}"));
        assert_eq!(&d.key, key, "the file's key is its name");
        let city = if d.district == "dev" { gtf_game::world::dev_city() } else { gtf_game::world::city() }.unwrap();
        let errs = missions::validate(&d, text, &city);
        assert!(errs.is_empty(), "{}", errs.join("\n"));
    }
}

#[test]
fn the_validator_names_the_file_and_line() {
    gtf_game::register();
    let text = r#"
key = "broken"
title = "Broken"
protagonist = "nobody"
start = "spot:nowhere"

[[step]]
kind = "goto"

[[step]]
kind = "kill"
targets = ["ghost"]
"#;
    let d = missions::parse(text).unwrap();
    let city = gtf_game::world::dev_city().unwrap();
    let errs = missions::validate(&d, text, &city);
    let all = errs.join("\n");
    assert!(all.contains("no protagonist 'nobody'"), "{all}");
    assert!(all.contains("'spot:nowhere' is not in"), "{all}");
    assert!(all.contains("missions/broken.toml:7: goto needs a target"), "{all}");
    assert!(all.contains("missions/broken.toml:10: no actor 'ghost'"), "{all}");
}

#[test]
fn every_step_kind_is_in_a_test_mission() {
    let mut seen = std::collections::BTreeSet::new();
    for (_, text) in missions::FILES {
        let d = missions::parse(text).unwrap();
        for s in &d.step {
            seen.insert(s.kind.name());
            for sub in &s.steps {
                seen.insert(sub.kind.name());
            }
        }
    }
    for k in missions::StepKind::ALL {
        assert!(seen.contains(k.name()), "no test mission has a {} step", k.name());
    }
}

#[test]
fn the_bot_finishes_both_test_missions_on_three_seeds() {
    for scene in ["freeway/mission/dev_test", "freeway/mission/dev_test2"] {
        for seed in 1..=3 {
            let (o, st) = bot_run(scene, seed, 600, &[]);
            assert_eq!(o, Outcome::Won, "{scene} seed {seed}: {st}");
        }
    }
}

#[test]
fn the_bot_takes_the_loud_way_too() {
    let (o, st) = bot_run("freeway/mission/dev_test2", 2, 600, &["mission choose loud"]);
    assert_eq!(o, Outcome::Won, "{st}");
    let flags: Vec<String> = serde_json::from_value(st["flags"].clone()).unwrap();
    assert!(flags.contains(&"loud".to_string()), "the loud way was taken: {flags:?}");
}

/// Plays `scene` with the bot until the mission reaches step `n`.
fn bot_until_step(scene: &str, n: u64) -> Sim {
    let mut s = sim(scene, 1);
    for _ in 0..60 * 300 {
        if game(&s).mission_status()["step"].as_u64() == Some(n) {
            return s;
        }
        let input = s.bot_input();
        s.step(&input);
    }
    panic!("{scene}: never reached step {n}: {}", game(&s).mission_status());
}

/// Fails the mission and waits for the fail card; returns its items.
fn fail_card(s: &mut Sim) -> Vec<String> {
    s.step(&cmd("mission fail"));
    s.run(60 * 3, &InputFrame::default());
    let p = s.present().expect("the game presents itself");
    let screen = p.ui.screen.expect("the fail card");
    assert_eq!(screen.id, "mission/failed");
    let pav_core::present::ScreenBody::Menu(m) = screen.body else { panic!("a menu") };
    m.items.iter().map(|i| i.id.clone()).collect()
}

#[test]
fn a_failed_mission_retries_from_its_checkpoint_and_offers_skip_after_three_fails() {
    // dev_test2: the kill step (1) is a checkpoint; fail during collect (2).
    let mut s = bot_until_step("freeway/mission/dev_test2", 2);
    let killed = |s: &Sim| {
        let run = game(s).missions.run.as_ref().unwrap();
        ["g1", "g2"].iter().all(|id| gtf_game::missions::down_of(s, run, id))
    };
    assert!(killed(&s), "the bot took them out");
    let items = fail_card(&mut s);
    assert_eq!(items, ["retry", "quit"], "no Skip after one fail");
    s.step(&press(pad::SOUTH));
    let st = game(&s).mission_status();
    assert_eq!(st["phase"], "play");
    assert_eq!(st["step"], 1, "back at the checkpoint: the kill step");
    assert!(!killed(&s), "the Ditch Kings are back on their feet");
    assert_eq!(game(&s).missions.retries, 1);
    // Two more fails: Skip is offered, and skips to the next checkpoint (the stealth step, 5).
    for _ in 0..2 {
        let items = fail_card(&mut s);
        if items.len() == 3 {
            break;
        }
        s.step(&press(pad::SOUTH));
    }
    let items = fail_card_now(&s);
    assert_eq!(items, ["retry", "skip", "quit"], "Skip after three fails");
    s.step(&press(pad::DOWN));
    s.step(&InputFrame::default());
    s.step(&press(pad::SOUTH));
    let st = game(&s).mission_status();
    assert_eq!(st["phase"], "play", "{st}");
    assert_eq!(st["step"], 5, "skipped to the next checkpoint: {st}");
    assert_eq!(game(&s).missions.skips, 1);
}

/// The fail card's items as they stand.
fn fail_card_now(s: &Sim) -> Vec<String> {
    let p = s.present().expect("the game presents itself");
    let screen = p.ui.screen.expect("the fail card");
    let pav_core::present::ScreenBody::Menu(m) = screen.body else { panic!("a menu") };
    m.items.iter().map(|i| i.id.clone()).collect()
}

#[test]
fn quitting_leaves_the_mission_and_its_people() {
    let mut s = bot_until_step("freeway/mission/dev_test2", 2);
    s.step(&cmd("mission quit"));
    assert!(game(&s).missions.run.is_none());
    assert!(s.state.entities.iter().all(|e| !e.name.starts_with("mission:")), "the mission's people are gone");
}

#[test]
fn a_pass_pays_keeps_the_record_texts_and_autosaves() {
    let mut s = sim("freeway/mission/dev_test2", 1);
    let cash0 = game(&s).money;
    for _ in 0..60 * 600 {
        if game(&s).missions.passed > 0 {
            break;
        }
        let input = s.bot_input();
        s.step(&input);
    }
    let g = game(&s);
    assert_eq!(g.missions.passed, 1, "{}", g.mission_status());
    assert_eq!(g.money, cash0 + 800, "the reward is paid");
    let r = &g.missions.records["dev_test2"];
    assert!(r.passed && r.best.is_some() && r.medal.is_some(), "{r:?}");
    assert_eq!(g.phone.messages.first().map(|m| m.from.as_str()), Some("roxy"), "Roxy's text arrives");
    // The results, then on: an autosave.
    for _ in 0..60 * 8 {
        let input = s.bot_input();
        s.step(&input);
    }
    let g = game(&s);
    assert!(g.missions.run.is_none(), "the results were left");
    assert_eq!(g.save_req.as_ref().map(|r| r.slot.as_str()), Some("auto"), "autosaved");
}

#[test]
#[ignore = "exploratory: prints the bot's progress (cargo test -p gtf_game --test missions -- --ignored --nocapture)"]
fn bot_progress_report() {
    for scene in ["freeway/mission/dev_test", "freeway/mission/dev_test2"] {
        let (o, st) = bot_run(scene, 1, 420, &[]);
        eprintln!("{scene}: {o:?} {st}");
    }
}

#[test]
#[ignore = "diagnostic: GTF_SCENE, GTF_STEP and GTF_SECS pick the run; prints the bot's view each second"]
fn bot_trace() {
    let scene = std::env::var("GTF_SCENE").unwrap_or_else(|_| "freeway/mission/dev_test".into());
    let from: u64 = std::env::var("GTF_STEP").ok().and_then(|x| x.parse().ok()).unwrap_or(0);
    let secs: u64 = std::env::var("GTF_SECS").ok().and_then(|x| x.parse().ok()).unwrap_or(120);
    let mut s = sim(&scene, 1);
    for t in 0..secs * 60 {
        if s.play().unwrap().outcome() != Outcome::Playing {
            break;
        }
        let input = s.bot_input();
        s.step(&input);
        let g = game(&s);
        let st = g.mission_status();
        if st["step"].as_u64().unwrap_or(0) >= from && t % 60 == 0 {
            let feet = s.player().map(|p| p.pos).unwrap_or_default();
            let actors: Vec<String> = g
                .missions
                .run
                .as_ref()
                .map(|r| {
                    r.actors
                        .iter()
                        .map(|(k, a)| {
                            let p = a.entity.and_then(|e| s.state.entities.get(e)).map(|e| e.pos).unwrap_or(a.at);
                            format!("{k}@{:.1},{:.1}", p.x, p.z)
                        })
                        .collect()
                })
                .unwrap_or_default();
            let near = gtf_game::drive::Drive::nearest(&s, s.player().map(|p| p.pos).unwrap_or_default() - glam::Vec3::Y * 0.9);
            eprintln!(
                "    actors {actors:?} nearest car {near:?} health {:.0} armor {:.0} stars {}",
                g.combat.health, g.combat.armor, g.heat.stars
            );
            if let Some(a) = g.traffic.cars.iter().find(|a| a.pilot) {
                eprintln!(
                    "    pilot lane {} along {:.1} route {:?} goal {:?} planned {} arrived {} speed {:.1} held {} blocked {:.1} mood {:?}",
                    a.lane, a.along, a.route, a.goal, a.planned, a.arrived, a.speed, a.held, a.blocked, a.mood
                );
            }
            eprintln!(
                "t={:5.1} step {} {} | feet {:.1},{:.1},{:.1} drive {:?} | bot move {:.2},{:.2} held {:#x} pressed {:#x} stick_r {:.2} cmds {:?} | pilot {} arrived {}",
                t as f32 / 60.0,
                st["step"],
                st["kind"],
                feet.x,
                feet.y,
                feet.z,
                g.drive.phase,
                input.move_dir.x,
                input.move_dir.y,
                input.pad.held,
                input.pad.pressed,
                input.pad.stick_r.x,
                input.cmds,
                g.traffic.cars.iter().any(|a| a.pilot),
                g.traffic.pilot_arrived(),
            );
        }
    }
    eprintln!("{}", game(&s).mission_status());
}
