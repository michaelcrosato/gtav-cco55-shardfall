//! M3 vehicles: every archetype in `content/freeway/vehicles.toml` loads, and every car meets the
//! feel targets of PLAN §5.6.3 on the engine's feel tracks (`pav_core::vehicle::feel`): top
//! speed and acceleration within ±10% of the table (vehicles slower than 100 km/h: to 80% of
//! their top speed), braking from 100 km/h in 38–50 m (compact) and 32–40 m (sports), 0.9–1.1 g
//! on the skidpad and a 25–45° handbrake slide (cars; heavy vehicles of 2.5 t and up get looser
//! ranges), no roll-over on flat ground, a 15% grade at ≥ 60% of top speed (cars; heavy ones
//! must climb it), and the kicker jump landed upright. The helicopters fly.

use gtf_game::vehicles;
use pav_core::vehicle::VehicleInput;
use pav_core::vehicle::feel::{self, Test};
use pav_core::{InputFrame, Sim};

fn check_group(keys: &[&str]) {
    let all = vehicles::all().unwrap();
    let mut failures = Vec::new();
    for key in keys {
        let a = all.iter().find(|a| a.key == *key).unwrap_or_else(|| panic!("{key} is not in vehicles.toml"));
        let f = feel::run(&a.def, &Test::ALL);
        println!("{}", vehicles::feel_row(a, &f));
        for m in vehicles::feel_misses(a, &f) {
            failures.push(format!("{key}: {m}"));
        }
    }
    assert!(failures.is_empty(), "feel targets missed:\n{}", failures.join("\n"));
}

#[test]
fn every_archetype_loads() {
    let all = vehicles::all().unwrap();
    assert_eq!(all.len(), 15, "13 required + 2 optional archetypes (PLAN §5.6.3)");
    for (key, _, _) in vehicles::FEEL_TARGETS {
        assert!(all.iter().any(|a| a.key == *key), "{key}");
    }
    for a in all.iter() {
        assert!(!a.name.is_empty() && !a.colors.is_empty(), "{}", a.key);
        assert_eq!(a.def.color, a.colors[0], "{}: paint is its first colour", a.key);
        if !a.helicopter() {
            assert!(!a.def.seats.is_empty() && !a.def.doors.is_empty(), "{}: seats and doors", a.key);
            // Its body and wheel props ship in every build (the browser's and a lone exe's).
            for name in [&a.def.body_asset, &a.def.wheel_asset] {
                assert!(pav_core::props::store::shipped(name), "{}: '{name}' is not a shipped asset", a.key);
            }
        } else {
            // A helicopter's body ships too, with the rotor groups the view spins.
            let name = &a.def.body_asset;
            assert!(pav_core::props::store::shipped(name), "{}: '{name}' is not a shipped asset", a.key);
            let body = pav_core::props::get(name).unwrap_or_else(|| panic!("{}: '{name}' does not load", a.key));
            for group in ["rotor.", "tail_rotor."] {
                assert!(body.compiled.parts.iter().any(|p| p.id.starts_with(group)), "{}: no {group}* parts", a.key);
            }
        }
    }
    assert_eq!(all.iter().filter(|a| a.helicopter()).count(), 2);
    assert!(vehicles::get("nope").unwrap_err().to_string().contains("okami_kaze"), "unknown keys list the known ones");
}

#[test]
fn compact_and_family_cars_meet_their_targets() {
    check_group(&["fenwick_commuter", "zephyr_wagon", "paloma_cab"]);
}

#[test]
fn fast_cars_meet_their_targets() {
    check_group(&["okami_kaze", "bravura_stallion", "sppd_interceptor"]);
}

#[test]
fn trucks_and_suvs_meet_their_targets() {
    check_group(&["daimon_titan", "marlin_bay", "fenwick_courier", "sppd_tactical"]);
}

#[test]
fn heavy_vehicles_meet_their_targets() {
    check_group(&["metro_bus", "ironclad_armored", "city_works"]);
}

#[test]
fn the_helicopters_fly() {
    for key in ["skylark_news", "sppd_air"] {
        let a = vehicles::get(key).unwrap();
        let mut sim = Sim::empty(1);
        sim.history.enabled = false;
        let st = &mut sim.state;
        st.statics.add(
            &mut st.physics,
            pav_core::statics::Block::new(
                glam::Vec3::new(-50.0, -1.0, -50.0),
                glam::Vec3::new(50.0, 0.0, 200.0),
                pav_core::Color::hex("#555555"),
            ),
        );
        let id = vehicles::spawn(&mut sim, key, glam::Vec3::ZERO, 0.0, None).unwrap();
        let start = sim.state.entities.get(id).unwrap().pos;
        sim.set_vehicle_input(id, Some(VehicleInput { throttle: 1.0, ..Default::default() }));
        sim.run(240, &InputFrame::default());
        let up = sim.state.entities.get(id).unwrap().pos;
        assert!(up.y - start.y > 4.0, "{key} climbs: {start} -> {up}");
        sim.set_vehicle_input(id, Some(VehicleInput { pitch: 1.0, ..Default::default() }));
        sim.run(180, &InputFrame::default());
        let p = sim.state.entities.get(id).unwrap().pos;
        assert!(p.z - up.z > 15.0, "{key} flies forward: {up} -> {p}");
        let s = sim.vehicle_status(id).unwrap();
        assert!(s.speed > 0.5 * a.def.max_speed, "{key}: {} m/s", s.speed);
    }
}
