//! M7: activities — every stunt jump is landed from its run-up on three seeds (the bonus, the
//! banner, done once), the Lost Shields are taken by walking up to them and all fifteen pay
//! (and put the gold Kaze out at Dez's place), and completion reaches 100% with everything done.

use glam::{Vec2, Vec3};
use gtf_game::Freeway;
use gtf_game::activities;
use pav_core::{InputFrame, Sim};

fn sim(scene: &str, seed: u64) -> Sim {
    gtf_game::register();
    Sim::new(scene, seed).unwrap_or_else(|e| panic!("{scene}: {e:#}"))
}

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn game_mut(sim: &mut Sim) -> &mut Freeway {
    sim.state.play.as_mut().unwrap().get_mut::<Freeway>().unwrap()
}

fn cmd(sim: &mut Sim, c: &str) {
    sim.step(&InputFrame { cmds: vec![c.into()], ..Default::default() });
}

fn steps(sim: &mut Sim, n: usize) {
    for _ in 0..n {
        sim.step(&InputFrame::default());
    }
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

/// Puts the player at `at` and waits for the ground to stream in.
fn stand_at(s: &mut Sim, at: Vec3) {
    let pid = s.state.player.unwrap();
    s.set_position(pid, at + Vec3::Y * 0.3);
    steps(s, 240);
}

/// From jump `id`'s run-up in an Okami Kaze: flat out at the lip, hands off in the air, the
/// brakes once down. Whether it counted.
fn jump_run(seed: u64, id: &str) -> (bool, String) {
    let d = activities::def().unwrap();
    let j = d.jump.iter().find(|j| j.id == id).unwrap().clone();
    let mut s = sim("freeway/roam", seed);
    steps(&mut s, 30);
    let run = Vec3::from(j.runup);
    stand_at(&mut s, run);
    cmd(&mut s, "drive okami_kaze");
    steps(&mut s, 30);
    let car = game(&s).drive.vehicle().expect("in the car");
    let dir = j.dir();
    s.place_vehicle(car, run, dir.x.atan2(dir.y));
    steps(&mut s, 60);
    let mut launched = false;
    for t in 0..60 * 30 {
        let c = gtf_game::drive::Car::read(&s, car).unwrap();
        let air = s.vehicle_status(car).is_some_and(|st| st.airborne);
        launched |= air;
        let at = flat(c.feet());
        let past = (at - j.gate()).dot(dir) > 0.0;
        let aim = if past { j.land() } else { j.gate() + dir * 2.0 };
        let want = (aim - at).normalize_or_zero();
        let fwd = flat(c.forward()).normalize_or_zero();
        let cross = fwd.x * want.y - fwd.y * want.x;
        let mut f = InputFrame::default();
        if !air {
            f.pad.stick_l.x = (cross * 2.5).clamp(-1.0, 1.0);
        }
        match (launched, air) {
            (false, _) => f.pad.r2 = 1.0,
            (true, true) => {}
            (true, false) => f.pad.l2 = 1.0,
        }
        s.step(&f);
        if std::env::var("GTF_JUMP").is_ok() && t % 30 == 0 {
            let c = gtf_game::drive::Car::read(&s, car).unwrap();
            eprintln!(
                "t={:.1} at {:?} v={:.1} air={air} up={:.2}",
                t as f32 / 60.0,
                c.feet(),
                c.vel.length(),
                (c.rot * Vec3::Y).y
            );
        }
        let g = game(&s);
        if g.activities.done.contains(&format!("jump:{id}")) {
            return (true, format!("t={:.1}s", t as f32 / 60.0));
        }
        if launched && !air && c.vel.length() < 0.5 && t > 600 {
            break;
        }
    }
    let c = gtf_game::drive::Car::read(&s, car).unwrap();
    let banner = game(&s).activities.jumps.banner.clone();
    (false, format!("launched {launched}, ended at {:?}, speed {:.1}, banner {banner:?}", c.feet(), c.vel.length()))
}

/// A run at one jump with captures along the way (`GTF_JUMP`, `GTF_SEED`).
#[test]
#[ignore = "diagnostic: GTF_JUMP=<id> GTF_SEED=<n> prints the run and captures out/act/jump_*.png"]
fn jump_look() {
    let id = std::env::var("GTF_JUMP").unwrap_or("ditch".into());
    let seed = std::env::var("GTF_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1);
    let (ok, how) = jump_run(seed, &id);
    eprintln!("{id} seed {seed}: {ok} {how}");
}

/// How many seeds the bot suites play: 3, or `GTF_SEEDS` (the bug bash plays 5).
fn seeds() -> u64 {
    std::env::var("GTF_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(3)
}

#[test]
fn every_stunt_jump_is_landed_from_its_runup_on_three_seeds() {
    let d = activities::def().unwrap();
    assert_eq!(d.jump.len(), 5, "PLAN: five stunt jumps");
    let mut missed = Vec::new();
    for j in &d.jump {
        for seed in 1..=seeds() {
            let (ok, how) = jump_run(seed, &j.id);
            eprintln!("{} seed {seed}: {ok} {how}", j.id);
            if !ok {
                missed.push(format!("{} seed {seed}: {how}", j.id));
            }
        }
    }
    assert!(missed.is_empty(), "{}", missed.join("\n"));
}

/// Walks the player toward `to` (the world-space move input) until `done` or `secs` pass.
fn walk_to(s: &mut Sim, to: Vec3, secs: f32, done: impl Fn(&Sim) -> bool) -> bool {
    for _ in 0..(secs * 60.0) as usize {
        if done(s) {
            return true;
        }
        let me = s.state.entities.get(s.state.player.unwrap()).unwrap().pos;
        let f = InputFrame { move_dir: (flat(to) - flat(me)).normalize_or_zero(), ..Default::default() };
        s.step(&f);
    }
    done(s)
}

#[test]
fn every_lost_shield_is_taken_by_walking_up_to_it_and_all_fifteen_pay() {
    let d = activities::def().unwrap();
    assert_eq!(d.shield.len(), 15, "PLAN: fifteen Lost Shields");
    let mut s = sim("freeway/roam", 1);
    steps(&mut s, 30);
    let cash = game(&s).money;
    for (n, sh) in d.shield.iter().enumerate() {
        let id = format!("shield:{}", sh.id);
        // Five metres out in front of the sign, on its ground, deck or roof.
        let a = sh.yaw.to_radians();
        stand_at(&mut s, sh.pos() + Vec3::new(a.sin(), 0.0, -a.cos()) * 5.0);
        let me = s.state.entities.get(s.state.player.unwrap()).unwrap().pos;
        assert!((me.y - 0.875 - sh.at[1]).abs() < 1.0, "{}: stood at {me:?}, the sign is at {:?}", sh.id, sh.at);
        assert!(!game(&s).activities.done.contains(&id), "{}: not taken from 5 m", sh.id);
        let took = walk_to(&mut s, sh.pos(), 8.0, |s| game(s).activities.done.contains(&id));
        assert!(took, "{}: walked up to it and took it", sh.id);
        let g = game(&s);
        let card = format!("Lost Shield {} of 15", n + 1);
        assert!(g.notes.iter().any(|x| x.title == card && x.body == sh.trivia), "{}: its card '{card}'", sh.id);
        if n < 14 {
            assert_eq!(g.money, cash, "{}: no cash before the fifteenth", sh.id);
        }
    }
    let g = game(&s);
    assert_eq!(g.money, cash + activities::shields::ALL_BONUS, "all fifteen pay $10,000");
    assert!(g.activities.shields.rewarded);
    assert!(g.phone.messages.iter().any(|m| m.from == "dez" && m.body.contains("gold")), "Dez texts about the gold Kaze");
    assert!((g.completion() - 0.1).abs() < 1e-4, "the shields are a tenth of completion: {}", g.completion());
    // Near Dez's place (out of the camera's sight of it) the gold Kaze is put out at the kerb.
    let city = gtf_game::world::city().unwrap();
    let (door, _) = city.spot("dez_home").unwrap();
    let away = city.walk.nearest_walkable(door + Vec2::new(0.0, 90.0), 15).unwrap();
    stand_at(&mut s, Vec3::new(away.x, city.ground_y(away), away.y));
    steps(&mut s, 120);
    let gold = game(&s).activities.shields.gold.expect("the gold Kaze is out");
    let e = s.state.entities.get(gold).unwrap();
    assert_eq!(e.name, "okami_kaze");
    assert_eq!(e.vehicle.as_ref().unwrap().def.color, activities::shields::GOLD);
    assert!(flat(e.pos).distance(door) < 60.0, "at the kerb near Dez's door: {:?}", e.pos);
}

#[test]
fn completion_reaches_100_with_everything_done() {
    let mut s = sim("freeway/roam", 1);
    steps(&mut s, 5);
    assert!(game(&s).completion() < 1e-4, "a new game: 0%");
    let d = activities::def().unwrap();
    let g = game_mut(&mut s);
    for m in gtf_game::missions::all().into_iter().filter(|m| m.district.is_empty()) {
        let medal = (!m.medals.is_empty()).then_some(pav_core::present::Medal::Gold);
        g.missions.records.insert(m.key.clone(), gtf_game::missions::Record { passed: true, medal, best: Some(60.0), plays: 1 });
    }
    let after_missions = g.completion();
    assert!(after_missions < 0.95, "missions alone are not everything: {after_missions}");
    for id in d.units().into_iter().chain(d.shields()) {
        g.activities.done.insert(id);
    }
    assert!((g.completion() - 1.0).abs() < 1e-4, "everything done: 100%, not {}", g.completion());
}
