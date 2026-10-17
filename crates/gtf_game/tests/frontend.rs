//! M1: the frontend runs on the controller and the pointer's commands alone (boot → title →
//! main menu → free roam ⇄ pause), the pause menu holds the world still, the weapon wheel slows
//! it down, settings are kept by the host, the idle timer is ready for attract mode, and every
//! UI gallery page builds — all of it replayable.

use glam::{Vec2, Vec3};
use gtf_game::Freeway;
use gtf_game::frontend::Page;
use pav_core::input::{PadState, pad};
use pav_core::present::{PlayFrame, ScreenBody};
use pav_core::{InputFrame, Sim};

fn sim(scene: &str) -> Sim {
    gtf_game::register();
    Sim::new(scene, 1).unwrap_or_else(|e| panic!("{scene}: {e:#}"))
}

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn page(sim: &Sim) -> Page {
    game(sim).front.page
}

fn press(bits: u32) -> InputFrame {
    InputFrame { pad: PadState { held: bits, pressed: bits, ..Default::default() }, activity: true, ..Default::default() }
}

fn hold(bits: u32, stick: Vec2) -> InputFrame {
    InputFrame { pad: PadState { held: bits, stick_l: stick, ..Default::default() }, activity: true, ..Default::default() }
}

fn cmd(c: &str) -> InputFrame {
    InputFrame { cmds: vec![c.into()], activity: true, ..Default::default() }
}

fn idle(sim: &mut Sim, ticks: u64) {
    sim.run(ticks, &InputFrame::default());
}

fn present(sim: &mut Sim) -> PlayFrame {
    (*sim.frame().present.expect("the game presents itself")).clone()
}

/// Down the main menu to `id` and accept it.
fn choose(sim: &mut Sim, id: &str) {
    for _ in 0..10 {
        let p = present(sim);
        let Some(ScreenBody::Title(t)) = p.ui.screen.map(|s| s.body) else { panic!("not on the main menu") };
        let menu = t.menu.unwrap();
        if menu.items[menu.focus].id == id {
            sim.step(&press(pad::SOUTH));
            return;
        }
        sim.step(&press(pad::DOWN));
        idle(sim, 1);
    }
    panic!("no '{id}' on the main menu");
}

#[test]
fn boot_title_menu_and_into_free_roam() {
    let mut s = sim("freeway");
    assert_eq!(page(&s), Page::Boot);
    idle(&mut s, 60 * 3);
    assert_eq!(page(&s), Page::Title, "the boot splash ends by itself");
    let p = present(&mut s);
    assert_eq!(p.ui.screen.as_ref().unwrap().title, "GRAND THEFT FREEWAY");
    assert!(p.camera.is_some() && p.view.is_some() && p.map.is_some());
    idle(&mut s, 30);
    s.step(&press(pad::SOUTH));
    assert_eq!(page(&s), Page::Main, "any button opens the menu");
    idle(&mut s, 2);
    // Continue is greyed out without a save, so the focus starts on it but skips it moving.
    choose(&mut s, "free_roam");
    assert_eq!(page(&s), Page::Play);
    let p = present(&mut s);
    assert!(p.ui.screen.is_none() && p.ui.hud.is_some(), "the HUD replaces the menus");
    assert!(p.app.mouse_look, "free roam holds the pointer for mouse look");
    assert_eq!(p.app.keys.as_ref().unwrap().name, "foot");
}

#[test]
fn the_pointer_picks_menu_items() {
    let mut s = sim("freeway");
    idle(&mut s, 60 * 3);
    s.step(&cmd("ui.accept title"));
    assert_eq!(page(&s), Page::Main, "a click on the title opens the menu");
    s.step(&cmd("ui.focus about"));
    assert_eq!(game(&s).front.main, 4);
    s.step(&cmd("ui.accept settings"));
    assert_eq!(page(&s), Page::Settings);
    s.step(&cmd("ui.focus tab:2"));
    assert_eq!(game(&s).front.settings_tab, 2);
    s.step(&press(pad::EAST));
    assert_eq!(page(&s), Page::Main, "○ backs out");
}

