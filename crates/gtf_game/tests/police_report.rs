//! Police diagnostics (ignored; `--ignored --nocapture`): every unit's mode, distance, speed and
//! path each second after the player becomes wanted.

use gtf_game::Freeway;
use pav_core::{InputFrame, Sim};

#[test]
#[ignore]
fn units_report() {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", 3).unwrap();
    sim.step(&InputFrame { cmds: vec!["time 22:30".into()], ..Default::default() });
    for _ in 0..200 {
        sim.step(&InputFrame::default());
    }
    sim.step(&InputFrame { cmds: vec!["wanted 3".into()], ..Default::default() });
    for t in 0..(30 * 60) {
        sim.step(&InputFrame::default());
        if t % 120 == 0 {
            let g = sim.play().unwrap().get::<Freeway>().unwrap();
            let me = sim.feet(sim.state.player.unwrap()).unwrap();
            eprintln!(
                "t{} stars {} searching {} unseen {:.1} health {:.0}",
                t / 60,
                g.heat.stars,
                g.heat.searching,
                g.heat.unseen,
                g.combat.health
            );
            for u in &g.police.units {
                let st = sim.vehicle_status(u.vehicle);
                let p = sim.state.entities.get(u.vehicle).map(|e| e.pos).unwrap_or_default();
                eprintln!(
                    "   {:?} {:?} dist {:.0} kmh {:.0} path {} unseen {:.1} crew {} pos {:.0?} p0 {:.0?} plast {:.0?} goal {:.0?} me {:.0?}",
                    u.kind,
                    u.mode,
                    p.distance(me),
                    st.map(|s| s.kmh).unwrap_or(0.0),
                    u.path.len(),
                    u.unseen,
                    u.crew.len(),
                    p,
                    u.path.first(),
                    u.path.last(),
                    u.goal,
                    me
                );
            }
            let out: Vec<String> = g
                .police
                .officers
                .iter()
                .filter(|o| o.duty != gtf_game::police::Duty::Seated)
                .map(|o| format!("{:?} {:.0}", o.duty, sim.feet(o.id).map(|f| f.distance(me)).unwrap_or(0.0)))
                .collect();
            eprintln!("   officers on foot: {out:?}");
        }
    }
}

