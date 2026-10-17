//! M0: the game is registered from its own crate, every sub-scene builds and steps, and its state
//! survives snapshot files, rewind and replays like the rest of the simulation.

use pav_core::{InputFrame, Sim, SimState};

fn sim(scene: &str, seed: u64) -> Sim {
    gtf_game::register();
    Sim::new(scene, seed).unwrap_or_else(|e| panic!("{scene}: {e:#}"))
}

#[test]
fn every_freeway_scene_builds_and_steps() {
    for scene in ["freeway", "freeway/roam", "freeway/dev", "freeway/attract"] {
        let mut s = sim(scene, 1);
        assert_eq!(s.play().map(|g| g.key()), Some(gtf_game::KEY), "{scene}");
        assert!(s.state.player.is_some(), "{scene}: no player");
        s.run(120, &InputFrame::default());
        assert!(s.frame().hud.lines[0].starts_with("Grand Theft Freeway"), "{scene}");
    }
    assert!(pav_core::scenes::names().iter().any(|n| n == gtf_game::KEY));
}

#[test]
fn every_reference_view_loads_with_its_camera() {
    let views = gtf_game::views::all().unwrap();
    assert!(views.len() >= 6, "the six reference views of PLAN §7.2");
    for v in views {
        let scene = format!("freeway/view/{}", v.name);
        let mut s = sim(&scene, 1);
        s.run(10, &InputFrame::default());
        let cam = s.frame().present.as_ref().and_then(|p| p.camera).unwrap_or_else(|| panic!("{scene}: no camera"));
        assert!((cam.eye - v.eye).length() < 1e-3, "{scene}: the camera sits at the view's eye");
    }
    assert!(Sim::new("freeway/view/nowhere", 1).is_err(), "an unknown view is an error");
}

#[test]
fn snapshots_rewind_and_replays_are_exact() {
    let mut s = sim("freeway/roam", 3);
    let walk = InputFrame { move_dir: glam::Vec2::new(0.6, -0.4), ..Default::default() };
    for _ in 0..240 {
        s.step(&walk);
    }
    let mut bytes = Vec::new();
    ciborium::into_writer(&s.snapshot(), &mut bytes).unwrap();
    let state: SimState = ciborium::from_reader(bytes.as_slice()).unwrap();
    let mut copy = sim("freeway/roam", 3);
    copy.restore(state);
    assert_eq!(copy.state_hash(), s.state_hash());
    for _ in 0..240 {
        s.step(&walk);
        copy.step(&walk);
    }
    assert_eq!(copy.state_hash(), s.state_hash());

    let (tick, hash) = (s.state.tick, s.state_hash());
    assert!(s.rewind_to(tick - 90));
    assert!(s.rewind_to(tick));
    assert_eq!(s.state_hash(), hash);

    let mut again = sim("freeway/roam", 3);
    for f in s.recording.iter() {
        again.step(f);
    }
    assert_eq!(again.state_hash(), hash);
}
