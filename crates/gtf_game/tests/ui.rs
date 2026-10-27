//! M7 (PLAN §7.8): the UI model in the states that matter — the HUD on foot, with money and stars,
//! in a car; the pause menu's five tabs; the weapon, switch and radio wheels; the phone; and the
//! banners (MISSION FAILED, WASTED, BUSTED). Each state's `UiModel` is also written as JSON under
//! the target's test folder (`ui/<state>.json`) for a look at what the renderer is handed.

use glam::Vec2;
use gtf_game::Freeway;
use gtf_game::fate::Fate;
use gtf_game::frontend::{PAUSE_TABS, Page};
use pav_core::input::{PadState, pad};
use pav_core::present::{BannerKind, ScreenBody, UiModel};
use pav_core::{InputFrame, Sim};

fn roam(seed: u64) -> Sim {
    gtf_game::register();
    let mut s = Sim::new("freeway/roam", seed).unwrap();
    s.step(&cmd("time 13:00"));
    s.run(60, &InputFrame::default());
    s
}

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn game_mut(sim: &mut Sim) -> &mut Freeway {
    sim.state.play.as_mut().unwrap().get_mut::<Freeway>().unwrap()
}

fn cmd(c: &str) -> InputFrame {
    InputFrame { cmds: vec![c.into()], activity: true, ..Default::default() }
}

fn press(bits: u32) -> InputFrame {
    InputFrame { pad: PadState { held: bits, pressed: bits, ..Default::default() }, activity: true, ..Default::default() }
}

fn hold(bits: u32, stick: Vec2) -> InputFrame {
    InputFrame { pad: PadState { held: bits, stick_l: stick, ..Default::default() }, activity: true, ..Default::default() }
}

/// The UI model this tick, written to `ui/<name>.json`.
fn ui(sim: &mut Sim, name: &str) -> UiModel {
    let ui = sim.frame().present.expect("the game presents itself").ui.clone();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("ui");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&ui).unwrap()).unwrap();
    ui
}

#[test]
fn the_hud_reads_the_game_on_foot_with_money_and_stars_and_in_a_car() {
    let mut s = roam(1);
    let u = ui(&mut s, "hud_foot");
    let hud = u.hud.expect("a HUD in free roam");
    let me = s.player().unwrap().pos;
    let map = hud.minimap.expect("the radar");
    assert!(map.player.distance(Vec2::new(me.x, me.z)) < 0.5, "the radar's player is the player: {:?} {me:?}", map.player);
    assert!(map.radius > 50.0 && !map.blips.is_empty(), "the radar shows the town round the player");
    let v = hud.vitals.expect("health and armour");
    assert!(v.health > 0.99 && (0.0..=1.0).contains(&v.armor) && (0.0..=1.0).contains(&v.ability));
    assert_eq!(hud.money.as_ref().map(|m| m.amount), Some(game(&s).money), "the cash");
    assert!(hud.wanted.as_ref().is_none_or(|w| w.stars == 0), "no stars");
    assert!(hud.weapon.is_none(), "bare fists show no weapon panel");
    assert!(hud.objective.as_deref().is_some_and(|o| o.starts_with("Explore")), "free roam's tip: {:?}", hud.objective);
    assert!(hud.vehicle.is_none() && hud.crosshair.is_none());
    assert!(u.screen.is_none() && u.wheel.is_none() && u.phone.is_none() && u.banners.is_empty());

    // Money in shows beside the cash; two stars flash while they search.
    let cash = game(&s).money;
    s.step(&cmd("cash 500"));
    s.step(&cmd("wanted 2"));
    s.run(2, &InputFrame::default());
    let hud = ui(&mut s, "hud_money_stars").hud.unwrap();
    let money = hud.money.unwrap();
    assert_eq!((money.amount, money.delta), (cash + 500, 500), "+$500 beside the cash");
    let w = hud.wanted.expect("the stars");
    assert_eq!((w.stars, w.max), (2, 5));
    assert!(w.flashing == game(&s).heat.searching, "the stars flash while they search");

    // In a car: its name (and the station) for a few seconds; the radar zooms out.
    s.step(&cmd("wanted 0"));
    s.step(&cmd("drive okami_kaze"));
    s.run(30, &InputFrame::default());
    let hud = ui(&mut s, "hud_car").hud.unwrap();
    let car = hud.vehicle.expect("the car's card");
    eprintln!("in a car: {} on {:?} ({:?})", car.name, car.station, car.song);
    assert!(!car.name.is_empty());
    assert!(hud.minimap.unwrap().radius >= map.radius, "the radar sees further from a car");
}

#[test]
fn the_pause_menu_has_its_five_tabs() {
    let mut s = roam(1);
    s.step(&press(pad::START));
    s.run(2, &InputFrame::default());
    assert_eq!(game(&s).front.page, Page::Pause);
    for (n, tab) in PAUSE_TABS.iter().enumerate() {
        s.step(&cmd(&format!("ui.focus tab:{n}")));
        s.run(2, &InputFrame::default());
        let id = format!("pause/{}", tab.to_ascii_lowercase());
        let u = ui(&mut s, &id.replace('/', "_"));
        let screen = u.screen.expect("the pause menu");
        assert_eq!(screen.id, id);
        assert_eq!(screen.tab, n);
        assert_eq!(screen.tabs, PAUSE_TABS.map(String::from).to_vec());
        assert!(!screen.hints.is_empty(), "{id}: the button hints");
        let body = serde_json::to_string(&screen.body).unwrap();
        eprintln!("{id}: {} bytes", body.len());
        assert!(body.len() > 60, "{id} is empty: {body}");
        match (*tab, &screen.body) {
            ("MAP", b) => assert!(matches!(b, ScreenBody::Map(_)), "{id}: {body}"),
            ("STATS", _) => assert!(body.contains("Completion") || body.contains("COMPLETION"), "{id}: the completion card"),
            _ => {}
        }
    }
    s.step(&press(pad::START));
    assert_eq!(game(&s).front.page, Page::Play);
}

