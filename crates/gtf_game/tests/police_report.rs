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
