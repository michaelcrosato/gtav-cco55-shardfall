//! The district builds from its file, deterministically, within sane counts; every chunk the
//! map covers has content; the map picture covers the district.

use pav_core::region::RegionProvider;
use pav_world::City;

fn district() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/freeway/district.toml")).unwrap()
}

#[test]
fn paloma_bay_builds() {
    let t = std::time::Instant::now();
    let city = City::from_toml(&district()).unwrap_or_else(|e| panic!("{e}"));
    let ms = t.elapsed().as_millis();
    let (chunks, items) = city.stats();
    eprintln!("Paloma Bay: {chunks} chunks, {items} items, {} buildings, built in {ms} ms", city.buildings.len());
    assert_eq!(city.chunk_bounds.0, glam::IVec2::new(-16, -20));
    assert_eq!(city.chunk_bounds.1, glam::IVec2::new(15, 15));
    // The hills rise to the north; the city stays flat.
    let hills = city.terrain.max_height();
    assert!((35.0..70.0).contains(&hills), "hills top out at {hills} m");
    assert_eq!(city.terrain.height_at(glam::Vec2::new(-148.0, -225.0)), 0.0, "Sunset Row is flat");
    for (name, x, z) in [
        ("lookout", -248.0, -584.0),
        ("lookout drive", -200.0, -584.0),
        ("mirador drive", -100.0, -488.0),
        ("sign", -56.0, -620.0),
    ] {
        eprintln!("ground at {name}: {:.1} m", city.ground_y(glam::Vec2::new(x, z)));
    }
    assert!(chunks >= 1000, "nearly every chunk has something: {chunks}");
    assert!(city.buildings.len() > 300, "{} buildings", city.buildings.len());
    // The same file builds the same city.
    let again = City::from_toml(&district()).unwrap();
    assert_eq!(again.stats(), city.stats());
    let mut a = pav_core::region::RegionFill::default();
    let mut b = pav_core::region::RegionFill::default();
    city.fill(3, -2, &mut a);
    again.fill(3, -2, &mut b);
    assert_eq!(format!("{:?}", a.blocks), format!("{:?}", b.blocks));
    // The map picture.
    let m = &city.map;
    assert_eq!((m.width, m.height), (1024, 1152));
    if std::env::var("CITYMAP").is_ok() {
        let f = std::fs::File::create(concat!(env!("CARGO_MANIFEST_DIR"), "/../../out/citymap.png")).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), m.width, m.height);
        e.set_color(png::ColorType::Rgba);
        e.write_header().unwrap().write_image_data(&m.rgba).unwrap();
    }
}