#[test]
fn the_wheels_show_what_is_owned_and_picked() {
    let mut s = roam(1);
    // Weapons: Dez owns fists and the pistol; the SMG once given.
    s.step(&cmd("give buzz"));
    s.step(&press(pad::L1));
    s.run(20, &hold(pad::L1, Vec2::ZERO));
    s.run(5, &hold(pad::L1, Vec2::new(1.0, 0.0)));
    let w = ui(&mut s, "wheel_weapons").wheel.expect("the weapon wheel");
    assert_eq!(w.id, "weapons");
    let owned: Vec<bool> = w.slots.iter().map(|s| s.enabled).collect();
    eprintln!("weapon slots {:?}, focus {:?}: {}", w.slots.iter().map(|s| &s.label).collect::<Vec<_>>(), w.focus, w.center);
    for (i, o) in owned.iter().enumerate() {
        assert_eq!(*o, game(&s).crew.owns(i), "slot {i}");
    }
    assert!(owned[0] && owned[2], "fists and the SMG");
    assert_eq!(w.focus, Some(2), "the stick right is the SMG");
    assert!(w.center.starts_with(&w.slots[2].label));
    s.step(&InputFrame::default());
    assert!(ui(&mut s, "wheel_closed").wheel.is_none());

    // The switch wheel: the three of them.
    s.step(&press(pad::DOWN));
    s.run(20, &hold(pad::DOWN, Vec2::ZERO));
    let w = ui(&mut s, "wheel_switch").wheel.expect("the switch wheel");
    assert_eq!(w.id, "switch");
    assert_eq!(w.slots.len(), game(&s).crew.members.len());
    s.run(1, &InputFrame::default());

    // The radio wheel, in a car: every station and Off.
    s.step(&cmd("drive okami_kaze"));
    s.run(30, &InputFrame::default());
    s.step(&press(pad::LEFT));
    s.run(30, &hold(pad::LEFT, Vec2::ZERO));
    let w = ui(&mut s, "wheel_radio").wheel.expect("the radio wheel");
    assert_eq!(w.id, "radio");
    let stations = gtf_game::radio::def().unwrap().station.len();
    assert_eq!(w.slots.len(), stations + 1, "the stations and Off");
    assert_eq!(w.slots.last().unwrap().label, "Off");
}

#[test]
fn the_phone_opens_on_its_apps() {
    let mut s = roam(1);
    s.step(&cmd("phone home"));
    s.run(2, &InputFrame::default());
    let p = ui(&mut s, "phone_home").phone.expect("the phone");
    assert_eq!(p.app, "home");
    assert_eq!(p.tiles.len(), gtf_game::phone::APPS.len());
    assert_eq!(p.clock, gtf_game::daytime::format_time(game(&s).sky.hour));
    for app in ["contacts", "messages", "jobs"] {
        s.step(&cmd(&format!("phone {app}")));
        s.run(2, &InputFrame::default());
        let p = ui(&mut s, &format!("phone_{app}")).phone.expect("the phone");
        eprintln!("{app}: {} rows: {:?}", p.rows.len(), p.rows.iter().take(3).map(|r| &r.title).collect::<Vec<_>>());
        assert_eq!(p.app, app);
        assert!(!p.rows.is_empty() || app == "messages", "{app} lists something");
    }
    s.step(&cmd("phone close"));
    s.run(2, &InputFrame::default());
    assert!(ui(&mut s, "phone_closed").phone.is_none());
}

#[test]
fn banners_announce_mission_failed_wasted_and_busted() {
    let mut s = roam(1);
    s.step(&cmd("mission start m1_repo_man"));
    s.run(60 * 8, &InputFrame::default());
    assert!(game(&s).missions.run.is_some(), "the mission is on");
    s.step(&cmd("mission fail"));
    s.run(30, &InputFrame::default());
    let u = ui(&mut s, "banner_failed");
    let b = u.banners.iter().find(|b| b.kind == BannerKind::Failed).expect("MISSION FAILED");
    assert_eq!(b.title, "MISSION FAILED");
    s.step(&cmd("mission quit"));
    s.run(60 * 6, &InputFrame::default());

    let mut s = roam(2);
    game_mut(&mut s).combat.health = 0.0;
    s.run(30, &InputFrame::default());
    let u = ui(&mut s, "banner_wasted");
    let b = u.banners.iter().find(|b| b.kind == BannerKind::Wasted).expect("WASTED");
    assert_eq!(b.title, "WASTED");
    assert!(b.subtitle.as_deref().is_some_and(|t| t.contains("Hospital")), "{:?}", b.subtitle);
    assert!(u.overlay.desaturate > 0.0, "the picture drains");

    // (The arrest itself is `heat.rs`'s; here only what it shows.)
    let mut s = roam(3);
    game_mut(&mut s).fate = Fate::Busted { t: 0.0 };
    s.run(30, &InputFrame::default());
    let u = ui(&mut s, "banner_busted");
    let b = u.banners.iter().find(|b| b.kind == BannerKind::Busted).expect("BUSTED");
    assert_eq!(b.title, "BUSTED");
    assert!(b.subtitle.as_deref().is_some_and(|t| t.contains("Fine")), "{:?}", b.subtitle);
}
