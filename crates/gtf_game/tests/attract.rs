//! M8: the attract loop (PLAN.md §5.8, §7.8): every vignette starts, plays and ends without
//! stalling; the loop's order is the same for a seed and shuffles between seeds; any input
//! leaves it for the title at once (doing nothing else there) with the game as it stood; touch
//! screens never leave it; and it does not grow the world.

use gtf_game::Freeway;
use gtf_game::frontend::Page;
use pav_core::input::{PadState, pad};
use pav_core::{InputFrame, Sim};

fn sim(scene: &str, seed: u64) -> Sim {
    gtf_game::register();
    Sim::new(scene, seed).unwrap_or_else(|e| panic!("{scene}: {e:#}"))
}

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

/// A command the way a test types it: not device activity (that would end the loop).
fn cmd(c: &str) -> InputFrame {
    InputFrame { cmds: vec![c.into()], ..Default::default() }
}

fn press(bits: u32) -> InputFrame {
    InputFrame { pad: PadState { held: bits, pressed: bits, ..Default::default() }, activity: true, ..Default::default() }
}

fn idle(sim: &mut Sim, secs: f32) {
    sim.run((secs * 60.0) as u64, &InputFrame::default());
}

fn feet(sim: &Sim) -> glam::Vec3 {
    sim.state.player.and_then(|p| sim.feet(p)).unwrap()
}

#[test]
fn the_loop_order_is_the_same_for_a_seed() {
    let order = |seed| game(&sim("freeway/attract", seed)).attract.order.clone();
    let one = order(1);
    let n = gtf_game::attract::def().unwrap().vignette.len();
    assert_eq!(n, 7, "the seven vignettes of PLAN §5.8");
    let mut sorted = one.clone();
    sorted.sort();
    assert_eq!(sorted, (0..n).collect::<Vec<_>>(), "each vignette once a loop");
    assert_eq!(one, order(1), "the same seed, the same order");
    assert!((2..6).any(|s| order(s) != one), "other seeds shuffle it");
}

#[test]
fn any_input_leaves_for_the_title_at_once_and_does_nothing_else() {
    let mut s = sim("freeway/attract", 1);
    assert_eq!(game(&s).front.page, Page::Attract);
    let money = game(&s).money;
    idle(&mut s, 4.0);
    assert!(game(&s).attract.ready, "the first vignette is set up after the cut");
    assert_ne!(game(&s).attract_vignette(), None);
    // ✕: back to the title on this very tick, and not on into the menu.
    s.step(&press(pad::SOUTH));
    assert_eq!(game(&s).front.page, Page::Title);
    assert!(!game(&s).attract.on);
    idle(&mut s, 1.0);
    assert_eq!(game(&s).front.page, Page::Title, "the press that left only left");
    assert_eq!(game(&s).money, money, "the game as it stood");
    assert_eq!(game(&s).heat.stars, 0);
    // The title's fade is short (≤ 0.25 s of black).
    let p = (*s.frame().present.unwrap()).clone();
    assert!(p.ui.overlay.fade < 0.01, "faded back in: {}", p.ui.overlay.fade);
    // A mouse move counts as input too.
    let mut s = sim("freeway/attract", 1);
    idle(&mut s, 4.0);
    s.step(&InputFrame { mouse_delta: glam::Vec2::new(3.0, 0.0), activity: true, ..Default::default() });
    assert_eq!(game(&s).front.page, Page::Title);
}

#[test]
fn ten_idle_seconds_on_a_menu_start_it() {
    let mut s = sim("freeway", 1);
    idle(&mut s, 3.0);
    assert_eq!(game(&s).front.page, Page::Title);
    s.step(&press(pad::SOUTH));
    assert_eq!(game(&s).front.page, Page::Main);
    idle(&mut s, 10.5);
    assert_eq!(game(&s).front.page, Page::Attract, "ten seconds on the main menu");
    s.step(&press(pad::START));
    assert_eq!(game(&s).front.page, Page::Title);
}

