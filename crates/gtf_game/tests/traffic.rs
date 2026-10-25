//! M4 traffic: cars fill the roads around the player and drive them for two minutes without
//! running red lights, crashing into each other, getting stuck or appearing in view; they pull
//! over for a siren behind them; the buses stop at their stops; a driver the player runs into
//! gets out, one the player shaves past stops short or swerves.

use std::collections::BTreeMap;

use gtf_game::Freeway;
use gtf_game::traffic::{Mood, Tier};
use pav_core::entity::EntityId;
use pav_core::vehicle::VehicleInput;
use pav_core::{InputFrame, Sim};
use pav_world::lanes::Light;

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn cmd(sim: &mut Sim, c: &str) {
    sim.step(&InputFrame { cmds: vec![c.into()], ..Default::default() });
}

fn roam(seed: u64) -> Sim {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", seed).unwrap();
    cmd(&mut sim, "time 12:00");
    sim
}

#[test]
fn two_minutes_of_traffic_obey_the_lights_and_keep_moving() {
    let mut sim = roam(11);
    let city = gtf_game::world::city().unwrap();
    let g = &city.lanes;
    // Each car's lane last tick, and how long that lane's light has been red.
    let mut last: BTreeMap<EntityId, u32> = BTreeMap::new();
    let mut red_for: BTreeMap<u32, f32> = BTreeMap::new();
    let (mut violations, mut moved, mut samples, mut most) = (0, 0, 0, 0);
    let dt = 1.0 / 60.0;
    for tick in 0..7200 {
        sim.step(&InputFrame::default());
        let gm = game(&sim);
        let t = gm.time;
        most = most.max(gm.traffic.cars.len());
        for a in &gm.traffic.cars {
            if let Some(&prev) = last.get(&a.car)
                && tick > 120
                && prev != a.lane
                && g.lanes[prev as usize].next.contains(&a.lane)
                && red_for.get(&prev).copied().unwrap_or(0.0) > 1.0
                && a.mood == Mood::Cruise
            {
                violations += 1;
                eprintln!(
                    "red light run at tick {tick}: car {:?} lane {prev} -> {} tier {:?} speed {:.1} along {:.1} held {} len {:.1} stop {:.1}",
                    a.car, a.lane, a.tier, a.speed, a.along, a.held, g.lanes[prev as usize].length, g.lanes[prev as usize].stop
                );
            }
            last.insert(a.car, a.lane);
            if tick % 60 == 0 {
                samples += 1;
                if a.speed > 1.0 {
                    moved += 1;
                }
            }
        }
        // Lights of the lanes cars are on.
        let lanes: Vec<u32> = gm.traffic.cars.iter().map(|a| a.lane).collect();
        for l in lanes {
            let red = g.light(l, t) == Some(Light::Red);
            let e = red_for.entry(l).or_insert(0.0);
            *e = if red { *e + dt } else { 0.0 };
        }
    }
    let s = game(&sim).traffic.summary();
    eprintln!("traffic after 2 min: {s}; up to {most} cars; {moved}/{samples} samples moving; {violations} red lights run");
    assert!(most >= 25, "the roads fill: {most}");
    assert_eq!(violations, 0, "nobody runs a red light");
    assert!(s["ai_crashes"].as_u64().unwrap() <= 1, "{s}");
    assert_eq!(s["stuck_cars"], 0, "{s}");
    assert_eq!(s["spawn_in_view"], 0, "{s}");
    assert_eq!(s["nan_positions"], 0, "{s}");
    assert!(moved * 3 > samples, "traffic flows (a third or more moving at any moment): {moved}/{samples}");
}

#[test]
fn cars_pull_over_for_a_siren_behind_them() {
    let mut sim = roam(5);
    for _ in 0..300 {
        sim.step(&InputFrame::default());
    }
    // A traffic car near the player at full physics, and a police car 18 m behind it.
    let city = gtf_game::world::city().unwrap();
    let (car, lane, along) = {
        let gm = game(&sim);
        let a = gm
            .traffic
            .cars
            .iter()
            .filter(|a| a.tier == Tier::Full && a.bus.is_none() && a.along > 25.0)
            .min_by_key(|a| a.car)
            .expect("a car near the player");
        (a.car, a.lane, a.along)
    };
    let (p, dir) = city.lanes.lanes[lane as usize].at(along - 18.0);
    let cop =
        gtf_game::vehicles::spawn(&mut sim, "sppd_interceptor", p + glam::Vec3::Y * 0.05, dir.x.atan2(dir.z), None).unwrap();
    sim.set_vehicle_input(cop, Some(VehicleInput { throttle: 0.25, siren: true, ..Default::default() }));
    let mut pulled = false;
    for _ in 0..240 {
        sim.step(&InputFrame::default());
        if game(&sim).traffic.cars.iter().any(|a| a.car == car && a.mood == Mood::PullOver) {
            pulled = true;
            break;
        }
    }
    assert!(pulled, "the car ahead pulled over for the siren");
}

