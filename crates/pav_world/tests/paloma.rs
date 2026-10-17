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
    assert_eq!(city.chunk_bounds.0, glam::IVec2::new(-16, -16));
    assert_eq!(city.chunk_bounds.1, glam::IVec2::new(15, 15));
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
    assert_eq!((m.width, m.height), (1024, 1024));
    if std::env::var("CITYMAP").is_ok() {
        let f = std::fs::File::create(concat!(env!("CARGO_MANIFEST_DIR"), "/../../out/citymap.png")).unwrap();
        let mut e = png::Encoder::new(std::io::BufWriter::new(f), m.width, m.height);
        e.set_color(png::ColorType::Rgba);
        e.write_header().unwrap().write_image_data(&m.rgba).unwrap();
    }
}
