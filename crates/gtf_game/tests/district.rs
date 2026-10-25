//! M7 (PLAN §7.8): the district at play. The city is the same whatever the seed (people and
//! traffic differ; roads, buildings and landmarks never do); the sidewalks, crossings and paths
//! join one network; no building stands on a road; every landmark has a door on that network.
//! (Paloma Bay's build determinism and its lane graph's connectivity are `pav_world`'s own tests:
//! `tests/determinism.rs`, `tests/lanes.rs`.)

use std::collections::VecDeque;

use glam::{IVec2, Vec2};
use pav_core::{InputFrame, Sim};
use pav_world::walk::Walk;

/// FNV-1a over a list of numbers.
fn digest(xs: impl IntoIterator<Item = f32>) -> u64 {
    xs.into_iter().fold(0xcbf2_9ce4_8422_2325, |h, x| {
        x.to_bits().to_le_bytes().iter().fold(h, |h, b| (h ^ *b as u64).wrapping_mul(0x100_0000_01b3))
    })
}

#[test]
fn the_city_is_the_same_whatever_the_seed() {
    gtf_game::register();
    let run = |seed: u64| {
        let mut s = Sim::new("freeway/roam", seed).unwrap();
        s.run(120, &InputFrame::default());
        // The world built round the player: every static block, in order of place.
        let mut blocks: Vec<[f32; 6]> =
            s.state.statics.iter_alive().map(|(_, b)| [b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z]).collect();
        blocks.sort_by(|a, b| {
            a.iter().zip(b).map(|(x, y)| x.total_cmp(y)).find(|o| o.is_ne()).unwrap_or(std::cmp::Ordering::Equal)
        });
        let world = digest(blocks.iter().flatten().copied());
        // Who is about: everyone but the player.
        let people = digest(s.state.entities.iter().filter(|e| Some(e.id) != s.state.player).flat_map(|e| e.pos.to_array()));
        (blocks.len(), world, people)
    };
    let (n1, w1, p1) = run(1);
    let (n9, w9, p9) = run(9);
    eprintln!("seed 1: {n1} blocks {w1:016x}, people {p1:016x}; seed 9: {n9} blocks {w9:016x}, people {p9:016x}");
    assert!(n1 > 1000, "the city round the player is built ({n1} blocks)");
    assert_eq!((n1, w1), (n9, w9), "the city differs between seeds");
    assert_ne!(p1, p9, "the people and traffic are the seed's");
}

/// The walk map's connected pieces (4-neighbour, every walkable tile): their sizes, largest first,
/// with a tile of each.
fn pieces(walk: &pav_world::walk::WalkMap) -> Vec<(usize, IVec2)> {
    let mut seen = vec![false; (walk.w * walk.h) as usize];
    let mut out = Vec::new();
    for z in 0..walk.h {
        for x in 0..walk.w {
            let t = IVec2::new(x, z);
            if seen[(z * walk.w + x) as usize] || !walk.walkable(t) {
                continue;
            }
            let mut n = 0;
            let mut q = VecDeque::from([t]);
            seen[(z * walk.w + x) as usize] = true;
            while let Some(c) = q.pop_front() {
                n += 1;
                for d in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
                    let m = c + d;
                    if m.x < 0 || m.y < 0 || m.x >= walk.w || m.y >= walk.h {
                        continue;
                    }
                    let i = (m.y * walk.w + m.x) as usize;
                    if !seen[i] && walk.walkable(m) {
                        seen[i] = true;
                        q.push_back(m);
                    }
                }
            }
            out.push((n, t));
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.0));
    out
}

