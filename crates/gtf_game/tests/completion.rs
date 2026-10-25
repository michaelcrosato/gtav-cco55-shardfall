//! M7 acceptance (PLAN §9 M7): completion from play. In one game, from free roam, the bots play
//! every story mission (the heist the loud way), the race and both hold-ups (started as their
//! markers start them), vigilante and both random events, land every stunt jump and walk up to
//! every Lost Shield. Completion then holds the whole of the story, activities and shields
//! shares, and the gold medals' share is what the bots earned (100% with gold everywhere is
//! `activities.rs`'s `completion_reaches_100_with_everything_done`).

use glam::{Vec2, Vec3};
use gtf_game::Freeway;
use gtf_game::activities;
use gtf_game::missions::Phase;
use pav_core::input::{PadState, pad};
use pav_core::{InputFrame, Sim};

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn cmd(s: &mut Sim, c: &str) {
    s.step(&InputFrame { cmds: vec![c.into()], ..Default::default() });
}

fn steps(s: &mut Sim, n: usize) {
    for _ in 0..n {
        s.step(&InputFrame::default());
    }
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

/// The bot plays mission `key`, started from free roam; a fail is retried from its checkpoint
/// (✕ on the card) as a player would, twice at most. Whether it was passed.
fn play_mission(s: &mut Sim, key: &str) -> bool {
    cmd(s, &format!("mission start {key}"));
    let mut retries = 0;
    for _ in 0..60 * 900 {
        let g = game(s);
        let Some(run) = g.missions.run.as_ref() else {
            return g.missions.records.get(key).is_some_and(|r| r.passed);
        };
        if matches!(run.phase, Phase::Failed { .. })
            && s.present().and_then(|p| p.ui.screen.map(|sc| sc.id)).as_deref() == Some("mission/failed")
        {
            if retries == 2 {
                cmd(s, "mission quit");
                steps(s, 120);
                return false;
            }
            retries += 1;
            eprintln!("{key}: failed ({}); retrying", g.mission_status()["reason"]);
            s.step(&InputFrame {
                pad: PadState { held: pad::SOUTH, pressed: pad::SOUTH, ..Default::default() },
                ..Default::default()
            });
            continue;
        }
        let input = s.bot_input();
        s.step(&input);
    }
    false
}

/// The bot plays a street activity (`activity <id>`) until completion counts it (`unit`), or it
/// is over without. Whether it was counted.
fn play_street(s: &mut Sim, id: &str, unit: &str) -> bool {
    cmd(s, &format!("activity {id}"));
    for t in 0..60 * 1800 {
        let g = game(s);
        if g.activities.done.contains(unit) {
            return true;
        }
        if t > 60 && g.activities.vigilante.run.is_none() && g.activities.events.on.is_none() {
            return false;
        }
        let input = s.bot_input();
        s.step(&input);
    }
    false
}

#[test]
#[ignore = "long: the whole game by the bots"]
fn the_bots_play_the_whole_game_through() {
    gtf_game::register();
    let mut s = Sim::new("freeway/roam", 1).unwrap();
    steps(&mut s, 60);
    // (No random event by chance: each is played below, when it is its turn.)
    cmd(&mut s, "events off");
    let mut missed = Vec::new();
    // The story, in order (the heist the loud way), then the race and the hold-ups.
    for key in [
        "m1_repo_man",
        "m2_midlife",
        "m3_overdrive",
        "s1_rush_hour",
        "s2_heavy_metal",
        "s3_test_drive",
        "h0_board",
        "h1_freeway_job",
        "act_pretzel_run",
        "act_holdup_gas",
        "act_holdup_grind",
    ] {
        if key == "h1_freeway_job" {
            cmd(&mut s, "mission choose loud");
        }
        let ok = play_mission(&mut s, key);
        let g = game(&s);
        eprintln!("{key}: {} (completion {:.1}%)", if ok { "passed" } else { "MISSED" }, g.completion() * 100.0);
        if !ok {
            missed.push(key.to_string());
        }
        // Back to calm before the next (a hold-up leaves stars).
        cmd(&mut s, "wanted 0");
        steps(&mut s, 120);
    }
    // The street activities.
    for (id, unit) in
        [("vigilante", "vigilante"), ("purse_snatcher", "event:purse_snatcher"), ("getaway_needed", "event:getaway_needed")]
    {
        let ok = play_street(&mut s, id, unit);
        eprintln!("{id}: {} (completion {:.1}%)", if ok { "done" } else { "MISSED" }, game(&s).completion() * 100.0);
        if !ok {
            missed.push(id.to_string());
        }
        cmd(&mut s, "wanted 0");
        steps(&mut s, 120);
    }
    // The stunt jumps, a run at each from its run-up (the stunt bot), up to three tries.
    let d = activities::def().unwrap();
    for j in &d.jump {
        let id = format!("jump:{}", j.id);
        let mut tries = 0;
        while !game(&s).activities.done.contains(&id) && tries < 3 {
            tries += 1;
            cmd(&mut s, &format!("stunt {}", j.id));
            for _ in 0..60 * 50 {
                if game(&s).activities.jumps.run.is_none() {
                    break;
                }
                let input = s.bot_input();
                s.step(&input);
            }
        }
        let ok = game(&s).activities.done.contains(&id);
        eprintln!("{id}: {} after {tries} runs", if ok { "landed" } else { "MISSED" });
        if !ok {
            missed.push(id);
        }
    }
    // The Lost Shields: five metres out in front of each, then walk up to it.
    for sh in &d.shield {
        let id = format!("shield:{}", sh.id);
        let a = sh.yaw.to_radians();
        // Out of the car first (△, as a player gets out).
        for t in 0..240 {
            if game(&s).drive.vehicle().is_none() && !game(&s).drive.busy() {
                break;
            }
            let tap = if t % 30 == 0 { pad::NORTH } else { 0 };
            s.step(&InputFrame { pad: PadState { held: tap, pressed: tap, ..Default::default() }, ..Default::default() });
        }
        let pid = s.state.player.unwrap();
        s.set_position(pid, sh.pos() + Vec3::new(a.sin(), 0.0, -a.cos()) * 5.0 + Vec3::Y * 0.3);
        steps(&mut s, 240);
        for _ in 0..60 * 8 {
            if game(&s).activities.done.contains(&id) {
                break;
            }
            let me = s.state.entities.get(s.state.player.unwrap()).unwrap().pos;
            s.step(&InputFrame { move_dir: (flat(sh.pos()) - flat(me)).normalize_or_zero(), ..Default::default() });
        }
        if !game(&s).activities.done.contains(&id) {
            missed.push(id);
        }
    }
    let g = game(&s);
    let p = g.completion_parts();
    eprintln!(
        "completion {:.1}%: story {:.2}, activities {:.2}, shields {:.2}, gold {:.2} ({} of {} golds)",
        g.completion() * 100.0,
        p.story,
        p.activities,
        p.shields,
        p.gold,
        p.gold_count.0,
        p.gold_count.1
    );
    assert!(missed.is_empty(), "the bots missed: {missed:?}");
    assert!((p.story - 1.0).abs() < 1e-4 && (p.activities - 1.0).abs() < 1e-4 && (p.shields - 1.0).abs() < 1e-4, "{p:?}");
    let expected = 0.9 + 0.1 * p.gold_count.0 as f32 / p.gold_count.1.max(1) as f32;
    assert!(
        (g.completion() - expected).abs() < 1e-3,
        "completion {} = 90% from play + the golds earned {expected}",
        g.completion()
    );
}
