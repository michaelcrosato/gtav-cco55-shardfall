//! M6: the three protagonists. Switching keeps each one's cash, weapons and place; the switch
//! wheel picks who; the far switch flies the sky-cam; abilities slow the world, change the
//! damage, wash the screen and drain their meter.

use glam::Vec2;
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

fn press(bits: u32) -> InputFrame {
    InputFrame { pad: PadState { held: bits, pressed: bits, ..Default::default() }, ..Default::default() }
}

fn hold(bits: u32, stick: Vec2) -> InputFrame {
    InputFrame { pad: PadState { held: bits, stick_l: stick, ..Default::default() }, ..Default::default() }
}

/// Switches with the command and waits for the sky-cam to land.
fn switch_to(s: &mut Sim, who: &str) {
    s.step(&cmd(&format!("switch {who}")));
    for _ in 0..60 * 15 {
        if game(s).crew.switch.is_none() {
            break;
        }
        s.step(&InputFrame::default());
    }
    assert!(game(s).crew.switch.is_none(), "the switch to {who} landed");
    assert_eq!(game(s).crew.key(), who);
}

#[test]
fn switching_keeps_each_protagonists_things() {
    let mut s = sim("freeway/dev");
    s.run(30, &InputFrame::default());
    assert_eq!(game(&s).crew.key(), "dez");
    assert_eq!(game(&s).money, 350, "Dez starts with $350");
    s.step(&cmd("cash 1000"));
    s.step(&cmd("give c4r"));
    let c4r = 4;
    assert!(game(&s).crew.owns(c4r));
    let dez_feet = game(&s).player_point(&s);
    switch_to(&mut s, "mack");
    let g = game(&s);
    assert_eq!(g.money, 2500, "Mack's own cash");
    assert!(!g.crew.owns(c4r), "Mack never bought the carbine");
    assert!(g.crew.owns(3), "Mack starts with the pump shotgun");
    assert_eq!(g.crew.members[0].cash, 1350, "Dez's cash is kept");
    assert!(g.player_point(&s).distance(dez_feet) > 5.0, "Mack is somewhere else");
    // The player's body is Mack's now (dressed as him).
    let puppet = s.player().unwrap().character.as_ref().unwrap().puppet.clone().unwrap();
    assert_eq!(puppet.shirt, "#1f2d4d");
    switch_to(&mut s, "dez");
    let g = game(&s);
    assert_eq!(g.money, 1350);
    assert!(g.crew.owns(c4r));
    assert!(g.player_point(&s).distance(dez_feet) < 3.0, "switched back soon after, Dez is where he was left");
    assert_eq!(g.crew.switches, 2);
}

#[test]
fn the_switch_wheel_picks_and_the_sky_cam_flies() {
    let mut s = sim("freeway/dev");
    s.run(30, &InputFrame::default());
    // Hold D-pad down: the wheel opens, time slows.
    s.step(&press(pad::DOWN));
    s.run(20, &hold(pad::DOWN, Vec2::ZERO));
    assert!(game(&s).switch_wheel_open());
    assert_eq!(s.play_time_scale(), 0.25, "slow motion while the wheel is open");
    let p = s.present().unwrap();
    assert_eq!(p.ui.wheel.as_ref().map(|w| w.id.as_str()), Some("switch"));
    // Toward Mack (the second of three slots, a third of the way round), then let go.
    let a = std::f32::consts::TAU / 3.0;
    s.run(5, &hold(pad::DOWN, Vec2::new(a.sin(), a.cos())));
    s.step(&InputFrame::default());
    assert!(game(&s).crew.switch.is_some(), "the switch began");
    // The sky-cam rises high over the district, then lands on Mack.
    let start = game(&s).camera(&s).eye.y;
    let mut highest: f32 = 0.0;
    for _ in 0..60 * 10 {
        if game(&s).crew.switch.is_none() {
            break;
        }
        highest = highest.max(game(&s).camera(&s).eye.y);
        s.step(&InputFrame::default());
    }
    assert!(highest > start + 100.0, "the sky-cam went up ({start:.0} → {highest:.0} m)");
    assert_eq!(game(&s).crew.key(), "mack");
    assert!(game(&s).camera(&s).eye.y < start + 10.0, "and came back down behind Mack");
}

#[test]
fn abilities_slow_the_world_change_the_damage_and_drain() {
    let mut s = sim("freeway/dev");
    s.run(30, &InputFrame::default());
    switch_to(&mut s, "mack");
    s.step(&cmd("meter 1"));
    s.step(&press(pad::L3 | pad::R3));
    let g = game(&s);
    assert!(g.crew.ability, "Deadeye on");
    assert_eq!(s.play_time_scale(), 0.35, "Deadeye slows the world");
    assert_eq!(g.combat.power, 1.5, "and hits harder");
    assert!(s.present().unwrap().ui.overlay.tint.is_some(), "the screen is washed");
    s.run(120, &InputFrame::default());
    let meter = game(&s).crew.me().unwrap().meter;
    assert!(meter < 0.85 && meter > 0.6, "two seconds drain a fifth of the meter: {meter}");
    s.step(&press(pad::L3 | pad::R3));
    assert!(!game(&s).crew.ability, "the chord again turns it off");
    assert_eq!(s.play_time_scale(), 1.0);
    // Roxy's Overdrive: double damage, half taken.
    switch_to(&mut s, "roxy");
    s.step(&cmd("meter 1"));
    s.step(&press(pad::L3 | pad::R3));
    let g = game(&s);
    assert!(g.crew.ability);
    assert_eq!((g.combat.power, g.combat.tough), (2.0, 0.5));
    // Dez's Slipstream works only at the wheel.
    switch_to(&mut s, "dez");
    s.step(&cmd("meter 1"));
    s.step(&press(pad::L3 | pad::R3));
    assert!(!game(&s).crew.ability, "Slipstream needs a car");
    s.step(&cmd("drive bravura_stallion"));
    s.run(5, &InputFrame::default());
    s.step(&press(pad::L3 | pad::R3));
    let g = game(&s);
    assert!(g.crew.ability, "at the wheel it works");
    assert_eq!(s.play_time_scale(), 0.5);
    assert!(g.drive.grip > 0.3, "with more grip");
}