#[test]
fn the_sidewalks_crossings_and_paths_join_one_network() {
    let city = gtf_game::world::city().unwrap();
    let walk = &city.walk;
    let all = pieces(walk);
    let total: usize = all.iter().map(|p| p.0).sum();
    let main = all[0].0;
    for (n, t) in all.iter().skip(1).take(12) {
        let c = walk.centre(*t);
        eprintln!("  apart: {n} tiles at ({:.0}, {:.0}) {:?}", c.x, c.y, walk.get(*t));
    }
    eprintln!("{} pieces; the main one holds {main} of {total} walkable tiles", all.len());
    // Every sidewalk and crossing tile is on the main network.
    let main_tiles = {
        let mut seen = vec![false; (walk.w * walk.h) as usize];
        let mut q = VecDeque::from([all[0].1]);
        seen[(all[0].1.y * walk.w + all[0].1.x) as usize] = true;
        while let Some(c) = q.pop_front() {
            for d in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
                let m = c + d;
                if m.x < 0 || m.y < 0 || m.x >= walk.w || m.y >= walk.h {
                    continue;
                }
                let i = (m.y * walk.w + m.x) as usize;
                if !seen[i] && walk.walkable(m) {
                    seen[i] = true;
                    q.push_back(m);
                }
            }
        }
        seen
    };
    let mut stray = Vec::new();
    for z in 0..walk.h {
        for x in 0..walk.w {
            let t = IVec2::new(x, z);
            if matches!(walk.get(t), Walk::Sidewalk | Walk::Crossing { .. }) && !main_tiles[(z * walk.w + x) as usize] {
                stray.push(walk.centre(t));
            }
        }
    }
    for p in stray.iter().take(10) {
        eprintln!("  a sidewalk apart at ({:.0}, {:.0})", p.x, p.y);
    }
    eprintln!("{} sidewalk or crossing tiles off the main network", stray.len());
    assert!(main as f32 > total as f32 * 0.95, "the main network holds {main} of {total} tiles");
    assert!(stray.is_empty(), "{} sidewalk or crossing tiles off the main network", stray.len());
}

#[test]
fn no_building_stands_on_a_road() {
    let city = gtf_game::world::city().unwrap();
    let mut on = Vec::new();
    for (i, b) in city.buildings.iter().enumerate() {
        // Its footprint, half a metre in from the walls, every two metres.
        let (lo, hi) = (b.min + Vec2::splat(0.5), b.max - Vec2::splat(0.5));
        let mut z = lo.y;
        'b: while z <= hi.y {
            let mut x = lo.x;
            while x <= hi.x {
                let p = Vec2::new(x, z);
                if city.grid.road(city.grid.cell_of(p)).is_some() {
                    on.push(format!(
                        "building {i} ({:.0},{:.0})-({:.0},{:.0}) on the road at ({x:.0}, {z:.0})",
                        b.min.x, b.min.y, b.max.x, b.max.y
                    ));
                    break 'b;
                }
                x += 2.0;
            }
            z += 2.0;
        }
    }
    eprintln!("{} buildings checked", city.buildings.len());
    assert!(city.buildings.len() > 300);
    assert!(on.is_empty(), "{}", on.join("\n"));
}

#[test]
fn every_landmark_has_a_door_on_the_walking_network() {
    let city = gtf_game::world::city().unwrap();
    let walk = &city.walk;
    let main = pieces(walk)[0].1;
    // Tiles reachable from the main network.
    let mut seen = vec![false; (walk.w * walk.h) as usize];
    let mut q = VecDeque::from([main]);
    seen[(main.y * walk.w + main.x) as usize] = true;
    while let Some(c) = q.pop_front() {
        for d in [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y] {
            let m = c + d;
            if m.x < 0 || m.y < 0 || m.x >= walk.w || m.y >= walk.h {
                continue;
            }
            let i = (m.y * walk.w + m.x) as usize;
            if !seen[i] && walk.walkable(m) {
                seen[i] = true;
                q.push_back(m);
            }
        }
    }
    let mut bad = Vec::new();
    for l in &city.def.landmark {
        match gtf_game::world::door(&city, &l.key) {
            None => bad.push(format!("{}: no door", l.key)),
            Some(d) => {
                let t = walk.tile_of(Vec2::new(d.x, d.z));
                if !seen[(t.y * walk.w + t.x) as usize] {
                    bad.push(format!("{}: its door ({:.0}, {:.0}) is off the walking network", l.key, d.x, d.z));
                }
            }
        }
    }
    eprintln!("{} landmarks", city.def.landmark.len());
    assert!(city.def.landmark.len() >= 20);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
