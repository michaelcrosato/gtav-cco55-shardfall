//! M7 (PLAN §7.8, D3): the player on foot. The stick walks (tilted a little) and runs, ✕ held
//! sprints until the stamina bar is spent and it refills after a rest, □ jumps, running into a
//! low wall vaults it, a long fall hurts (a short one does not), L3 crouches (and a jump stands
//! up), and deep water is swum in. Speeds are the controller's own (`Character::vel`),
//! whatever is in the way.

use glam::{IVec2, Vec2, Vec3};
use gtf_game::Freeway;
use gtf_game::player::{RUN, SPRINT, WALK};
use pav_core::input::{PadState, pad};
use pav_core::{InputFrame, Sim};

fn roam() -> Sim {
    gtf_game::register();
    let mut s = Sim::new("freeway/roam", 1).unwrap();
    s.run(30, &InputFrame::default());
    s
}

/// Free roam with the player in the middle of open ground (a plaza, the beach: 7 × 7 walk tiles
/// of it, clear of buildings), where a few seconds of running meet nothing.
fn on_open_ground() -> Sim {
    let mut s = roam();
    let city = gtf_game::world::city().unwrap();
    let walk = &city.walk;
    let open = |t: IVec2| walk.get(t) == pav_world::walk::Walk::Open;
    let t = (3..walk.h - 3)
        .flat_map(|z| (3..walk.w - 3).map(move |x| IVec2::new(x, z)))
        .find(|t| (-3..=3).all(|dz| (-3..=3).all(|dx| open(*t + IVec2::new(dx, dz)))))
        .expect("open ground");
    let c = walk.centre(t);
    let id = s.state.player.unwrap();
    s.set_position(id, Vec3::new(c.x, city.ground_y(c) + 1.0, c.y));
    s.run(30, &InputFrame::default());
    s
}

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

/// The stick pushed east (world space, `tilt` of the way), with `held` buttons (and `pressed` on
/// this tick).
fn stick(tilt: f32, held: u32, pressed: u32) -> InputFrame {
    InputFrame {
        move_dir: Vec2::new(tilt, 0.0),
        pad: PadState { held: held | pressed, pressed, ..Default::default() },
        ..Default::default()
    }
}

/// The player's speed over the ground, as the controller drives it.
fn speed(sim: &Sim) -> f32 {
    let v = sim.player().and_then(|e| e.character.as_ref()).map(|c| c.vel).unwrap_or(Vec3::ZERO);
    Vec2::new(v.x, v.z).length()
}

fn hold(sim: &mut Sim, f: &InputFrame, ticks: usize) {
    for _ in 0..ticks {
        sim.step(f);
    }
}

#[test]
fn the_stick_walks_and_runs_and_cross_sprints_until_the_stamina_is_spent() {
    let mut s = on_open_ground();
    hold(&mut s, &stick(0.35, 0, 0), 45);
    let walk = speed(&s);
    let want = WALK * 0.35 / gtf_game::player::WALK_TILT;
    assert!((walk - want).abs() < 0.15, "a light tilt walks, as fast as it is tilted: {walk} (want {want:.2})");
    hold(&mut s, &stick(0.5, 0, 0), 45);
    let edge = speed(&s);
    assert!(edge > 1.3 && edge <= WALK + 0.05, "near the walk band's edge, a full walk: {edge}");
    hold(&mut s, &stick(1.0, 0, 0), 45);
    let run = speed(&s);
    assert!((run - RUN).abs() < 0.3, "the stick pushed runs: {run}");
    hold(&mut s, &stick(1.0, pad::SOUTH, 0), 45);
    let sprint = speed(&s);
    eprintln!("walk {walk:.2} (edge {edge:.2}), run {run:.2}, sprint {sprint:.2} m/s");
    assert!((sprint - SPRINT).abs() < 0.4, "✕ held sprints: {sprint}");

    // A full bar lasts `Foot::capacity(stamina stat)` seconds, then the sprint drops to a run.
    let stat = {
        let g = game(&s);
        g.crew.members[g.crew.current].stats.stamina
    };
    let cap = gtf_game::player::Foot::capacity(stat);
    let mut t = 0.75;
    loop {
        s.step(&stick(1.0, pad::SOUTH, 0));
        t += 1.0 / 60.0;
        if !game(&s).foot.sprinting || t > cap + 5.0 {
            break;
        }
    }
    eprintln!("sprinted {t:.1} s of a {cap:.1} s bar");
    assert!((t - cap).abs() < 1.0, "the bar ran out after {t:.1} s (capacity {cap:.1} s)");
    assert!(game(&s).foot.stamina < 0.01);
    hold(&mut s, &stick(1.0, pad::SOUTH, 0), 30);
    assert!(!game(&s).foot.sprinting, "spent, ✕ only runs");

    // A rest refills it (after a second, two seconds of sprint a second).
    hold(&mut s, &InputFrame::default(), 60 * 4);
    let back = game(&s).foot.stamina;
    eprintln!("after 4 s of rest the bar is at {back:.2}");
    assert!(back > (6.0 / cap).min(1.0) - 0.05, "the bar refills: {back}");
}