#[test]
fn a_driver_the_player_runs_into_gets_out() {
    let mut sim = roam(5);
    for _ in 0..300 {
        sim.step(&InputFrame::default());
    }
    cmd(&mut sim, "drive okami_kaze");
    let mine = game(&sim).drive.vehicle().expect("the player drives");
    let (car, driver) = {
        let gm = game(&sim);
        let a = gm
            .traffic
            .cars
            .iter()
            .filter(|a| a.tier == Tier::Full && a.bus.is_none() && !a.mission && a.driver.is_some())
            .min_by_key(|a| a.car)
            .expect("a car near the player");
        (a.car, a.driver.unwrap())
    };
    // A hard knock from the player's car (as the physics reports it).
    let pos = sim.state.entities.get(car).unwrap().pos;
    let crash = pav_core::frame::SimEvent::Crash { vehicle: mine, other: Some(car), impulse: 4000.0, pos };
    // (The game is taken out of the sim while it runs, as the sim does each tick.)
    let mut g = sim.state.play.take().unwrap();
    g.get_mut::<Freeway>().unwrap().traffic.crashes(&mut sim, &[crash], &[], Some(mine));
    sim.state.play = Some(g);
    let a = game(&sim).traffic.cars.iter().find(|a| a.car == car).unwrap();
    assert!(a.crossed, "the knock is the player's");
    // Shaken for a moment, then out of the car to have words (or to film it, or to leave).
    sim.state.play.as_mut().unwrap().get_mut::<Freeway>().unwrap().traffic.cars.iter_mut().find(|a| a.car == car).unwrap().mood =
        Mood::Shaken;
    let mut out = false;
    for _ in 0..300 {
        sim.step(&InputFrame::default());
        let gm = game(&sim);
        if gm.peds.list.iter().any(|p| p.id == driver) && !gm.traffic.cars.iter().any(|a| a.car == car) {
            out = true;
            break;
        }
    }
    assert!(out, "the driver got out and the crowd has them");
    let seated = sim.state.entities.get(driver).and_then(|e| e.character.as_ref()).is_some_and(|c| c.riding.is_some());
    assert!(!seated, "out of the seat");
}

#[test]
fn a_near_miss_makes_a_driver_stop_short_or_swerve() {
    let mut sim = roam(5);
    for _ in 0..300 {
        sim.step(&InputFrame::default());
    }
    let cars: Vec<EntityId> = game(&sim)
        .traffic
        .cars
        .iter()
        .filter(|a| a.tier == Tier::Full && a.bus.is_none() && !a.mission && a.mood == Mood::Cruise)
        .map(|a| a.car)
        .take(2)
        .collect();
    assert_eq!(cars.len(), 2, "two cars near the player");
    let mut g = sim.state.play.take().unwrap();
    let fw = g.get_mut::<Freeway>().unwrap();
    for (car, roll) in [(cars[0], 0.2), (cars[1], 0.8)] {
        let left = sim.state.entities.get(car).unwrap().pos + glam::Vec3::X * 2.0;
        fw.traffic.startle(&sim, car, left, roll);
    }
    let a = |id: EntityId| fw.traffic.cars.iter().find(|a| a.car == id).unwrap();
    assert_eq!(a(cars[0]).mood, Mood::Shaken, "a hard stop");
    assert!(a(cars[1]).swerve != 0.0, "a swerve away");
    sim.state.play = Some(g);
    // The swerve passes and the car drives on.
    for _ in 0..120 {
        sim.step(&InputFrame::default());
    }
    if let Some(a) = game(&sim).traffic.cars.iter().find(|a| a.car == cars[1]) {
        assert_eq!(a.swerve, 0.0, "over in a moment");
    }
}

#[test]
fn the_buses_stop_at_their_stops() {
    let mut sim = roam(7);
    let mut dwelt = false;
    for _ in 0..(180 * 60) {
        sim.step(&InputFrame::default());
        if game(&sim).traffic.cars.iter().any(|a| a.bus.is_some_and(|b| b.dwell > 0.0)) {
            dwelt = true;
            break;
        }
    }
    let gm = game(&sim);
    assert!(gm.traffic.cars.iter().filter(|a| a.bus.is_some()).count() >= 1, "a bus runs");
    assert!(dwelt, "a bus waited at a stop within three minutes");
}