/// The landmarks built from parts: the showroom's breakable glass, their lots (the news
/// helicopter on the hospital roof), the turning wheel, the floors people stand on (the pier's
/// deck over the water), the places missions name, inside where they should be.
#[test]
fn landmarks_have_their_places() {
    let city = City::from_toml(&district()).unwrap_or_else(|e| panic!("{e}"));
    for s in &city.def.spot {
        let (p, _) = city.spot(&s.name).unwrap();
        eprintln!("spot {}: cell {:?}, world ({:.1}, {:.1}), stands at {:.2} m", s.name, s.at, p.x, p.y, city.ground_y(p) + s.y);
    }
    let key = |i: u16| city.def.landmark[i as usize].key.as_str();
    assert_eq!(city.panes.iter().filter(|p| key(p.landmark) == "luxe_motors").count(), 14, "the showroom's breakable panes");
    // The Skylark stands on St. Brigid's helipad, 30.8 m over the ground.
    let heli = city.lots.iter().find(|l| l.kind == "skylark_news").expect("the news helicopter's place");
    assert!(heli.always && key(heli.landmark) == "hospital");
    let ground = city.ground_y(glam::Vec2::new(heli.pos.x, heli.pos.z));
    assert!((heli.pos.y - ground - 30.8).abs() < 0.1, "on the roof: {} over {ground}", heli.pos.y);
    for kind in ["city_works", "ironclad_armored", "sppd_interceptor", "bravura_stallion"] {
        assert!(city.lots.iter().any(|l| l.kind == kind && l.always), "a {kind} parked in its landmark");
    }
    // The Big Wheel: a quarter turn in a quarter period, every car hanging on the rim.
    let w = city.wheels.first().expect("the Big Wheel");
    assert_eq!(w.cars, 16);
    assert!((w.angle(w.period as f64 * 0.25) - w.angle(0.0) - std::f32::consts::FRAC_PI_2).abs() < 1e-3);
    let cars = w.cars_at(37.0);
    assert_eq!(cars.len(), 16);
    for (p, _) in &cars {
        assert!(((*p + glam::Vec3::Y * w.car_pivot).distance(w.hub) - w.radius).abs() < 0.01);
    }
    // People stand on the pier's deck where it is over the water, level with the boardwalk path.
    let over_water = glam::Vec2::new(-200.0, 440.0);
    assert_eq!(city.zone_at(over_water), Some("water"));
    let path = city.ground_y(glam::Vec2::new(-176.0, 440.0));
    assert!((city.ground_y(over_water) - path).abs() < 0.1, "the deck {} by the path {path}", city.ground_y(over_water));
    // The counters, beds, showroom and bay are indoors; the yards and gates walkable; the roof
    // spots stand high.
    for name in [
        "gun_counter",
        "gas_counter",
        "gas_clerk",
        "grind_counter",
        "grind_clerk",
        "customs_bay",
        "dez_bed",
        "mack_bed",
        "roxy_bed",
        "luxe_showroom",
        "warehouse_bay",
    ] {
        let (p, _) = city.spot(name).unwrap_or_else(|| panic!("spot {name}"));
        assert_eq!(city.walk.get(city.walk.tile_of(p)), pav_world::walk::Walk::Indoor, "{name} is indoors");
        assert!(city.landmark_at(p).is_some(), "{name} is in a landmark");
    }
    for name in [
        "cityworks_yard",
        "warehouse_door",
        "diner_lot",
        "diner_door",
        "depot_yard",
        "pier_wheel",
        "boardwalk_stalls",
        "impound_lot",
    ] {
        let (p, _) = city.spot(name).unwrap_or_else(|| panic!("spot {name}"));
        assert!(city.walk.nearest_walkable(p, 1).is_some(), "{name} is on walkable ground");
    }
    // The gates and the showroom's front are on the street outside.
    for name in ["luxe_front", "cityworks_gate", "depot_gate"] {
        let (p, _) = city.spot(name).unwrap_or_else(|| panic!("spot {name}"));
        let near = city.lanes.nearest(glam::Vec3::new(p.x, 0.0, p.y), |_| true).map(|(_, _, d)| d);
        assert!(near.is_some_and(|d| d < 6.0), "{name} is on a road: {near:?}");
    }
    assert!(city.spot_height("hospital_roof").unwrap() > 30.0 && city.spot_height("garage_top").unwrap() > 10.0);
}

/// Sunset Row's shops carry names on their sign bands, facing the boulevard.
#[test]
fn sunset_row_shops_have_names() {
    let city = City::from_toml(&district()).unwrap_or_else(|e| panic!("{e}"));
    let names = &city.def.zones["commercial"].signs.names;
    let (lo, hi) = city.chunk_bounds;
    let mut signs = Vec::new();
    for x in lo.x..=hi.x {
        for z in lo.y..=hi.y {
            if let Some(b) = city.bin(x, z) {
                signs.extend(b.labels.iter().filter(|l| names.contains(&l.text)).map(|l| (l.text.clone(), l.pos)));
            }
        }
    }
    for (t, p) in &signs {
        eprintln!("{t} at ({:.0}, {:.1}, {:.0})", p.x, p.y, p.z);
    }
    assert!(signs.len() >= 12, "{} named shops", signs.len());
    // On the boulevard's two sides (rows 24-25: z -256 to -224), nowhere else.
    assert!(signs.iter().all(|(_, p)| (-262.0..-218.0).contains(&p.z)), "all along Sunset Row");
}
