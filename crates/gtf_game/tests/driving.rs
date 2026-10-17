//! M3: the player and vehicles — △ walks to the driver's door and gets in, R2 drives, △ stops
//! and gets out; a locked car's window is smashed (its alarm sounds); a driver is pulled out;
//! held △ at speed bails out and the car rolls on; the helicopter climbs on R2; the chase
//! camera sits behind and widens with speed.

use glam::Vec3;
use gtf_game::Freeway;
use gtf_game::drive::Phase;
use pav_core::entity::EntityId;
use pav_core::input::pad;
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

fn steps(sim: &mut Sim, n: usize, f: &InputFrame) {
    for _ in 0..n {
        sim.step(f);
    }
}

/// The newest vehicle called `key`.
fn newest(sim: &Sim, key: &str) -> EntityId {
    sim.state.entities.iter().filter(|e| e.vehicle.is_some() && e.name == key).map(|e| e.id).max().unwrap()
}

fn press(button: u32) -> InputFrame {
    let mut f = InputFrame::default();
    f.pad.held = button;
    f.pad.pressed = button;
    f
}

fn hold(button: u32) -> InputFrame {
    let mut f = InputFrame::default();
    f.pad.held = button;
    f
}

fn throttle(r2: f32) -> InputFrame {
    let mut f = InputFrame::default();
    f.pad.r2 = r2;
    f
}

fn riding(sim: &Sim) -> Option<EntityId> {
    sim.player().and_then(|p| p.character.as_ref()).and_then(|c| c.riding)
}

/// Presses △ and steps until the player sits in a car (or `max` ticks): the ticks it took.
fn get_in(sim: &mut Sim, max: usize) -> usize {
    sim.step(&press(pad::NORTH));
    for t in 1..max {
        if matches!(game(sim).drive.phase, Phase::In { .. }) {
            return t;
        }
        sim.step(&InputFrame::default());
    }
    max
}

/// Puts the player a couple of metres from `car`'s driver's door.
fn walk_up(sim: &mut Sim, car: EntityId) {
    let c = gtf_game::drive::Car::read(sim, car).unwrap();
    let door = c.door_point();
    let out = (door - c.feet()).normalize_or_zero();
    sim.set_position(sim.state.player.unwrap(), door + out * 1.5 + c.forward() * 0.5 + Vec3::Y * 0.1);
    steps(sim, 10, &InputFrame::default());
}

fn roam() -> Sim {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", 3).unwrap();
    cmd(&mut sim, "time 12:00");
    steps(&mut sim, 90, &InputFrame::default());
    sim
}

#[test]
fn getting_in_driving_and_getting_out() {
    let mut sim = roam();
    cmd(&mut sim, "car fenwick_commuter");
    let car = newest(&sim, "fenwick_commuter");
    steps(&mut sim, 30, &InputFrame::default());
    walk_up(&mut sim, car);
    let ticks = get_in(&mut sim, 180);
    eprintln!("in after {ticks} ticks ({:.2} s)", ticks as f32 / 60.0);
    assert_eq!(riding(&sim), Some(car), "seated in the car");
    assert!(ticks as f32 / 60.0 <= 2.0, "walk, open, sit, shut within two seconds: {ticks} ticks");
    // The radio came on and the car's card shows.
    assert!(game(&sim).radio.on.is_some(), "the radio plays in a car");
    // R2 for three seconds.
    steps(&mut sim, 180, &throttle(1.0));
    let st = sim.vehicle_status(car).unwrap();
    eprintln!("after 3 s of throttle: {:.1} km/h, gear {}", st.kmh, st.gear);
    assert!(st.kmh > 30.0, "it drives: {} km/h", st.kmh);
    // The chase camera: behind the car, wider with speed.
    let c = gtf_game::drive::Car::read(&sim, car).unwrap();
    let cam = game(&sim).chase.intent(&gtf_game::camera::Ride {
        center: c.center,
        yaw: c.yaw,
        vel: c.vel,
        half: c.half,
        drift: 0.0,
        heli: false,
    });
    assert!((cam.eye - c.center).dot(c.forward()) < -3.0, "behind the car: {cam:?}");
    assert!(cam.fov > 61.0, "wider at speed: {}", cam.fov);
    // A tap of △: it brakes to a stop, then the player gets out beside it.
    sim.step(&press(pad::NORTH));
    let mut out_after = None;
    for t in 0..360 {
        sim.step(&InputFrame::default());
        if game(&sim).drive.phase == Phase::Foot {
            out_after = Some(t);
            break;
        }
    }
    eprintln!("out after {out_after:?} ticks");
    assert!(out_after.is_some(), "out within six seconds");
    assert_eq!(riding(&sim), None);
    let feet = sim.feet(sim.state.player.unwrap()).unwrap();
    let c = gtf_game::drive::Car::read(&sim, car).unwrap();
    assert!(feet.distance(c.feet()) < 3.5, "standing beside it");
    assert!(game(&sim).radio.on.is_none(), "the radio stays in the car");
}

