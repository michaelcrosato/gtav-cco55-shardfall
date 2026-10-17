//! Vehicles v2 (`pav_core::vehicle`): cars pitch on slopes and lean in turns without tipping
//! over, hold still on a grade, right themselves after three seconds on their roof, dent and
//! break lamps and lose health in crashes (`SimEvent::Crash`), burn and explode, follow a
//! game's positions while kinematic, swing doors, seat riders by seat, float then sink in water,
//! and land a kicker jump upright. The feel tracks measure any definition.

use glam::{Quat, Vec3};
use pav_core::color::Color;
use pav_core::frame::SimEvent;
use pav_core::shape::{Look, Shape};
use pav_core::statics::{Block, Decor, RegionKey};
use pav_core::vehicle::feel::{self, Test};
use pav_core::vehicle::{VehicleDef, VehicleInput};
use pav_core::zones::{Zone, ZoneKind};
use pav_core::{EntityId, InputFrame, Sim};

/// A family sedan: 4.8 m long, 1.5 t, rear drive.
fn sedan() -> VehicleDef {
    toml::from_str(
        r#"
        class = "sedan"
        size = [0.92, 0.6, 2.4]
        clearance = 0.26
        com = [0.0, -0.32, 0.1]
        mass = 1500
        power = 3680
        knee = 0.4
        max_speed = 32.0
        brake = 13000
        grip = 1.0
        drift_grip = 0.3
        steer = 34.0
        steer_at_top = 0.35
        wheels = { radius = 0.33, track = 0.79, front_z = 1.45, rear_z = -1.4 }
        doors = [{ part = "door_l", side = "left" }, { part = "door_r", side = "right" }]
        seats = [[0.37, 0.55, 0.15], [-0.37, 0.55, 0.15]]
        "#,
    )
    .expect("the sedan parses")
}

/// An empty world with flat ground (and whatever `extra` adds).
fn world(extra: impl FnOnce(&mut Sim)) -> Sim {
    let mut sim = Sim::empty(1);
    sim.history.enabled = false;
    let st = &mut sim.state;
    st.statics
        .add(&mut st.physics, Block::new(Vec3::new(-100.0, -1.0, -100.0), Vec3::new(100.0, 0.0, 400.0), Color::hex("#555555")));
    extra(&mut sim);
    sim
}

/// A 15% grade rising along +z from the ground at z = 20.
fn slope(sim: &mut Sim) {
    let (len, angle) = (80.0f32, 0.15f32.atan());
    let (s, c) = angle.sin_cos();
    let back = Vec3::new(0.0, 0.5 * c - len * 0.5 * s, -0.5 * s - len * 0.5 * c);
    let pos = Vec3::new(0.0, -back.y, 20.0 - back.z);
    let st = &mut sim.state;
    st.statics.add_decor(
        &mut st.physics,
        RegionKey::chunk_of(pos),
        Decor {
            shape: Shape::Box { half: Vec3::new(8.0, 0.5, len * 0.5) },
            pos,
            rot: Quat::from_rotation_x(-angle),
            color: Color::hex("#a0a0a0"),
            look: Look::Cel,
            emissive: 0.0,
            solid: true,
            collider: None,
            sway: Default::default(),
            material: None,
            glow: Default::default(),
        },
    );
}

fn car(sim: &mut Sim, def: VehicleDef, feet: Vec3, rot: Quat) -> EntityId {
    let id = sim.spawn_vehicle("car", def, feet, rot, None);
    sim.run(20, &InputFrame::default());
    id
}

fn hold(sim: &mut Sim, id: EntityId, vi: VehicleInput, ticks: u64) -> Vec<SimEvent> {
    sim.set_vehicle_input(id, Some(vi));
    sim.run(ticks, &InputFrame::default());
    sim.drain_events()
}

fn pose(sim: &Sim, id: EntityId) -> (Vec3, Quat) {
    let e = sim.state.entities.get(id).unwrap();
    (e.pos, e.rot)
}

fn speed(sim: &Sim, id: EntityId) -> f32 {
    let b = sim.state.entities.get(id).unwrap().body.unwrap();
    sim.state.physics.bodies[b].linvel().length()
}

fn pitch_deg(q: Quat) -> f32 {
    (q * Vec3::Z).y.asin().to_degrees()
}

fn roll_deg(q: Quat) -> f32 {
    (q * Vec3::X).y.asin().to_degrees()
}

const GO: VehicleInput =
    VehicleInput { throttle: 1.0, brake: 0.0, steer: 0.0, pitch: 0.0, handbrake: false, horn: false, siren: false };

