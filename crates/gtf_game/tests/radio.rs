//! M7 radio (PLAN.md §5.6.13, §5.12): in a car on SPNN the anchor's headlines come up as
//! subtitles while the host babbles them, and a traffic report says where the player's police
//! chase is (worked out once, as it goes on air); between songs a music station's DJ talks and
//! the radio card shows their line; at two stars the scanner names a code and where the police
//! last saw the player (a radio-voice loop and a subtitle); the radio sits 6 dB under the
//! effects, ducks 8 dB under a mission's dialogue and 12 dB in the pause menu, and keeps
//! playing through the pause on its own clock; listening changes nothing else in the game.

use gtf_game::Freeway;
use gtf_game::radio::{self, Words};
use pav_core::input::{PadState, pad};
use pav_core::present::PlayFrame;
use pav_core::radio::SegmentKind;
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

fn present(sim: &mut Sim) -> PlayFrame {
    (*sim.frame().present.expect("the game presents itself")).clone()
}

/// Free roam, sitting in a car, its card gone.
fn in_a_car() -> Sim {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", 2).unwrap();
    cmd(&mut sim, "time 13:00");
    steps(&mut sim, 60);
    cmd(&mut sim, "drive fenwick_commuter");
    steps(&mut sim, 260);
    assert!(game(&sim).drive.vehicle().is_some(), "in the car");
    sim
}

/// Moves the radio's clock to the first moment from now that `want` holds.
fn skip_to(sim: &mut Sim, want: impl Fn(&Freeway, f64) -> bool) -> f64 {
    let from = game(sim).radio.clock;
    let t = (0..72_000).map(|k| from + k as f64 * 0.05).find(|t| want(game(sim), *t)).expect("it comes on air");
    game_mut(sim).radio.clock = t;
    t
}

/// Whether `text` reads as `template` with its `{...}`s filled in.
fn fits(template: &str, text: &str) -> bool {
    let mut rest = text;
    for (i, piece) in template.split(['{', '}']).enumerate() {
        if i % 2 == 1 || piece.is_empty() {
            continue;
        }
        match rest.find(piece) {
            Some(k) => rest = &rest[k + piece.len()..],
            None => return false,
        }
    }
    true
}

#[test]
fn spnn_reads_the_news_with_subtitles() {
    let mut sim = in_a_car();
    cmd(&mut sim, "radio spnn");
    let def = radio::def().unwrap();
    let talk = &def.talk["spnn"];
    skip_to(&mut sim, |g, t| g.radio.on_air(&def, t).is_some_and(|a| talk.headlines.contains(&a.text)));
    steps(&mut sim, 20);
    let p = present(&mut sim);
    let sub = p.ui.subtitle.expect("the headline as a subtitle");
    eprintln!("{sub:?}");
    assert!(talk.headlines.contains(&sub.text), "{sub:?}");
    assert_eq!(sub.speaker.as_deref(), Some("BROCK HARDWELL · SPNN"));
    // The host babbles it: a voice on the radio's air now, and the radio 6 dB under the
    // effects.
    let m = p.audio.music.expect("the radio plays");
    assert_eq!(m.station, "spnn");
    assert!(m.segments.iter().any(|s| s.kind == SegmentKind::Voice && (s.start..s.start + s.seconds).contains(&m.offset)));
    assert!((m.volume - game(&sim).settings.music * 0.501).abs() < 1e-3, "{}", m.volume);
    assert_eq!(p.audio.ducking, 0.0);
    // The subtitles setting turns them off.
    game_mut(&mut sim).settings.subtitles = false;
    steps(&mut sim, 1);
    assert!(present(&mut sim).ui.subtitle.is_none());
}

#[test]
fn spnn_traffic_says_where_the_chase_is() {
    let mut sim = in_a_car();
    cmd(&mut sim, "radio spnn");
    cmd(&mut sim, "wanted 3");
    steps(&mut sim, 5);
    let def = radio::def().unwrap();
    let at = skip_to(&mut sim, |g, t| g.radio.pending(&def, t).is_some_and(|p| p.1 == Words::Traffic));
    steps(&mut sim, 3);
    let said = game(&sim).radio.said.last().cloned().expect("the report was worked out as it went on air");
    let news = game(&sim).news(&sim, &def);
    eprintln!("{} ({news:?})", said.text);
    let tr = &def.talk["spnn"].traffic;
    assert!(tr.pursuit.iter().chain(&tr.search).any(|l| fits(l, &said.text)), "a wanted report: {}", said.text);
    assert!(!tr.quiet.iter().any(|l| fits(l, &said.text)));
    assert!(said.text.contains(&news.player) || said.text.contains(&news.zone), "names the place: {}", said.text);
    let on = game(&sim).radio.on_air(&def, at + 0.2).expect("on air");
    assert_eq!((on.speaker.as_str(), on.text.as_str()), ("Skye Lane", said.text.as_str()));
    // Clearing the heat mid-report does not change what was said.
    cmd(&mut sim, "wanted 0");
    steps(&mut sim, 5);
    assert_eq!(game(&sim).radio.on_air(&def, at + 0.3).unwrap().text, said.text);
    // With nobody wanted, the next report is ordinary traffic.
    let t2 = skip_to(&mut sim, |g, t| t > at + 20.0 && g.radio.pending(&def, t).is_some_and(|p| p.1 == Words::Traffic));
    steps(&mut sim, 3);
    let quiet = game(&sim).radio.on_air(&def, t2 + 0.2).unwrap().text;
    assert!(tr.quiet.iter().any(|l| fits(l, &quiet)), "{quiet}");
}