#[test]
fn a_locked_car_is_broken_into_and_a_driver_is_pulled_out() {
    let mut sim = roam();
    cmd(&mut sim, "car paloma_cab");
    let cab = newest(&sim, "paloma_cab");
    game_mut(&mut sim).drive.locked.insert(cab);
    steps(&mut sim, 30, &InputFrame::default());
    walk_up(&mut sim, cab);
    let ticks = get_in(&mut sim, 240);
    assert_eq!(riding(&sim), Some(cab), "in after {ticks} ticks");
    assert!(game(&sim).drive.alarms.contains_key(&cab), "the alarm sounds");
    assert!(!game(&sim).drive.locked.contains(&cab));
    // Out again, then a car with someone at the wheel.
    sim.step(&press(pad::NORTH));
    steps(&mut sim, 90, &InputFrame::default());
    assert_eq!(game(&sim).drive.phase, Phase::Foot);
    cmd(&mut sim, "car fenwick_commuter");
    let car = newest(&sim, "fenwick_commuter");
    let door = gtf_game::drive::Car::read(&sim, car).unwrap().door_point();
    let driver = sim.spawn_npc("driver", door + Vec3::X * 0.0, 0.0, pav_core::puppet::PuppetDef::default(), None, None);
    sim.enter_vehicle(driver, car);
    steps(&mut sim, 30, &InputFrame::default());
    walk_up(&mut sim, car);
    let ticks = get_in(&mut sim, 240);
    let out = sim.state.entities.get(driver).and_then(|e| e.character.as_ref()).map(|c| c.riding);
    eprintln!("carjacked after {ticks} ticks; the driver now rides {out:?}");
    assert_eq!(riding(&sim), Some(car), "the player took the wheel");
    assert_eq!(out, Some(None), "the driver was pulled out");
}

#[test]
fn holding_triangle_at_speed_bails_out_and_the_car_rolls_on() {
    let mut sim = roam();
    cmd(&mut sim, "drive okami_kaze");
    let car = newest(&sim, "okami_kaze");
    assert_eq!(riding(&sim), Some(car), "seated by the command");
    let mut kmh = 0.0;
    for _ in 0..300 {
        sim.step(&throttle(1.0));
        kmh = sim.vehicle_status(car).unwrap().kmh;
        if kmh > 70.0 {
            break;
        }
    }
    assert!(kmh > 60.0, "fast enough to bail: {kmh}");
    steps(&mut sim, 25, &hold(pad::NORTH));
    assert_eq!(riding(&sim), None, "bailed out");
    assert_eq!(game(&sim).drive.phase, Phase::Foot);
    steps(&mut sim, 20, &InputFrame::default());
    let rolling = sim.vehicle_status(car).unwrap().kmh;
    eprintln!("bailed at {kmh:.0} km/h; the car still rolls at {rolling:.0} km/h");
    assert!(rolling > 20.0, "the empty car coasts on: {rolling}");
}

#[test]
fn the_helicopter_climbs_on_r2() {
    let mut sim = roam();
    cmd(&mut sim, "drive skylark_news");
    let heli = newest(&sim, "skylark_news");
    assert_eq!(riding(&sim), Some(heli));
    let y0 = sim.state.entities.get(heli).unwrap().pos.y;
    // The rotor spins up, then it climbs.
    steps(&mut sim, 300, &throttle(1.0));
    let y1 = sim.state.entities.get(heli).unwrap().pos.y;
    eprintln!("helicopter climbed {:.1} m", y1 - y0);
    assert!(y1 - y0 > 5.0, "climbed: {}", y1 - y0);
}