#[test]
fn a_car_pitches_up_a_grade_and_holds_still_on_it() {
    let mut sim = world(slope);
    let id = car(&mut sim, sedan(), Vec3::ZERO, Quat::IDENTITY);
    hold(&mut sim, id, GO, 240);
    let (p, q) = pose(&sim, id);
    assert!(p.z > 30.0 && p.y > 1.5, "on the grade: {p}");
    assert!((pitch_deg(q) - 8.5).abs() < 2.0, "pitched like the grade: {:.1} deg", pitch_deg(q));
    // The brakes stop it on the grade (held on, they would then reverse it); parked, with
    // nobody in it, it stays put.
    let mut stopped = false;
    for _ in 0..120 {
        hold(&mut sim, id, VehicleInput { brake: 1.0, ..Default::default() }, 1);
        if speed(&sim, id) < 0.3 {
            stopped = true;
            break;
        }
    }
    assert!(stopped, "the brakes stop it: {} m/s", speed(&sim, id));
    sim.set_vehicle_input(id, None);
    let before = pose(&sim, id).0;
    sim.run(180, &InputFrame::default());
    assert!(pose(&sim, id).0.distance(before) < 0.3, "parked on the grade it does not roll away");
}

#[test]
fn a_hard_turn_leans_the_body_without_tipping_it() {
    let mut sim = world(|_| {});
    let id = car(&mut sim, sedan(), Vec3::ZERO, Quat::IDENTITY);
    hold(&mut sim, id, GO, 240);
    let (mut lean, mut lowest) = (0.0f32, 1.0f32);
    for _ in 0..150 {
        hold(&mut sim, id, VehicleInput { throttle: 0.5, steer: 1.0, ..Default::default() }, 1);
        let q = pose(&sim, id).1;
        lean = lean.max(roll_deg(q).abs());
        lowest = lowest.min((q * Vec3::Y).y);
    }
    assert!(lean > 1.0, "the body leans in the turn: {lean:.1} deg");
    assert!(lean < 12.0 && lowest > 0.9, "and stays on its wheels: {lean:.1} deg, up.y {lowest:.2}");
    assert_eq!(sim.vehicle_status(id).unwrap().wheels_down, 4);
}

#[test]
fn a_car_on_its_roof_rights_itself_after_three_seconds() {
    let mut sim = world(|_| {});
    let id = sim.spawn_vehicle("car", sedan(), Vec3::new(0.0, 1.2, 0.0), Quat::from_rotation_z(std::f32::consts::PI), None);
    sim.run(150, &InputFrame::default());
    let up = |sim: &Sim| (pose(sim, id).1 * Vec3::Y).y;
    assert!(up(&sim) < -0.5, "lying on its roof: up.y {}", up(&sim));
    assert!(sim.vehicle_status(id).unwrap().upside_down);
    sim.run(180, &InputFrame::default());
    assert!(up(&sim) > 0.85, "back on its wheels: up.y {}", up(&sim));
    sim.run(60, &InputFrame::default());
    assert!(sim.vehicle_status(id).unwrap().wheels_down >= 3);
}

#[test]
fn a_crash_dents_breaks_lamps_costs_health_and_says_so() {
    let mut sim = world(|sim| {
        let st = &mut sim.state;
        st.statics
            .add(&mut st.physics, Block::new(Vec3::new(-10.0, 0.0, 80.0), Vec3::new(10.0, 3.0, 82.0), Color::hex("#777777")));
    });
    let id = car(&mut sim, sedan(), Vec3::ZERO, Quat::IDENTITY);
    let mut events = Vec::new();
    for _ in 0..600 {
        let v = speed(&sim, id);
        events.extend(hold(&mut sim, id, if v < 15.0 { GO } else { VehicleInput::default() }, 1));
        if events.iter().any(|e| matches!(e, SimEvent::Crash { vehicle, .. } if *vehicle == id)) {
            break;
        }
    }
    sim.run(30, &InputFrame::default());
    let crash = events.iter().find_map(|e| match e {
        SimEvent::Crash { vehicle, impulse, pos, .. } if *vehicle == id => Some((*impulse, *pos)),
        _ => None,
    });
    let (impulse, at) = crash.expect("hitting the wall makes a crash event");
    assert!(impulse > 1500.0 * 10.0, "a hard hit: {impulse} N·s");
    assert!(at.z > 75.0, "where it hit: {at}");
    let s = sim.vehicle_status(id).unwrap();
    assert!(s.health < 0.8 && s.health > 0.0, "health {}", s.health);
    assert!(s.crumple[0] > 0.3 && s.crumple[1] == 0.0, "dented in front: {:?}", s.crumple);
    assert!(s.lights_broken[0] && s.lights_broken[1] && !s.lights_broken[2], "headlights broken: {:?}", s.lights_broken);
}

#[test]
fn a_burning_car_explodes_after_five_seconds() {
    let mut sim = world(|_| {});
    let id = car(&mut sim, sedan(), Vec3::ZERO, Quat::IDENTITY);
    let other = car(&mut sim, sedan(), Vec3::new(4.0, 0.0, 0.0), Quat::IDENTITY);
    assert!(sim.damage_vehicle(id, 1400.0));
    let s = sim.vehicle_status(id).unwrap();
    assert!(s.burning && s.smoking && !s.wrecked, "{s:?}");
    sim.run(240, &InputFrame::default());
    assert!(!sim.vehicle_status(id).unwrap().wrecked, "still burning at 4 s");
    sim.run(90, &InputFrame::default());
    let events = sim.drain_events();
    assert!(sim.vehicle_status(id).unwrap().wrecked, "exploded by 5.5 s");
    assert!(events.iter().any(|e| matches!(e, SimEvent::Explosion { .. })));
    let near = sim.vehicle_status(other).unwrap();
    assert!(near.health < 0.7, "the blast hurt the car beside it: {}", near.health);
}

