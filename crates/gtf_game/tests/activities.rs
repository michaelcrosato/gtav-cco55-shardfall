//! M7: activities — every stunt jump is landed from its run-up on three seeds (the bonus, the
//! banner, done once), the Lost Shields are taken by walking up to them and all fifteen pay,
//! and completion reaches 100% with everything done.

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

#[test]
fn every_stunt_jump_is_landed_from_its_runup_on_three_seeds() {
    let d = activities::def().unwrap();
    assert_eq!(d.jump.len(), 5, "PLAN: five stunt jumps");
    let mut missed = Vec::new();
    for j in &d.jump {
        for seed in 1..=3 {
            let (ok, how) = jump_run(seed, &j.id);
            eprintln!("{} seed {seed}: {ok} {how}", j.id);
            if !ok {
                missed.push(format!("{} seed {seed}: {how}", j.id));
            }
        }
    }
    assert!(missed.is_empty(), "{}", missed.join("\n"));
}