#[test]
fn the_pause_menu_holds_the_world_still() {
    let mut s = sim("freeway/roam");
    idle(&mut s, 30);
    // Walk, then pause mid-stride.
    s.run(30, &hold(0, Vec2::new(0.0, 1.0)));
    s.step(&press(pad::START));
    assert_eq!(page(&s), Page::Pause);
    assert!(s.world_paused());
    let (pos, time) = (s.player().unwrap().pos, game(&s).time);
    s.run(120, &hold(0, Vec2::new(1.0, 0.0)));
    assert_eq!(s.player().unwrap().pos, pos, "nobody moves while paused");
    assert_eq!(game(&s).time, time, "game time stops");
    assert!(game(&s).ui_time > 2.0, "the UI clock keeps running");
    // L1/R1 switch tabs; the map tab is a picture of the lot.
    s.step(&press(pad::R1));
    assert_eq!(game(&s).front.pause_tab, 0, "GAME → MAP wraps around");
    let p = present(&mut s);
    assert_eq!(p.ui.screen.as_ref().unwrap().id, "pause/map");
    s.step(&press(pad::START));
    assert_eq!(page(&s), Page::Play);
    s.run(10, &hold(0, Vec2::new(0.0, 1.0)));
    assert_ne!(s.player().unwrap().pos, pos, "walks again after resuming");
}

#[test]
fn the_weapon_wheel_slows_time_and_picks_a_weapon() {
    let mut s = sim("freeway/roam");
    idle(&mut s, 30);
    s.step(&press(pad::L1));
    s.run(20, &hold(pad::L1, Vec2::ZERO));
    assert!(game(&s).wheel_open());
    assert_eq!(s.play_time_scale(), 0.25, "slow motion while the wheel is open");
    assert!(present(&mut s).ui.wheel.is_some());
    // Push the stick right (slot 2: the SMG), then let go.
    s.run(5, &hold(pad::L1, Vec2::new(1.0, 0.0)));
    s.step(&InputFrame::default());
    assert!(!game(&s).wheel_open());
    assert_eq!(s.play_time_scale(), 1.0);
    assert_eq!(game(&s).weapon, 2);
    let p = present(&mut s);
    assert_eq!(p.ui.hud.unwrap().weapon.unwrap().name, "SMG");
    assert_eq!(p.ui.toasts.last().unwrap().title, "SMG");
    // A tap swaps back to the last weapon.
    s.step(&press(pad::L1));
    s.step(&InputFrame::default());
    assert_eq!(game(&s).weapon, 0, "back to fists");
}

#[test]
fn settings_are_kept_by_the_host_and_handed_back() {
    let mut s = sim("freeway");
    idle(&mut s, 60 * 3);
    s.step(&cmd("ui.accept title"));
    s.step(&cmd("ui.accept settings"));
    // AUDIO: master volume one step down.
    s.step(&press(pad::LEFT));
    let p = present(&mut s);
    let req = p.app.settings.expect("the changed settings go to the host");
    assert_eq!(req.serial, 1);
    let kept: serde_json::Value = serde_json::from_str(&req.json).unwrap();
    assert_eq!(kept["master"], 0.7);
    // A new run gets them back from the host.
    let mut again = sim("freeway");
    again.step(&cmd(&format!("host.settings {}", req.json)));
    assert_eq!(game(&again).settings.master, 0.7);
    // The platform: touch screens hide the frame-rate readout.
    again.step(&cmd("host.platform web touch"));
    assert!(game(&again).web && game(&again).touch);
    assert!(!present(&mut again).ui.perf);
}

