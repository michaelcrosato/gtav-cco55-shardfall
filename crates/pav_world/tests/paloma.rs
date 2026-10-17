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
