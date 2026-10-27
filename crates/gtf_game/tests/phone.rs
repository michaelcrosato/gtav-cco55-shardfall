//! M6: the phone. D-pad up brings it up; the D-pad walks its tiles and ✕ opens them; the camera
//! catches whoever is in the middle of the frame; texts arrive with a notification; the map app
//! sets a waypoint.

use gtf_game::Freeway;
use pav_core::input::{PadState, pad};
use pav_core::{InputFrame, Sim};

fn sim(scene: &str) -> Sim {
    gtf_game::register();
    Sim::new(scene, 1).unwrap_or_else(|e| panic!("{scene}: {e:#}"))
}

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn cmd(c: &str) -> InputFrame {
    InputFrame { cmds: vec![c.into()], ..Default::default() }
}

/// A press, then a tick with nothing (so the next press is a new one).
fn tap(s: &mut Sim, bits: u32) {
    s.step(&InputFrame { pad: PadState { held: bits, pressed: bits, ..Default::default() }, ..Default::default() });
    s.step(&InputFrame::default());
}

#[test]
fn the_phone_comes_up_and_its_tiles_open() {
    let mut s = sim("freeway/dev");
    s.run(30, &InputFrame::default());
    tap(&mut s, pad::UP);
    let g = game(&s);
    assert!(g.phone.open);
    let ui = s.present().unwrap().ui;
    let phone = ui.phone.expect("the phone is drawn");
    assert_eq!(phone.app, "home");
    assert_eq!(phone.tiles.len(), 9, "a 3 × 3 grid of tiles");
    // Right to Messages, open it, back out; down a row to Map.
    tap(&mut s, pad::RIGHT);
    assert_eq!(game(&s).phone.focus, 1);
    tap(&mut s, pad::SOUTH);
    assert_eq!(game(&s).phone.app, "messages");
    tap(&mut s, pad::EAST);
    assert_eq!(game(&s).phone.app, "home");
    tap(&mut s, pad::DOWN);
    assert_eq!(game(&s).phone.focus, 4, "down a row: Jobs");
    tap(&mut s, pad::LEFT);
    tap(&mut s, pad::SOUTH);
    assert_eq!(game(&s).phone.app, "map");
    let rows = s.present().unwrap().ui.phone.unwrap().rows;
    assert!(!rows.is_empty(), "places to go");
    tap(&mut s, pad::SOUTH);
    assert!(game(&s).front.waypoint.is_some(), "a waypoint to the nearest place");
    assert!(!game(&s).phone.open, "the phone goes away");
    // ○ on the home screen puts it away.
    tap(&mut s, pad::UP);
    tap(&mut s, pad::EAST);
    assert!(!game(&s).phone.open);
}

#[test]
fn the_camera_catches_who_is_in_the_middle_of_the_frame() {
    let mut s = sim("freeway/dev");
    s.run(30, &InputFrame::default());
    // A stranger 12 m along the view.
    s.step(&cmd("target 12"));
    s.run(10, &InputFrame::default());
    let stranger = s.state.entities.iter().find(|e| e.name == "target").map(|e| e.id).expect("the stranger");
    tap(&mut s, pad::UP);
    tap(&mut s, pad::RIGHT);
    tap(&mut s, pad::RIGHT);
    tap(&mut s, pad::SOUTH);
    assert_eq!(game(&s).phone.app, "camera");
    assert!(game(&s).phone.camera());
    let view = game(&s).camera(&s);
    assert!((view.eye.y - game(&s).player_point(&s).y) > 1.4, "the view is from the player's eyes");
    assert!(s.present().unwrap().ui.hud.is_none_or(|h| h.opacity == 0.0), "no HUD over the viewfinder");
    tap(&mut s, pad::SOUTH);
    let g = game(&s);
    assert_eq!(g.phone.photos, 1);
    assert_eq!(g.phone.gallery.len(), 1);
    assert!(g.phone.subjects.contains(&stranger), "the stranger is in the photo: {:?}", g.phone.subjects);
    // Turned away, the next photo misses them.
    s.run(60, &InputFrame { pad: PadState { stick_r: glam::Vec2::new(1.0, 0.0), ..Default::default() }, ..Default::default() });
    tap(&mut s, pad::SOUTH);
    assert_eq!(game(&s).phone.photos, 2);
    assert!(!game(&s).phone.subjects.contains(&stranger));
    // Back out of the camera and away.
    tap(&mut s, pad::EAST);
    tap(&mut s, pad::EAST);
    assert!(!game(&s).phone.open);
}

#[test]
fn texts_arrive_with_a_notification() {
    let mut s = sim("freeway/mission/dev_test2");
    s.run(10, &InputFrame::default());
    let unread = game(&s).phone.unread();
    // A mission's text action: the phone gets it, a toast shows.
    let mut g = s.play().unwrap().get::<Freeway>().unwrap().clone();
    g.phone.receive("roxy", "Test text");
    assert_eq!(g.phone.unread(), unread + 1);
    assert_eq!(g.phone.messages[0].from, "roxy");
}