#[test]
fn square_jumps_and_lands() {
    let mut s = on_open_ground();
    let ground = s.feet(s.state.player.unwrap()).unwrap().y;
    s.step(&stick(0.0, 0, pad::WEST));
    let mut top = ground;
    let mut landed = false;
    for t in 0..120 {
        s.step(&InputFrame::default());
        let y = s.feet(s.state.player.unwrap()).unwrap().y;
        top = top.max(y);
        let grounded = s.player().and_then(|e| e.character.as_ref()).is_some_and(|c| c.grounded);
        if t > 10 && grounded {
            landed = true;
            break;
        }
    }
    eprintln!("jumped {:.2} m", top - ground);
    assert!(top - ground > 0.2, "□ leaves the ground: {:.2} m", top - ground);
    assert!(landed, "and comes down again");
}

#[test]
fn running_into_a_low_wall_vaults_it() {
    let mut s = on_open_ground();
    let feet = s.feet(s.state.player.unwrap()).unwrap();
    // A wall 0.9 m high, 0.4 m thick and 3 m wide, 4 m east.
    let half = Vec3::new(0.2, 0.45, 1.5);
    s.spawn(
        pav_core::entity::Spawn::new("low_wall", feet + Vec3::new(4.0, 0.45, 0.0))
            .visual(pav_core::shape::Visual::new(pav_core::shape::Shape::Box { half }, pav_core::Color::hex("#888888")))
            .body(pav_core::entity::BodyKind::Fixed),
    );
    s.run(5, &InputFrame::default());
    // Run east at it (no button): over it and on.
    let mut top = feet.y;
    for _ in 0..150 {
        s.step(&stick(1.0, 0, 0));
        top = top.max(s.feet(s.state.player.unwrap()).unwrap().y);
    }
    let x = s.feet(s.state.player.unwrap()).unwrap().x;
    eprintln!("crossed to {:.1} m east (the wall at 4), up {:.2} m", x - feet.x, top - feet.y);
    assert!(x - feet.x > 4.6, "past the wall: {:.1} m", x - feet.x);
    assert!(top - feet.y > 0.8, "up over it: {:.2} m", top - feet.y);
}

#[test]
fn a_long_fall_hurts_and_a_short_one_does_not() {
    let mut s = on_open_ground();
    let feet = s.feet(s.state.player.unwrap()).unwrap();
    let health = |s: &Sim| game(s).combat.health + game(s).combat.armor;
    let drop = |s: &mut Sim, h: f32| {
        let id = s.state.player.unwrap();
        s.set_position(id, feet + Vec3::Y * (h + 1.0));
        let before = game(s).combat.health + game(s).combat.armor;
        s.run(150, &InputFrame::default());
        before - (game(s).combat.health + game(s).combat.armor)
    };
    let short = drop(&mut s, 2.0);
    let long = drop(&mut s, 12.0);
    eprintln!("2 m: -{short:.0}, 12 m: -{long:.0} (left {:.0})", health(&s));
    assert!(short < 0.5, "a 2 m drop costs nothing: {short}");
    assert!(long > 30.0, "a 12 m drop hurts: {long}");
}

#[test]
fn l3_crouches_slower_and_a_jump_stands_up() {
    let mut s = on_open_ground();
    s.step(&stick(0.0, 0, pad::L3));
    s.step(&InputFrame::default());
    assert!(game(&s).foot.crouched, "L3 crouches");
    hold(&mut s, &stick(1.0, 0, 0), 45);
    let crouched = speed(&s);
    let posture = s.player().and_then(|e| e.character.as_ref()).map(|c| format!("{:?}", c.posture)).unwrap_or_default();
    eprintln!("crouched: {crouched:.2} m/s ({posture})");
    assert!(crouched < RUN * 0.8, "slower crouched: {crouched}");
    assert!(posture.contains("Crouch"), "the body crouches: {posture}");
    s.step(&stick(0.0, 0, pad::WEST));
    assert!(!game(&s).foot.crouched, "a jump stands up");
}

#[test]
fn deep_water_is_swum_in() {
    let mut s = roam();
    let city = gtf_game::world::city().unwrap();
    // A water cell with water all round it (the open sea, off the beach).
    let deep = (0..city.grid.h)
        .flat_map(|z| (0..city.grid.w).map(move |x| IVec2::new(x, z)))
        .find(|c| (-1..=1).all(|dz| (-1..=1).all(|dx| city.grid.zone(*c + IVec2::new(dx, dz)) == Some("water"))))
        .expect("open water");
    let at = city.cell_centre(deep);
    let id = s.state.player.unwrap();
    s.set_position(id, Vec3::new(at.x, 1.0, at.y));
    hold(&mut s, &InputFrame::default(), 120);
    let (swimming, depth) = s.player().and_then(|e| e.character.as_ref()).map(|c| (c.swimming, c.water_depth)).unwrap();
    eprintln!("at ({:.0}, {:.0}): swimming {swimming}, depth {depth:.2} m", at.x, at.y);
    assert!(swimming, "in deep water the player swims (depth {depth:.2} m)");
    // And keeps afloat while swimming on.
    hold(&mut s, &stick(1.0, 0, 0), 120);
    let feet = s.feet(id).unwrap();
    assert!(feet.y > -3.0, "afloat: feet at {:.2}", feet.y);
    assert!(speed(&s) > 0.5, "swimming moves");
}
