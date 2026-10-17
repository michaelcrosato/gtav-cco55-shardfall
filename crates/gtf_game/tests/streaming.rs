//! M2 acceptance: the tour flies Paloma Bay's roads at 40 m/s (downtown, Sunset Row, Route 404,
//! the hills, the docks, The Ditch) and streaming keeps up — at every check, each chunk with
//! content within 96 m of the camera is loaded — while the active and dormant chunk counts stay
//! bounded.

use gtf_game::tour::{SPEED, Tour};
use pav_core::statics::{CHUNK_SIZE, RegionKey};
use pav_core::{InputFrame, Sim};

#[test]
fn the_city_streams_without_holes_at_40_mps() {
    gtf_game::register();
    let mut sim = Sim::new("freeway/tour", 1).unwrap();
    let city = gtf_game::world::city().unwrap();
    let tour = Tour::new(&city);
    assert!(tour.length > 4000.0, "the tour covers the district ({} m)", tour.length);
    let step = 20u64;
    let ticks = (tour.length / SPEED * 60.0) as u64;
    let (mut holes, mut checks, mut max_active, mut max_dormant) = (0, 0, 0, 0);
    let mut worst = String::new();
    let mut t = 0;
    while t < ticks {
        sim.run(step, &InputFrame::default());
        t += step;
        let s = sim.state.tick as f32 / 60.0 * SPEED;
        let cam = tour.camera(s);
        let statics = &sim.state.statics;
        max_active = max_active.max(statics.chunks.len());
        max_dormant = max_dormant.max(statics.dormant.len());
        for p in [cam.eye, tour.at(s)] {
            let (cx, cz) = ((p.x / CHUNK_SIZE).floor() as i32, (p.z / CHUNK_SIZE).floor() as i32);
            for z in cz - 3..=cz + 3 {
                for x in cx - 3..=cx + 3 {
                    let centre = glam::Vec2::new((x as f32 + 0.5) * CHUNK_SIZE, (z as f32 + 0.5) * CHUNK_SIZE);
                    if centre.distance(glam::Vec2::new(p.x, p.z)) > 96.0 || city.bin(x, z).is_none() {
                        continue;
                    }
                    checks += 1;
                    if !statics.chunks.contains_key(&RegionKey::Chunk(x, z)) {
                        holes += 1;
                        if worst.is_empty() {
                            worst = format!("tick {}: chunk ({x}, {z}) missing near {p:?}", sim.state.tick);
                        }
                    }
                }
            }
        }
    }
    eprintln!(
        "tour {:.0} m: {checks} checks, {holes} holes, at most {max_active} active / {max_dormant} dormant chunks",
        tour.length
    );
    assert_eq!(holes, 0, "{worst}");
    assert!(max_active <= 200, "{max_active} active chunks");
    assert!(max_dormant <= 600, "{max_dormant} dormant chunks");
}