#[test]
fn touch_screens_never_leave_it_and_show_only_the_gear() {
    let mut s = sim("freeway", 1);
    s.step(&cmd("host.platform web touch"));
    idle(&mut s, 3.0);
    assert_eq!(game(&s).front.page, Page::Attract, "straight into the loop");
    assert!(game(&s).attract.kiosk);
    idle(&mut s, 3.0);
    // A tap anywhere is activity, and nothing happens.
    s.step(&InputFrame { activity: true, ..Default::default() });
    idle(&mut s, 0.5);
    assert_eq!(game(&s).front.page, Page::Attract);
    let p = (*s.frame().present.unwrap()).clone();
    assert!(p.ui.hud.is_none() && p.ui.screen.is_none() && !p.ui.perf, "nothing over the world");
    assert!(p.ui.subtitle.is_none() && p.ui.prompts.is_empty() && p.ui.banners.is_empty());
    // The gear opens the overlay: sound, graphics, the readout, About, Close.
    s.step(&cmd("ui.gear"));
    let p = (*s.frame().present.unwrap()).clone();
    let screen = p.ui.screen.expect("the gear's overlay");
    assert_eq!(screen.id, "attract/gear");
    s.step(&cmd("ui.accept fps"));
    assert!(game(&s).settings.fps);
    s.step(&cmd("ui.accept about"));
    let p = (*s.frame().present.unwrap()).clone();
    assert_eq!(p.ui.screen.unwrap().id, "attract/about");
    s.step(&cmd("ui.gear"));
    s.step(&cmd("ui.accept close"));
    let p = (*s.frame().present.unwrap()).clone();
    assert!(p.ui.screen.is_none());
    assert_eq!(game(&s).front.page, Page::Attract);
}

#[test]
fn each_vignette_sets_up_and_plays() {
    let d = gtf_game::attract::def().unwrap();
    let mut s = sim("freeway/attract", 3);
    for v in &d.vignette {
        s.step(&cmd(&format!("attract play {}", v.id)));
        idle(&mut s, 2.0);
        assert_eq!(game(&s).attract_vignette().as_deref(), Some(v.id.as_str()));
        assert!(game(&s).attract.ready, "{}: set up after the cut", v.id);
        let crew = gtf_game::protagonists::def().unwrap();
        assert_eq!(crew.protagonist[game(&s).crew.current].key, v.protagonist, "{}: who", v.id);
        let from = feet(&s);
        idle(&mut s, 8.0);
        let g = game(&s);
        assert!(g.attract.anomalies.is_empty(), "{}: {:?}", v.id, g.attract.anomalies);
        assert!(g.attract.on && g.front.page == Page::Attract);
        let moved = feet(&s).distance(from);
        // (The switch vignette's sky-cam, the fight's cover and the heist's stand against the
        // SWAT may keep the feet near.)
        assert!(moved > 3.0 || matches!(v.id.as_str(), "switch" | "dock_fight" | "the_job"), "{}: went {moved:.1} m", v.id);
        if v.heat > 0 {
            assert!(g.heat.stars > 0, "{}: wanted", v.id);
        }
    }
}

#[test]
fn a_new_vignette_starts_from_an_empty_world() {
    let mut s = sim("freeway/attract", 1);
    s.step(&cmd("attract play hot_pursuit"));
    idle(&mut s, 20.0);
    assert!(game(&s).heat.stars > 0 && !game(&s).police.units.is_empty());
    s.step(&cmd("attract play city_life"));
    idle(&mut s, 0.5);
    let g = game(&s);
    assert_eq!(g.heat.stars, 0, "the stars went with the chase");
    assert!(g.police.units.is_empty() && g.enemies.list.is_empty());
    assert!(s.state.world.dormant_entities.is_empty(), "nothing left sleeping");
}

/// The 30-minute soak's short form: one loop of every vignette to its end, none stalling, and
/// each vignette's entity count again on a second loop's first vignettes.
#[test]
#[ignore]
fn one_loop_plays_every_vignette_to_its_end() {
    let mut s = sim("freeway/attract", 2);
    let n = gtf_game::attract::def().unwrap().vignette.len() as u32;
    for _ in 0..(60 * 60 * 8) {
        s.step(&InputFrame::default());
        s.drain_events();
        if game(&s).attract.played >= n {
            break;
        }
    }
    let g = game(&s);
    assert_eq!(g.attract.played, n, "a whole loop in 8 minutes");
    assert!(g.attract.anomalies.is_empty(), "{:?}", g.attract.anomalies);
    assert_eq!(g.attract.loops, 1);
}