#[test]
fn a_dj_talks_between_songs_on_the_radio_card() {
    let mut sim = in_a_car();
    cmd(&mut sim, "radio freeway_fm");
    let def = radio::def().unwrap();
    skip_to(&mut sim, |g, t| g.radio.on_air(&def, t).is_some_and(|a| a.dj));
    steps(&mut sim, 20);
    let p = present(&mut sim);
    let card = p.ui.hud.and_then(|h| h.vehicle).expect("the radio card");
    eprintln!("{card:?}");
    assert_eq!(card.name, "TARMAC TINA");
    assert_eq!(card.station.as_deref(), Some("FREEWAY FM 104.04"));
    let line = card.song.expect("the DJ's line");
    assert!(def.dj["freeway_fm"].lines.iter().any(|l| fits(l, &line)), "{line}");
    assert!(p.ui.subtitle.is_none(), "the DJ is on the card, not in the subtitles");
    let m = p.audio.music.expect("the radio plays");
    assert!(m.offset > m.song.seconds, "between songs");
    assert!(m.segments.iter().any(|s| s.kind == SegmentKind::Voice && (s.start..s.start + s.seconds).contains(&m.offset)));
    // When the next song starts, its card shows again.
    skip_to(&mut sim, |g, t| g.radio.now(&def, t).is_some_and(|(_, _, off)| off > 0.5 && off < 1.0));
    steps(&mut sim, 1);
    let card = present(&mut sim).ui.hud.and_then(|h| h.vehicle).expect("the new song's card");
    assert!(card.song.unwrap().contains(" — "));
}

#[test]
fn the_scanner_names_a_code_and_where_the_police_saw_the_player() {
    let mut sim = in_a_car();
    cmd(&mut sim, "wanted 2");
    let mut seen = None;
    for _ in 0..300 {
        sim.step(&InputFrame::default());
        if let Some(c) = game(&sim).chatter.as_ref().filter(|c| c.age > 0.5) {
            seen = Some(c.clone());
            break;
        }
    }
    let c = seen.expect("dispatch talks within five seconds of two stars");
    let def = radio::def().unwrap();
    let news = game(&sim).news(&sim, &def);
    eprintln!("{} ({news:?})", c.text);
    assert!(def.scanner.lines.iter().chain(&def.scanner.air).any(|l| fits(l, &c.text)), "{}", c.text);
    assert!(c.text.contains(&news.zone), "where the police last saw the player: {} / {}", c.text, news.zone);
    let p = present(&mut sim);
    let sub = p.ui.subtitle.expect("the scanner's words");
    assert_eq!(sub.speaker.as_deref(), Some("SPPD DISPATCH"));
    assert_eq!(sub.text, c.text);
    assert!(p.audio.loops.iter().any(|l| l.cue == "voice.radio"), "the dispatcher's babble");
    // When the line ends the babble stops.
    let left = ((c.seconds - c.age) * 60.0) as usize + 2;
    steps(&mut sim, left);
    assert!(!present(&mut sim).audio.loops.iter().any(|l| l.cue == "voice.radio"));
}

#[test]
fn the_radio_ducks_under_dialogue_and_in_menus_and_plays_on_when_paused() {
    // Free roam: no duck; the pause menu: 12 dB, and the station plays on (its clock is real
    // time, so the host's song never has to jump back).
    let mut sim = in_a_car();
    cmd(&mut sim, "radio westside_99");
    steps(&mut sim, 2);
    assert_eq!(present(&mut sim).audio.ducking, 0.0);
    sim.step(&InputFrame { pad: PadState { held: pad::START, pressed: pad::START, ..Default::default() }, ..Default::default() });
    assert!(sim.world_paused());
    let (clock, time) = (game(&sim).radio.clock, game(&sim).time);
    steps(&mut sim, 60);
    let p = present(&mut sim);
    assert_eq!(p.audio.ducking, 12.0);
    assert!(p.audio.music.is_some(), "the radio plays on under the menu");
    assert_eq!(game(&sim).time, time, "game time stops");
    assert!((game(&sim).radio.clock - clock - 1.0).abs() < 1e-6, "the station keeps going: {}", game(&sim).radio.clock - clock);
    // A mission's dialogue: 8 dB while its subtitles show.
    gtf_game::register();
    let mut sim = Sim::new("freeway/mission/dev_test", 1).unwrap();
    cmd(&mut sim, "radio freeway_fm");
    let mut ducked = None;
    for _ in 0..900 {
        sim.step(&InputFrame::default());
        let p = present(&mut sim);
        if p.ui.subtitle.is_some() {
            ducked = Some(p.audio.ducking);
            break;
        }
        assert_eq!(p.audio.ducking, 0.0, "no duck before the dialogue");
    }
    assert_eq!(ducked, Some(8.0), "the cutscene's lines duck the radio");
}

#[test]
fn listening_changes_nothing_else() {
    // The same run with the radio on SPNN and with it off, both with two stars of scanner
    // chatter: the sim's random stream and the player end up the same.
    let run = |station: &str| {
        let mut sim = in_a_car();
        cmd(&mut sim, &format!("radio {station}"));
        cmd(&mut sim, "wanted 2");
        steps(&mut sim, 600);
        (serde_json::to_string(&sim.state.rng).unwrap(), sim.player().unwrap().pos, game(&sim).chatters)
    };
    let (on, off) = (run("spnn"), run("off"));
    assert!(on.2 > 0, "the scanner talked");
    assert_eq!(on, off);
}
