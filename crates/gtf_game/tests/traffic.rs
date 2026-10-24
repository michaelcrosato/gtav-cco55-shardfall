//! M4 traffic: cars fill the roads around the player and drive them for two minutes without
//! running red lights, crashing into each other, getting stuck or appearing in view; they pull
//! over for a siren behind them; the buses stop at their stops.

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