#[test]
fn a_kinematic_car_goes_where_the_game_puts_it_and_rolls_on_after() {
    let mut sim = world(|_| {});
    let id = car(&mut sim, sedan(), Vec3::ZERO, Quat::IDENTITY);
    assert!(sim.set_vehicle_kinematic(id, true));
    for i in 1..=60 {
        assert!(sim.move_kinematic_vehicle(id, Vec3::new(0.0, 0.0, i as f32 * 10.0 / 60.0), 0.0));
        sim.step(&InputFrame::default());
    }
    let (p, _) = pose(&sim, id);
    assert!((p.z - 10.0).abs() < 0.3, "moved by the game: {p}");
    let s = sim.vehicle_status(id).unwrap();
    assert!(s.kinematic && (s.speed - 10.0).abs() < 1.0, "its speed shows: {}", s.speed);
    assert!(sim.set_vehicle_kinematic(id, false));
    // Driven but off the pedals (a parked car, with nobody driving, brakes).
    hold(&mut sim, id, VehicleInput::default(), 30);
    let v = speed(&sim, id);
    assert!(v > 7.0, "back on its wheels it keeps rolling: {v} m/s");
}

#[test]
fn doors_swing_and_riders_take_seats() {
    let mut sim = world(|_| {});
    let id = car(&mut sim, sedan(), Vec3::ZERO, Quat::IDENTITY);
    let a = sim.spawn_character("a", Vec3::new(3.0, 0.0, 0.0));
    let b = sim.spawn_character("b", Vec3::new(-3.0, 0.0, 0.0));
    assert!(sim.enter_vehicle_seat(a, id, 0));
    assert!(sim.enter_vehicle_seat(b, id, 1));
    assert!(!sim.enter_vehicle_seat(b, id, 2), "the sedan has two seats");
    let s = sim.vehicle_status(id).unwrap();
    assert_eq!(s.occupants, vec![Some(a), Some(b)]);
    assert!(s.engine_on, "the driver starts it");
    assert!(sim.set_vehicle_door(id, 1, 1.0));
    sim.run(30, &InputFrame::default());
    let v = sim.state.entities.get(id).unwrap().vehicle.as_ref().unwrap();
    assert!(v.state.doors[1].open > 0.99 && v.state.doors[0].open == 0.0);
    // Riders sit in their seats: the driver on the left (+x), the passenger on the right.
    let (cp, _) = pose(&sim, id);
    let ax = sim.state.entities.get(a).unwrap().pos.x - cp.x;
    let bx = sim.state.entities.get(b).unwrap().pos.x - cp.x;
    assert!(ax > 0.2 && bx < -0.2, "seats: {ax} {bx}");
    sim.exit_vehicle(b, id);
    let out = sim.feet(b).unwrap();
    assert!(out.x < cp.x - 1.0, "the passenger gets out on the right: {out}");
    assert_eq!(sim.vehicle_status(id).unwrap().occupants, vec![Some(a), None]);
}

#[test]
fn in_deep_water_a_car_floats_then_sinks_and_drowns() {
    let mut sim = Sim::empty(1);
    sim.history.enabled = false;
    let st = &mut sim.state;
    st.statics
        .add(&mut st.physics, Block::new(Vec3::new(-30.0, -9.0, -30.0), Vec3::new(30.0, -8.0, 30.0), Color::hex("#304050")));
    let water = Zone::new(Vec3::new(-30.0, -8.0, -30.0), Vec3::new(30.0, 0.0, 30.0), ZoneKind::Water);
    st.statics.add_zone_to(RegionKey::chunk_of(Vec3::new(0.0, -4.0, 0.0)), water);
    let id = sim.spawn_vehicle("car", sedan(), Vec3::new(0.0, 0.2, 0.0), Quat::IDENTITY, None);
    sim.run(120, &InputFrame::default());
    let y2 = pose(&sim, id).0.y;
    assert!(y2 > -1.2, "floating at 2 s: {y2}");
    let s = sim.vehicle_status(id).unwrap();
    assert!(s.in_water && s.flooded && !s.engine_on, "{s:?}");
    sim.run(480, &InputFrame::default());
    let y10 = pose(&sim, id).0.y;
    assert!(y10 < -3.0, "sunk by 10 s: {y10}");
}

#[test]
fn the_feel_tracks_measure_a_definition() {
    let f = feel::run(&sedan(), &[Test::Accel, Test::Skidpad, Test::Jump]);
    let top = f.top_speed.unwrap();
    assert!(top > 0.9 * 32.0 && top <= 32.5, "top speed {top}");
    assert!(f.accel_time.unwrap() > 4.0, "0-100 {:?}", f.accel_time);
    let g = f.grip_g.unwrap();
    assert!((0.85..1.15).contains(&g), "skidpad {g} g");
    let j = f.jump.unwrap();
    assert!(j.air_time > 0.3 && j.upright && j.max_roll < 10.0, "{j:?}");
    assert!(!f.rolled_over);
}
