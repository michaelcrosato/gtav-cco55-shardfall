//! M3 vehicles: every archetype in `content/freeway/vehicles.toml` loads, and every car meets the
//! feel targets of PLAN §5.6.3 on the engine's feel tracks (`pav_core::vehicle::feel`): top
//! speed and acceleration within ±10% of the table (vehicles slower than 100 km/h: to 80% of
//! their top speed), braking from 100 km/h in 38–50 m (compact) and 32–40 m (sports), 0.9–1.1 g
//! on the skidpad and a 25–45° handbrake slide (cars; heavy vehicles of 2.5 t and up get looser
//! ranges), no roll-over on flat ground, a 15% grade at ≥ 60% of top speed (cars; heavy ones
//! must climb it), and the kicker jump landed upright. The helicopters fly.

use gtf_game::vehicles::{self, Archetype};
use pav_core::vehicle::VehicleInput;
use pav_core::vehicle::feel::{self, Feel, Test};
use pav_core::{InputFrame, Sim};

/// PLAN §5.6.3: (key, top speed m/s, 0–100 km/h seconds).
const TABLE: &[(&str, f32, f32)] = &[
    ("fenwick_commuter", 30.0, 9.0),
    ("zephyr_wagon", 31.0, 8.5),
    ("okami_kaze", 42.0, 3.8),
    ("bravura_stallion", 38.0, 4.6),
    ("daimon_titan", 31.0, 7.0),
    ("marlin_bay", 30.0, 7.5),
    ("fenwick_courier", 26.0, 11.0),
    ("paloma_cab", 32.0, 8.0),
    ("sppd_interceptor", 40.0, 5.0),
    ("metro_bus", 20.0, 18.0),
    ("ironclad_armored", 25.0, 14.0),
    ("city_works", 24.0, 13.0),
    ("sppd_tactical", 30.0, 9.0),
];

fn within(x: f32, target: f32, tol: f32) -> bool {
    (x - target).abs() <= target * tol
}

/// What `a` misses of its targets (empty: all met).
fn misses(a: &Archetype, f: &Feel, top: f32, accel: f32) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |ok: bool, what: String| {
        if !ok {
            out.push(what);
        }
    };
    let ts = f.top_speed.unwrap_or(0.0);
    check(within(ts, top, 0.1), format!("top speed {ts:.1} m/s (target {top})"));
    match f.accel_time {
        Some(t) => check(within(t, accel, 0.1), format!("0-{:.0} km/h in {t:.2} s (target {accel})", f.accel_to * 3.6)),
        None => check(false, format!("never reached {:.0} km/h", f.accel_to * 3.6)),
    }
    let d = f.brake_distance.unwrap_or(f32::MAX);
    match a.def.class.as_str() {
        "compact" => check((38.0..=50.0).contains(&d), format!("braking {d:.1} m (38-50)")),
        "sports" => check((32.0..=40.0).contains(&d), format!("braking {d:.1} m (32-40)")),
        _ => {
            let g = f.brake_from * f.brake_from / (2.0 * d) / 9.81;
            check((0.45..=1.15).contains(&g), format!("braking {d:.1} m from {:.1} m/s = {g:.2} g", f.brake_from))
        }
    }
    let g = f.grip_g.unwrap_or(0.0);
    let s = f.slide_deg.unwrap_or(0.0);
    let climb = f.climb_share.unwrap_or(0.0);
    if a.heavy() {
        check((0.6..=1.1).contains(&g), format!("skidpad {g:.2} g (0.6-1.1)"));
        check(s < 90.0, format!("handbrake slide {s:.0} deg spun it round"));
        check(climb >= 0.15, format!("15% grade at {:.0}% of top speed (heavy: climbs it)", climb * 100.0));
    } else {
        check((0.9..=1.1).contains(&g), format!("skidpad {g:.2} g (0.9-1.1)"));
        check((25.0..=45.0).contains(&s), format!("handbrake slide {s:.0} deg (25-45)"));
        check(f.slide_recovered == Some(true), "did not straighten out after the slide".into());
        check(climb >= 0.6, format!("15% grade at {:.0}% of top speed (>= 60%)", climb * 100.0));
    }
    check(!f.rolled_over, "rolled over on flat ground".into());
    match &f.jump {
        Some(j) => {
            check(j.upright, format!("not upright after the jump ({j:?})"));
            check(j.landing_pitch.abs() <= 20.0, format!("landed at {:.0} deg pitch", j.landing_pitch));
            check(j.max_roll <= 15.0, format!("rolled {:.0} deg in the air", j.max_roll));
        }
        None => check(false, "no jump".into()),
    }
    out
}

fn row(a: &Archetype, f: &Feel) -> String {
    let j = f.jump.clone().unwrap_or_default();
    format!(
        "{:18} top {:5.1}  0-{:.0} {:5.2}s  brake {:5.1}m  {:4.2}g  slide {:4.1}{}  climb {:4.0}%  jump {:+5.1}/{:+5.1}/{:+5.1} deg air {:.2}s {}",
        a.key,
        f.top_speed.unwrap_or(0.0),
        f.accel_to * 3.6,
        f.accel_time.unwrap_or(f32::NAN),
        f.brake_distance.unwrap_or(f32::NAN),
        f.grip_g.unwrap_or(f32::NAN),
        f.slide_deg.unwrap_or(f32::NAN),
        if f.slide_recovered == Some(true) { "" } else { "!" },
        f.climb_share.unwrap_or(0.0) * 100.0,
        j.takeoff_pitch,
        j.max_pitch,
        j.landing_pitch,
        j.air_time,
        if j.upright { "upright" } else { "NOT UPRIGHT" },
    )
}

fn check_group(keys: &[&str]) {
    let all = vehicles::all().unwrap();
    let mut failures = Vec::new();
    for key in keys {
        let a = all.iter().find(|a| a.key == *key).unwrap_or_else(|| panic!("{key} is not in vehicles.toml"));
        let &(_, top, accel) = TABLE.iter().find(|(k, _, _)| k == key).unwrap();
        let f = feel::run(&a.def, &Test::ALL);
        println!("{}", row(a, &f));
        for m in misses(a, &f, top, accel) {
            failures.push(format!("{key}: {m}"));
        }
    }
    assert!(failures.is_empty(), "feel targets missed:\n{}", failures.join("\n"));
}

#[test]
fn every_archetype_loads() {
    let all = vehicles::all().unwrap();
    assert_eq!(all.len(), 15, "13 required + 2 optional archetypes (PLAN §5.6.3)");
    for (key, _, _) in TABLE {
        assert!(all.iter().any(|a| a.key == *key), "{key}");
    }
    for a in all.iter() {
        assert!(!a.name.is_empty() && !a.colors.is_empty(), "{}", a.key);
        assert_eq!(a.def.color, a.colors[0], "{}: paint is its first colour", a.key);
        if !a.helicopter() {
            assert!(!a.def.seats.is_empty() && !a.def.doors.is_empty(), "{}: seats and doors", a.key);
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
