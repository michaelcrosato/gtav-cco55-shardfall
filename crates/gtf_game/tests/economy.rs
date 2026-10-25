//! M6: the economy and saving. Bang Bang Bros. sells guns, rounds and armour; Paloma Customs
//! resprays and repairs; Gas-N-Grab's snacks heal; prices come out of the protagonist's cash and
//! nothing is sold without it. A save holds every protagonist's things, missions, the phone, the
//! clock; loading it into a fresh game puts it all back; Continue loads the newest.

use gtf_game::Freeway;
use gtf_game::shops::Counter;
use pav_core::input::{PadState, pad};
use pav_core::{InputFrame, Sim};

fn sim(scene: &str) -> Sim {
    gtf_game::register();
    Sim::new(scene, 1).unwrap_or_else(|e| panic!("{scene}: {e:#}"))
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

fn tap(s: &mut Sim, bits: u32) {
    s.step(&InputFrame { pad: PadState { held: bits, pressed: bits, ..Default::default() }, ..Default::default() });
    s.step(&InputFrame::default());
}

/// The player to `c`'s door; returns the prompt the HUD shows there.
fn walk_up(s: &mut Sim, c: Counter) -> Option<String> {
    let door = game(s).counter_door(c).expect("the shop's door");
    let pid = s.state.player.unwrap();
    s.set_position(pid, door + glam::Vec3::Y * 0.05);
    s.run(5, &InputFrame::default());
    let ui = s.present().unwrap().ui;
    ui.prompts.iter().find(|p| p.button == pav_core::present::PadButton::Right).map(|p| p.label.clone())
}

fn menu_ids(s: &Sim) -> Vec<String> {
    let screen = s.present().unwrap().ui.screen.expect("a menu is up");
    let pav_core::present::ScreenBody::Menu(m) = screen.body else { panic!("a menu") };
    m.items.iter().map(|i| i.id.clone()).collect()
}

#[test]
fn the_gun_store_sells_guns_rounds_and_armour() {
    let mut s = sim("freeway/roam");
    s.run(30, &InputFrame::default());
    assert_eq!(walk_up(&mut s, Counter::Gun).as_deref(), Some("Bang Bang Bros."));
    tap(&mut s, pad::RIGHT);
    assert!(game(&s).shop.is_some(), "the counter's menu is up");
    assert!(s.world_paused(), "the world holds still while shopping");
    assert_eq!(s.present().unwrap().ui.screen.unwrap().id, "shop/gun");
    let ids = menu_ids(&s);
    assert!(ids.contains(&"gun:buzz".into()) && ids.contains(&"ammo:p9".into()) && ids.contains(&"armor".into()), "{ids:?}");
    // Dez's $350 does not buy an SMG.
    let before = game(&s).money;
    s.step(&cmd("ui.accept gun:buzz"));
    assert_eq!(game(&s).money, before, "no sale without the cash");
    assert!(!game(&s).crew.owns(2));
    // With the cash: the SMG, a box of its rounds, armour.
    s.step(&cmd("cash 5000"));
    let cash = game(&s).money;
    s.step(&cmd("ui.accept gun:buzz"));
    assert!(game(&s).crew.owns(2), "the SMG is his");
    assert_eq!(game(&s).money, cash - 1250);
    let reserve = game(&s).combat.reserve[2];
    s.step(&cmd("ui.accept ammo:buzz"));
    assert_eq!(game(&s).combat.reserve[2], reserve + 60);
    s.step(&cmd("ui.accept armor"));
    assert_eq!(game(&s).combat.armor, gtf_game::combat::MAX_ARMOR);
    assert_eq!(game(&s).money, cash - 1250 - 60 - 500);
    tap(&mut s, pad::EAST);
    assert!(game(&s).shop.is_none());
    assert!(!s.world_paused());
}

#[test]
fn snacks_heal_and_customs_resprays_and_repairs() {
    let mut s = sim("freeway/roam");
    s.run(30, &InputFrame::default());
    s.step(&cmd("cash 2000"));
    game_mut(&mut s).combat.health = 60.0;
    assert_eq!(walk_up(&mut s, Counter::Store).as_deref(), Some("Gas-N-Grab"));
    tap(&mut s, pad::RIGHT);
    s.step(&cmd("ui.accept snack:3"));
    assert!(game(&s).combat.health > 130.0, "the burrito heals: {}", game(&s).combat.health);
    tap(&mut s, pad::EAST);
    // In a car at Paloma Customs: a dented car comes out repaired and repainted.
    s.step(&cmd("drive fenwick_commuter"));
    s.run(5, &InputFrame::default());
    let car = game(&s).drive.vehicle().expect("in the car");
    s.damage_vehicle(car, 900.0);
    let door = game(&s).counter_door(Counter::Mods).unwrap();
    s.set_position(car, door + glam::Vec3::Y * 0.6);
    s.run(30, &InputFrame::default());
    assert_eq!(game(&s).counter, Some(Counter::Mods), "the garage door is right there");
    tap(&mut s, pad::RIGHT);
    let cash = game(&s).money;
    s.step(&cmd("ui.accept repair"));
    assert!(s.vehicle_status(car).unwrap().health > 0.99, "repaired");
    assert!(game(&s).money < cash, "for a price");
    let resprays = game(&s).resprays;
    s.step(&cmd("ui.accept respray"));
    assert_eq!(game(&s).resprays, resprays + 1);
}

#[test]
fn a_save_round_trips_into_a_fresh_game() {
    let mut s = sim("freeway/dev");
    s.run(30, &InputFrame::default());
    s.step(&cmd("cash 777"));
    s.step(&cmd("give marksman"));
    s.step(&cmd("time 21:30"));
    game_mut(&mut s).phone.receive("mack", "Remember the plan.");
    game_mut(&mut s)
        .missions
        .records
        .insert("dev_test".into(), gtf_game::missions::Record { passed: true, ..Default::default() });
    s.step(&cmd("switch roxy"));
    s.run(60 * 8, &InputFrame::default());
    assert_eq!(game(&s).crew.key(), "roxy");
    let roxy_at = game(&s).player_point(&s);
    s.step(&cmd("save slot2"));
    let req = game(&s).save_req.clone().expect("a save request for the host");
    assert_eq!(req.slot, "slot2");
    // A fresh game, handed the save by the host, loads it.
    let mut t = sim("freeway/dev");
    t.step(&cmd(&format!("host.save slot2 {}", req.json)));
    t.run(10, &InputFrame::default());
    assert_eq!(game(&t).crew.key(), "dez", "a fresh game starts as Dez");
    t.step(&cmd("load slot2"));
    t.run(10, &InputFrame::default());
    let g = game(&t);
    assert_eq!(g.crew.key(), "roxy", "the save plays Roxy");
    assert_eq!(g.crew.members[0].cash, 350 + 777, "Dez's cash");
    assert!(g.crew.members[0].owned[5], "Dez's marksman rifle");
    assert!(g.missions.passed("dev_test"));
    assert_eq!(g.phone.messages.first().map(|m| m.body.as_str()), Some("Remember the plan."));
    assert!((g.sky.hour - 21.5).abs() < 0.2, "the clock: {}", g.sky.hour);
    assert!(g.player_point(&t).distance(roxy_at) < 2.0, "Roxy where she was");
    assert_eq!(t.player().unwrap().character.as_ref().unwrap().puppet.as_ref().unwrap().shirt, "#c8242c", "dressed as Roxy");
}

#[test]
fn continue_loads_the_newest_save() {
    let mut s = sim("freeway/dev");
    s.run(30, &InputFrame::default());
    s.step(&cmd("cash 100"));
    s.step(&cmd("save slot1"));
    let old = game(&s).save_req.clone().unwrap().json;
    s.step(&cmd("cash 400"));
    s.step(&cmd("save slot3"));
    let new = game(&s).save_req.clone().unwrap().json;
    let mut t = sim("freeway");
    t.step(&cmd(&format!("host.save slot1 {old}")));
    t.step(&cmd(&format!("host.save slot3 {new}")));
    // Through the boot splash and the title to the main menu; Continue is first.
    t.run(60 * 3, &InputFrame::default());
    tap(&mut t, pad::SOUTH);
    t.run(30, &InputFrame::default());
    assert_eq!(game(&t).front.page, gtf_game::frontend::Page::Main);
    tap(&mut t, pad::SOUTH);
    // (Through the loading card.)
    t.run(60 * 3, &InputFrame::default());
    assert_eq!(game(&t).front.page, gtf_game::frontend::Page::Play);
    assert_eq!(game(&t).money, 350 + 100 + 400, "the newer save (slot 3) was loaded");
}

#[test]
fn the_safehouse_saves_into_a_slot() {
    let mut s = sim("freeway/dev");
    s.run(30, &InputFrame::default());
    let city = gtf_game::world::dev_city().unwrap();
    let home = gtf_game::anchor::foot(&city, &gtf_game::anchor::Anchor::parse("spot:dez_home").unwrap()).unwrap().0;
    let pid = s.state.player.unwrap();
    s.set_position(pid, home + glam::Vec3::Y * 0.05);
    s.run(5, &InputFrame::default());
    assert_eq!(game(&s).counter, Some(Counter::Save), "Dez's place");
    tap(&mut s, pad::RIGHT);
    assert_eq!(s.present().unwrap().ui.screen.unwrap().id, "shop/save");
    assert_eq!(menu_ids(&s), ["slot1", "slot2", "slot3"]);
    tap(&mut s, pad::DOWN);
    tap(&mut s, pad::SOUTH);
    assert_eq!(game(&s).save_req.as_ref().map(|r| r.slot.as_str()), Some("slot2"));
    assert!(game(&s).shop.is_none(), "saved, the menu goes");
    // Wanted, the bed refuses.
    s.step(&cmd("wanted 1"));
    tap(&mut s, pad::RIGHT);
    assert!(game(&s).shop.is_none(), "no saving with the police after you");
}