#[test]
#[ignore]
fn escape_report() {
    gtf_game::register();
    let seed = std::env::var("SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut sim = Sim::new("freeway/roam", seed).unwrap();
    sim.step(&InputFrame { cmds: vec!["time 13:00".into()], ..Default::default() });
    for _ in 0..120 {
        sim.step(&InputFrame::default());
    }
    // As the test does: cruise (ROLL=0: sit still) until the police see the car.
    let roll = std::env::var("ROLL").map(|v| v != "0").unwrap_or(true);
    let cmds: &[&str] = if roll { &["drive okami_kaze", "autopilot on", "wanted 3"] } else { &["drive okami_kaze", "wanted 3"] };
    for c in cmds {
        sim.step(&InputFrame { cmds: vec![c.to_string()], ..Default::default() });
    }
    let mut waited = 0;
    while waited < 120 * 60 {
        let h = &sim.play().unwrap().get::<Freeway>().unwrap().heat;
        if h.unseen <= 0.0 && !h.searching {
            break;
        }
        let input = sim.bot_input();
        sim.step(&input);
        waited += 1;
    }
    eprintln!("seen after {:.1} s", waited as f32 / 60.0);
    sim.step(&InputFrame { cmds: vec!["autopilot escape".into()], ..Default::default() });
    let mut seen_note = 0;
    for t in 0..(180 * 60) {
        let input = sim.bot_input();
        sim.step(&input);
        let events = sim.drain_events();
        let g = sim.play().unwrap().get::<Freeway>().unwrap();
        let mine = g.drive.vehicle();
        for ev in &events {
            if let pav_core::frame::SimEvent::Crash { vehicle, other, impulse, pos } = ev
                && Some(*vehicle) == mine
                && *impulse > 3000.0
            {
                let name =
                    other.and_then(|o| sim.state.entities.get(o)).map(|e| e.name.clone()).unwrap_or_else(|| "static".into());
                let hp = sim.vehicle_status(*vehicle).map(|s| s.health).unwrap_or(0.0);
                eprintln!("  t{:.2} CRASH into {name} impulse {impulse:.0} at {pos} hp {hp:.2}", t as f32 / 60.0);
            }
        }
        if let Ok(w) = std::env::var("WATCH")
            && let Some((a, b)) = w.split_once('-')
            && let (Ok(a), Ok(b)) = (a.parse::<f32>(), b.parse::<f32>())
            && (a..b).contains(&(t as f32 / 60.0))
            && t % 6 == 0
            && let Some(me) = mine.and_then(|c| gtf_game::drive::Car::read(&sim, c))
        {
            let fwd = me.forward();
            let left = me.left();
            let mut near: Vec<String> = Vec::new();
            for e in sim.state.entities.iter().filter(|e| e.vehicle.is_some() && Some(e.id) != mine) {
                let Some(o) = gtf_game::drive::Car::read(&sim, e.id) else { continue };
                let d = o.center - me.center;
                if d.length() < 30.0 {
                    let tag = if g.traffic.parked.values().any(|&p| p == e.id) {
                        "P".to_string()
                    } else if let Some(a) = g.traffic.cars.iter().find(|a| a.car == e.id) {
                        format!("T{:?}/{}/{:.1}/{:?}", a.tier, a.lane, a.along, a.mood)
                    } else {
                        "O".into()
                    };
                    near.push(format!(
                        "{}#{}[{}] f{:.1} l{:.1} v({:.1},{:.1})",
                        e.name,
                        e.id.0,
                        tag,
                        d.dot(fwd),
                        d.dot(left),
                        o.vel.dot(fwd),
                        o.vel.dot(left)
                    ));
                }
            }
            let a = g.traffic.cars.iter().find(|a| a.pilot);
            if let Some(a) = a {
                let city = gtf_game::world::city().unwrap();
                let l = &city.lanes.lanes[a.lane as usize];
                let (p, d) = l.at(a.along);
                let lft = glam::Vec3::new(d.z, 0.0, -d.x);
                let off = (me.feet() - p).dot(lft);
                let rel: Vec<String> = sim
                    .state
                    .entities
                    .iter()
                    .filter(|e| e.vehicle.is_some() && Some(e.id) != mine && e.pos.distance(me.center) < 20.0)
                    .map(|e| format!("#{} off {:.1}", e.id.0, (e.pos - p).dot(lft)))
                    .collect();
                eprintln!(
                    "    lane width {:.1} my_off {:.1} heading_err {:.1}° others {:?}",
                    l.width,
                    off,
                    fwd.cross(d).y.asin().to_degrees(),
                    rel
                );
            }
            eprintln!(
                "  w{:.2} v {:.1} lane {:?} along {:.1} passing {:.1} in {:?} near [{}]",
                t as f32 / 60.0,
                me.vel.length(),
                a.map(|a| a.lane),
                a.map(|a| a.along).unwrap_or(0.0),
                a.map(|a| a.passing).unwrap_or(0.0),
                g.traffic.pilot_input.map(|i| (i.throttle, i.brake, i.steer)),
                near.join("; ")
            );
        }
        let last = seen_note;
        for n in g.notes.iter().filter(|n| n.id > last) {
            eprintln!("  t{:.1} note {}: {}", t as f32 / 60.0, n.title, n.body);
            seen_note = n.id;
        }
        if t % 60 == 0 || !g.fate.alive() {
            let car = g.drive.vehicle();
            let st = car.and_then(|c| sim.vehicle_status(c));
            let me = sim.feet(sim.state.player.unwrap()).unwrap();
            let near = g
                .police
                .units
                .iter()
                .filter_map(|u| sim.state.entities.get(u.vehicle).map(|e| e.pos.distance(me)))
                .fold(f32::MAX, f32::min);
            eprintln!(
                "t{} lk {:.0} stars {} searching {} unseen {:.1} evade {:.1} hp {:.0} car {:?} kmh {:.0} car_hp {:.2} burning {} nearest_cop {:.0} fate {:?}",
                t / 60,
                g.heat.last_known.distance(me),
                g.heat.stars,
                g.heat.searching,
                g.heat.unseen,
                g.heat.evade,
                g.combat.health,
                car.is_some(),
                st.as_ref().map(|s| s.kmh).unwrap_or(0.0),
                st.as_ref().map(|s| s.health).unwrap_or(0.0),
                st.as_ref().map(|s| s.burning).unwrap_or(false),
                near,
                g.fate
            );
            for u in &g.police.units {
                let Some(e) = sim.state.entities.get(u.vehicle) else { continue };
                let st = sim.vehicle_status(u.vehicle);
                eprintln!(
                    "    unit #{} {:?} {:?} d {:.0} kmh {:.0} rails {} stuck {:.1} rev {:.1} crew {}",
                    u.vehicle.0,
                    u.kind,
                    u.mode,
                    e.pos.distance(me),
                    st.map(|s| s.kmh).unwrap_or(0.0),
                    u.rails,
                    u.stuck,
                    u.reverse,
                    g.police
                        .officers
                        .iter()
                        .filter(|o| o.vehicle == u.vehicle)
                        .map(|o| format!("{:?}", o.duty))
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            if let Some(v) = car.and_then(|c| sim.state.entities.get(c)).and_then(|e| e.vehicle.as_ref()) {
                let e = sim.state.entities.get(car.unwrap()).unwrap();
                eprintln!(
                    "    car {} #{} kinematic {} engine {} wheels_down {} gear {} burst {:?} upside {:.1} pos {:.1} {:.1} {:.1}",
                    e.name,
                    e.id.0,
                    v.state.kinematic,
                    v.state.engine_on,
                    v.state.wheels_down,
                    v.state.gear,
                    v.state.burst,
                    v.state.upside,
                    e.pos.x,
                    e.pos.y,
                    e.pos.z
                );
            }
            if car.is_none() {
                let tgt = g.bot.target.and_then(|t| sim.state.entities.get(t)).map(|e| (e.name.clone(), e.pos.distance(me)));
                eprintln!(
                    "    bot target {:?} walk {:?} enter {} path {} phase {:?} bailed {} taken {}",
                    tgt,
                    g.bot.walk,
                    g.bot.enter,
                    g.bot.path.len(),
                    g.drive.phase,
                    g.bot.bailed,
                    g.bot.taken
                );
            }
            if let Some(a) = g.traffic.cars.iter().find(|a| a.pilot) {
                let city = gtf_game::world::city().unwrap();
                let l = &city.lanes.lanes[a.lane as usize];
                eprintln!(
                    "    pilot lane {} {:?} {:?} along {:.0}/{:.0} v {:.1} mood {:?} held {} blocked {:.1} passing {:.1} rev {:.1} input {:?}",
                    a.lane,
                    l.kind,
                    l.turn,
                    a.along,
                    l.length,
                    a.speed,
                    a.mood,
                    a.held,
                    a.blocked,
                    a.passing,
                    a.reverse,
                    g.traffic.pilot_input
                );
            }
            if !g.fate.alive() || g.heat.stars == 0 {
                break;
            }
        }
    }
}
