//! M7 (PLAN §7.8, §6.10): saves. Everything a player earns survives a save into a fresh game —
//! the crew's cash and weapons, missions passed and the story's choices, every activity and Lost
//! Shield done (and so the completion), the clock and the weather — and a save from before a field
//! existed still loads, while a save from a newer game or a broken one is refused and changes
//! nothing. (The safehouse refusing with heat, the slot menu and Continue are in `economy.rs`.)

use gtf_game::Freeway;
use pav_core::{InputFrame, Sim};

fn roam(seed: u64) -> Sim {
    gtf_game::register();
    let mut s = Sim::new("freeway/roam", seed).unwrap();
    s.run(30, &InputFrame::default());
    s
}

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn game_mut(sim: &mut Sim) -> &mut Freeway {
    sim.state.play.as_mut().unwrap().get_mut::<Freeway>().unwrap()
}

fn cmd(c: &str) -> InputFrame {
    InputFrame { cmds: vec![c.into()], ..Default::default() }
}

/// A game well under way: cash, a rifle, two missions passed (one with its choice), activities
/// and shields done, the evening, rain. Returns its save.
fn a_game_under_way() -> (Sim, String) {
    let mut s = roam(1);
    s.step(&cmd("cash 1234"));
    s.step(&cmd("give marksman"));
    {
        let g = game_mut(&mut s);
        for key in ["m1_repo_man", "act_pretzel_run"] {
            g.missions.records.insert(key.into(), gtf_game::missions::Record { passed: true, ..Default::default() });
        }
        g.missions.story.insert("loud_way".into());
    }
    for id in ["jump:ditch", "jump:pier", "shield:pier_end", "shield:lookout", "vigilante", "event:purse_snatcher"] {
        s.step(&cmd(&format!("activity_done {id}")));
    }
    s.step(&cmd("time 19:40"));
    s.step(&cmd("weather rain"));
    s.run(10, &InputFrame::default());
    s.step(&cmd("save slot1"));
    let json = game(&s).save_req.clone().expect("a save for the host").json.to_string();
    (s, json)
}

/// A fresh game handed `json` by the host as `slot`, then loading it.
fn load_into_fresh(json: &str) -> Sim {
    let mut t = roam(2);
    t.step(&cmd(&format!("host.save slot1 {json}")));
    t.step(&cmd("load slot1"));
    t.run(10, &InputFrame::default());
    t
}

#[test]
fn everything_earned_survives_a_save_into_a_fresh_game() {
    let (s, json) = a_game_under_way();
    let before = game(&s);
    let t = load_into_fresh(&json);
    let after = game(&t);
    assert_eq!(after.money, before.money, "the cash");
    assert!(after.crew.members[after.crew.current].owned[5], "the marksman rifle");
    assert!(after.missions.passed("m1_repo_man") && after.missions.passed("act_pretzel_run"), "the missions passed");
    assert!(after.missions.story.contains("loud_way"), "the story's choice");
    assert_eq!(after.activities.done, before.activities.done, "the activities and shields done");
    assert!(after.activities.done.contains("shield:lookout") && after.activities.done.contains("vigilante"));
    let (c0, c1) = (before.completion(), after.completion());
    eprintln!("completion {:.1}% saved, {:.1}% loaded", c0 * 100.0, c1 * 100.0);
    assert!(c0 > 0.05 && (c0 - c1).abs() < 1e-6, "the completion: {c0} then {c1}");
    assert!((after.sky.hour - 19.67).abs() < 0.2, "the clock: {}", after.sky.hour);
    assert_eq!(after.sky.weather, "rain", "the weather");
    // The slot list's line reads the save.
    let line = gtf_game::progress::summary(&json).expect("a summary");
    eprintln!("slot line: {line}");
    assert!(line.contains(&format!("{:.0}%", c0 * 100.0)), "{line}");
}

#[test]
fn a_save_from_before_activities_existed_still_loads() {
    let (_, json) = a_game_under_way();
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v.as_object_mut().unwrap().remove("activities");
    let t = load_into_fresh(&v.to_string());
    let g = game(&t);
    assert!(g.missions.passed("m1_repo_man"), "the rest of the save is there");
    assert!(g.activities.done.is_empty(), "nothing done that the save did not know of");
}

#[test]
fn a_save_from_a_newer_game_or_a_broken_one_is_refused() {
    let (_, json) = a_game_under_way();
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v["version"] = serde_json::json!(9999);
    let newer = v.to_string();
    for (what, text) in [("newer", newer.as_str()), ("broken", "{\"version\": 1, \"members\": ")] {
        let mut t = roam(2);
        let money = game(&t).money;
        let mut play = t.state.play.take().unwrap();
        let err = play.get_mut::<Freeway>().unwrap().load_game(&mut t, text);
        t.state.play = Some(play);
        eprintln!("a {what} save: {:#}", err.as_ref().unwrap_err());
        assert!(err.is_err(), "a {what} save is refused");
        // Through the slot menu's command: nothing changes, and the player is told.
        t.step(&cmd(&format!("host.save slot3 {text}")));
        t.step(&cmd("load slot3"));
        t.run(5, &InputFrame::default());
        let g = game(&t);
        assert_eq!(g.money, money, "a {what} save changes nothing");
        assert!(g.activities.done.is_empty() && !g.missions.passed("m1_repo_man"));
        assert!(g.notes.iter().any(|n| n.title.contains("Can't load")), "the player is told");
    }
}