#[test]
fn ten_idle_seconds_on_the_title_are_attract_mode() {
    let mut s = sim("freeway");
    idle(&mut s, 60 * 3);
    assert_eq!(page(&s), Page::Title);
    idle(&mut s, 60 * 9);
    assert!(!game(&s).front.attract_due);
    // Any device activity resets the timer.
    s.step(&InputFrame { activity: true, ..Default::default() });
    idle(&mut s, 60 * 9);
    assert!(!game(&s).front.attract_due);
    idle(&mut s, 61);
    assert!(game(&s).front.attract_due, "10 s without input");
}

#[test]
fn every_gallery_page_builds_and_reads_as_json() {
    let mut s = sim("freeway/gallery");
    assert_eq!(page(&s), Page::Gallery);
    let mut ids = Vec::new();
    for i in 0..gtf_game::gallery::PAGES.len() {
        let p = present(&mut s);
        let json = serde_json::to_string(&p.ui).unwrap();
        assert!(json.len() > 100, "page {i} is empty: {json}");
        ids.push(json.len());
        s.step(&press(pad::R1));
    }
    assert_eq!(game(&s).front.gallery, 0, "pages wrap");
    let hud = gtf_game::gallery::page(0, 1.0, Vec3::ZERO).hud.unwrap();
    assert!(hud.minimap.is_some() && hud.wanted.is_some() && hud.crosshair.is_some());
    s.step(&press(pad::EAST));
    assert_eq!(page(&s), Page::Title, "○ leaves the gallery");
}

#[test]
fn the_camera_turns_with_the_right_stick_and_the_mouse() {
    let mut s = sim("freeway/roam");
    idle(&mut s, 10);
    let yaw = game(&s).cam.yaw;
    s.run(30, &InputFrame { pad: PadState { stick_r: Vec2::new(1.0, 0.0), ..Default::default() }, ..Default::default() });
    let turned = game(&s).cam.yaw;
    assert!((turned - yaw).abs() > 0.5, "the stick turns the camera: {yaw} -> {turned}");
    s.step(&InputFrame { mouse_delta: Vec2::new(-200.0, 0.0), ..Default::default() });
    assert!(game(&s).cam.yaw < turned, "the mouse too");
    let c = present(&mut s).camera.unwrap();
    assert!(c.eye.distance(c.target) > 3.0);
}

#[test]
fn a_frontend_session_replays_exactly() {
    let mut s = sim("freeway");
    let mut script = vec![InputFrame::default(); 200];
    script.push(press(pad::SOUTH));
    script.extend(vec![InputFrame::default(); 5]);
    script.push(cmd("ui.accept free_roam"));
    script.extend((0..90).map(|_| hold(0, Vec2::new(0.3, 1.0))));
    script.push(press(pad::L1));
    script.extend((0..30).map(|_| hold(pad::L1, Vec2::new(0.0, -1.0))));
    script.push(InputFrame::default());
    script.push(press(pad::START));
    script.extend(vec![InputFrame::default(); 30]);
    for f in &script {
        s.step(f);
    }
    let hash = s.state_hash();
    let mut again = sim("freeway");
    for f in s.recording.iter() {
        again.step(f);
    }
    assert_eq!(again.state_hash(), hash);
    assert_eq!(page(&again), Page::Pause);
}

#[test]
fn menus_make_their_sounds() {
    let mut s = sim("freeway");
    idle(&mut s, 60 * 3);
    s.drain_events();
    s.step(&press(pad::SOUTH));
    let cues = |s: &mut Sim| -> Vec<String> {
        s.drain_events()
            .into_iter()
            .filter_map(|e| match e {
                pav_core::SimEvent::Sound { cue, .. } => Some(cue),
                _ => None,
            })
            .collect()
    };
    assert_eq!(cues(&mut s), ["ui.accept"], "the title opens the menu with a click");
    s.step(&press(pad::DOWN));
    assert_eq!(cues(&mut s), ["ui.move"]);
    s.step(&press(pad::EAST));
    assert_eq!(cues(&mut s), ["ui.back"]);
}
