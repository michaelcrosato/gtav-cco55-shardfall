//! M7 (D2): the landmarks at play. A car driven at Luxe Motors' showroom glass goes out through
//! it without losing its speed, the panes in pieces; the news helicopter waits on St. Brigid's
//! roof and flies; Dez saves at his bed, inside his apartment.

use glam::{Vec2, Vec3};
use gtf_game::Freeway;
use gtf_game::drive::Phase;
use gtf_game::shops::Counter;
use pav_core::entity::EntityId;
use pav_core::{InputFrame, Sim};

fn game(sim: &Sim) -> &Freeway {
    sim.play().unwrap().get::<Freeway>().unwrap()
}

fn game_mut(sim: &mut Sim) -> &mut Freeway {
    sim.state.play.as_mut().unwrap().get_mut::<Freeway>().unwrap()
}

fn steps(sim: &mut Sim, n: usize, f: &InputFrame) {
    for _ in 0..n {
        sim.step(f);
    }
}

fn throttle(r2: f32) -> InputFrame {
    let mut f = InputFrame::default();
    f.pad.r2 = r2;
    f
}

fn roam() -> Sim {
    gtf_game::register();
    let mut sim = Sim::new("freeway/roam", 3).unwrap();
    sim.step(&InputFrame { cmds: vec!["time 12:00".into()], ..Default::default() });
    steps(&mut sim, 30, &InputFrame::default());
    sim
}

/// The player at `feet` with the ground around loaded.
fn put(sim: &mut Sim, feet: Vec3) {
    let pid = sim.state.player.unwrap();
    sim.set_position(pid, feet);
    sim.stream_now();
    steps(sim, 10, &InputFrame::default());
    sim.set_position(pid, feet);
}

/// The player at the wheel of `car`.
fn seat(sim: &mut Sim, car: EntityId) {
    let pid = sim.state.player.unwrap();
    assert!(sim.enter_vehicle_seat(pid, car, 0), "seated");
    game_mut(sim).drive.phase = Phase::In { car };
}

#[test]
fn a_car_goes_out_through_the_showroom_glass() {
    let mut sim = roam();
    let city = gtf_game::world::city().unwrap();
    let luxe = city.def.landmark.iter().position(|l| l.key == "luxe_motors").unwrap() as u16;
    let panes: Vec<_> = city.panes.iter().filter(|p| p.landmark == luxe).cloned().collect();
    let glass_x = panes[0].max.x;
    let alive = |sim: &Sim| panes.iter().map(|p| sim.state.statics.blocks_centred_in(p.min, p.max).len()).sum::<usize>();
    // Back from the empty display pad, facing the glass: a run-up across the showroom.
    let (pad, yaw) = city.spot("luxe_showroom").unwrap();
    let start = Vec2::new(pad.x + 6.0, pad.y);
    put(&mut sim, Vec3::new(start.x, city.ground_y(start), start.y + 5.0));
    let before = alive(&sim);
    assert!(before >= panes.len() * 3, "panes, bands and frames stand: {before}");
    let car = gtf_game::vehicles::spawn(
        &mut sim,
        "bravura_stallion",
        Vec3::new(start.x, city.ground_y(start) + 0.05, start.y),
        yaw,
        None,
    )
    .unwrap();
    steps(&mut sim, 30, &InputFrame::default());
    seat(&mut sim, car);
    // Full throttle west: through the glass and out over the pavement.
    let (mut near, mut past) = (None, None);
    for _ in 0..360 {
        sim.step(&throttle(1.0));
        let c = gtf_game::drive::Car::read(&sim, car).unwrap();
        let speed = Vec2::new(c.vel.x, c.vel.z).length();
        if near.is_none() && c.center.x < glass_x + 3.5 {
            near = Some(speed);
        }
        if c.center.x < glass_x - 3.5 {
            past = Some(speed);
            break;
        }
    }
    let after = alive(&sim);
    eprintln!("{before} glass blocks stood, {after} after; {near:?} m/s before the glass, {past:?} m/s through it");
    let (near, past) = (near.expect("the car reached the glass"), past.expect("the car went out through the glass"));
    assert!(near > 6.0, "a run at it: {near} m/s");
    assert!(past > near * 0.85, "the glass did not stop it: {near} → {past} m/s");
    assert!(after < before, "panes broke");
}

#[test]
fn the_news_helicopter_waits_on_the_hospital_roof_and_flies() {
    let mut sim = roam();
    let city = gtf_game::world::city().unwrap();
    let place = city.lots.iter().find(|l| l.kind == "skylark_news").unwrap().clone();
    let (roof, _) = city.spot("hospital_roof").unwrap();
    put(&mut sim, Vec3::new(roof.x, city.ground_y(roof) + city.spot_height("hospital_roof").unwrap(), roof.y));
    steps(&mut sim, 120, &InputFrame::default());
    let heli = sim
        .state
        .entities
        .iter()
        .find(|e| e.name == "skylark_news" && e.pos.distance(place.pos) < 4.0)
        .map(|e| e.id)
        .expect("the Skylark on the helipad");
    let parked = sim.state.entities.get(heli).unwrap().pos;
    assert!(parked.y > place.pos.y - 0.5, "standing on the roof: {parked} (place {})", place.pos);
    seat(&mut sim, heli);
    steps(&mut sim, 180, &throttle(1.0));
    let up = sim.state.entities.get(heli).unwrap().pos;
    eprintln!("the Skylark from {parked} to {up}");
    assert!(up.y > parked.y + 3.0, "it climbs off the roof");
}

#[test]
fn dez_saves_at_his_bed() {
    let mut sim = roam();
    let city = gtf_game::world::city().unwrap();
    let bed = gtf_game::anchor::foot(&city, &gtf_game::anchor::Anchor::parse("spot:dez_bed").unwrap()).unwrap().0;
    assert_eq!(city.walk.get(city.walk.tile_of(Vec2::new(bed.x, bed.z))), pav_world::walk::Walk::Indoor, "inside");
    put(&mut sim, bed + Vec3::Y * 0.05);
    steps(&mut sim, 5, &InputFrame::default());
    assert_eq!(game(&sim).counter, Some(Counter::Save), "the bed saves");
    // At the door outside, it does not.
    let home = gtf_game::anchor::foot(&city, &gtf_game::anchor::Anchor::parse("spot:dez_home").unwrap()).unwrap().0;
    put(&mut sim, home + Vec3::Y * 0.05);
    steps(&mut sim, 5, &InputFrame::default());
    assert_eq!(game(&sim).counter, None, "not on the doorstep");
}
