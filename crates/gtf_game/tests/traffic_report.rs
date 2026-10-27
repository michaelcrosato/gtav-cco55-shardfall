//! Traffic diagnostics (ignored; run with `--ignored --nocapture`): who crashes into whom and
//! what each car was doing before it, the buses' progress round their loop, and where people get
//! struck. Reports to stderr for investigating a failing traffic test or soak.

use gtf_game::Freeway;
use pav_core::frame::SimEvent;
use pav_core::{InputFrame, Sim};

#[test]
#[ignore]
fn crash_report() {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", 11).unwrap();
    sim.step(&InputFrame { cmds: vec!["time 12:00".into()], ..Default::default() });
    let city = gtf_game::world::city().unwrap();
    let g = &city.lanes;
    let mut hist: std::collections::BTreeMap<u32, std::collections::VecDeque<String>> = Default::default();
    let mut shown = 0;
    for tick in 0..7200 {
        sim.step(&InputFrame::default());
        let evs = sim.drain_events();
        let gm = sim.play().unwrap().get::<Freeway>().unwrap();
        if tick % 30 == 0 {
            for a in &gm.traffic.cars {
                let p = sim.state.entities.get(a.car).map(|e| e.pos).unwrap_or_default();
                let yaw =
                    sim.state.entities.get(a.car).and_then(|e| e.vehicle.as_ref()).map(|v| v.yaw.to_degrees()).unwrap_or(0.0);
                let base = sim.state.entities.get(a.car).and_then(|e| e.vehicle.as_ref()).map(|v| v.base).unwrap_or(0.5);
                let (_, off) = g.lanes[a.lane as usize].project(p - glam::Vec3::Y * base);
                let h = hist.entry(a.car.0).or_default();
                h.push_back(format!(
                    "t{tick} {:?} v{:.1} {:?} lane {} al {:.1} off {:.2} yaw {:.0} pos {:.1?} held {} route {:?} inp {:?}",
                    a.tier,
                    a.speed,
                    a.mood,
                    a.lane,
                    a.along,
                    off,
                    yaw,
                    p,
                    a.held,
                    &a.route[..a.route.len().min(3)],
                    gm_input(&sim, a.car)
                ));
                if h.len() > 40 {
                    h.pop_front();
                }
            }
        }
        for ev in &evs {
            if let SimEvent::Crash { vehicle, other: Some(o), impulse, pos } = ev {
                let a = gm.traffic.cars.iter().find(|a| a.car == *vehicle);
                let b = gm.traffic.cars.iter().find(|a| a.car == *o);
                if let (Some(a), Some(b)) = (a, b) {
                    if *impulse < 800.0 || vehicle.0 > o.0 {
                        continue;
                    }
                    let la = &g.lanes[a.lane as usize];
                    let lb = &g.lanes[b.lane as usize];
                    if shown < 2 {
                        shown += 1;
                        for l in hist.get(&vehicle.0).into_iter().flatten() {
                            eprintln!("   A {l}");
                        }
                        for l in hist.get(&o.0).into_iter().flatten() {
                            eprintln!("   B {l}");
                        }
                    }
                    eprintln!(
                        "t{tick} crash {impulse:.0} at {pos:.0?}: A lane {} {:?} {:?} turn {:?} j {:?} ap {:?} v {:.1} {:?} | B lane {} {:?} {:?} turn {:?} j {:?} ap {:?} v {:.1} {:?}",
                        a.lane,
                        a.tier,
                        la.kind,
                        la.turn,
                        la.junction,
                        la.approach,
                        a.speed,
                        a.mood,
                        b.lane,
                        b.tier,
                        lb.kind,
                        lb.turn,
                        lb.junction,
                        lb.approach,
                        b.speed,
                        b.mood
                    );
                }
            }
        }
    }
}

fn gm_input(sim: &Sim, car: pav_core::entity::EntityId) -> String {
    let st = sim.vehicle_status(car).unwrap();
    format!("thr {:.2} brk {} rev {} kin {}", st.throttle, st.braking, st.reversing, st.kinematic)
}

#[test]
#[ignore]
fn bus_report() {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", 7).unwrap();
    sim.step(&InputFrame { cmds: vec!["time 12:00".into()], ..Default::default() });
    for t in 0..(180 * 60) {
        sim.step(&InputFrame::default());
        if t % 900 == 0 {
            let gm = sim.play().unwrap().get::<Freeway>().unwrap();
            let bl = gm.traffic.bus_loop.clone().unwrap();
            if t == 0 {
                eprintln!("loop {} lanes, stops {:?}", bl.lanes.len(), bl.stops);
            }
            for a in gm.traffic.cars.iter().filter(|a| a.bus.is_some()) {
                let pos = sim.state.entities.get(a.car).unwrap().pos;
                let b = a.bus.unwrap();
                eprintln!(
                    "t{t} bus {:?} at {pos:.0?} lane {} {:?} v {:.1} held {} stop {} dist {:.0} dwell {:.1}",
                    a.car,
                    a.lane,
                    a.tier,
                    a.speed,
                    a.held,
                    b.stop,
                    bl.stops[b.stop].distance(pos),
                    b.dwell
                );
            }
        }
    }
}

#[test]
#[ignore]
fn strike_report() {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", 2).unwrap();
    for c in ["time 12:00", "drive fenwick_commuter", "autopilot on"] {
        sim.step(&InputFrame { cmds: vec![c.into()], ..Default::default() });
    }
    let city = gtf_game::world::city().unwrap();
    let mut seen = 0;
    for tick in 0..(180 * 60) {
        let input = sim.bot_input();
        sim.step(&input);
        sim.drain_events();
        let gm = sim.play().unwrap().get::<Freeway>().unwrap();
        if gm.struck.count > seen {
            seen = gm.struck.count;
            for (id, _) in &gm.struck.ghosts {
                let Some(f) = sim.feet(*id) else { continue };
                let tile = city.walk.at(glam::Vec2::new(f.x, f.z));
                let doing = gm.peds.list.iter().find(|p| p.id == *id).map(|p| format!("{:?}", p.doing));
                let near: Vec<String> = gm
                    .traffic
                    .cars
                    .iter()
                    .filter_map(|a| {
                        let p = sim.state.entities.get(a.car)?.pos;
                        (p.distance(f) < 6.0).then(|| {
                            format!(
                                "car {:?} pilot {} lane {} {:?} turn {:?} v {:.1}",
                                a.car,
                                a.pilot,
                                a.lane,
                                city.lanes.lanes[a.lane as usize].kind,
                                city.lanes.lanes[a.lane as usize].turn,
                                a.speed
                            )
                        })
                    })
                    .collect();
                eprintln!("t{tick} struck {:?} at {f:.1?} tile {tile:?} doing {doing:?} near {near:?}", id);
            }
        }
    }
}
