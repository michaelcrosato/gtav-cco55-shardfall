//! The lane graph of Paloma Bay: the open network is one strongly connected piece, routes exist
//! between lanes far apart, the freeway is reachable from the streets by its ramps, signals and
//! parking are where they should be, nearest-lane queries find the right road.

use std::collections::BTreeMap;

use glam::Vec3;
use pav_world::City;
use pav_world::lanes::{LaneGraph, LaneKind, Light, Turn};

fn city() -> City {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/freeway/district.toml")).unwrap();
    City::from_toml(&text).unwrap_or_else(|e| panic!("{e}"))
}

/// Tarjan's strongly connected components over the open lanes; returns the component of each.
fn components(g: &LaneGraph) -> Vec<usize> {
    let n = g.lanes.len();
    let open = |i: usize| !g.lanes[i].closed;
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0; n];
    let mut on = vec![false; n];
    let mut comp = vec![usize::MAX; n];
    let mut stack = Vec::new();
    let mut next = 0;
    let mut ncomp = 0;
    for root in 0..n {
        if !open(root) || index[root] != usize::MAX {
            continue;
        }
        // Iterative DFS: (node, next edge to look at).
        let mut work = vec![(root, 0usize)];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on[root] = true;
        while let Some(&mut (v, ref mut e)) = work.last_mut() {
            let l = &g.lanes[v];
            let mut succ: Vec<usize> =
                l.next.iter().copied().chain(l.left).chain(l.right).map(|j| j as usize).filter(|&j| open(j)).collect();
            // Traffic leaving at the map's edge comes back in there.
            if g.lanes[v].source && g.lanes[v].next.is_empty() {
                succ.extend((0..n).filter(|&j| g.lanes[j].source && g.lanes[j].prev.is_empty()));
            }
            if *e < succ.len() {
                let w = succ[*e];
                *e += 1;
                if index[w] == usize::MAX {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on[w] = true;
                    work.push((w, 0));
                } else if on[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                work.pop();
                if let Some(&(u, _)) = work.last() {
                    low[u] = low[u].min(low[v]);
                }
                if low[v] == index[v] {
                    loop {
                        let w = stack.pop().unwrap();
                        on[w] = false;
                        comp[w] = ncomp;
                        if w == v {
                            break;
                        }
                    }
                    ncomp += 1;
                }
            }
        }
    }
    comp
}

#[test]
fn the_open_network_is_one_piece() {
    let c = city();
    let g = &c.lanes;
    let comp = components(g);
    let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, k) in comp.iter().enumerate() {
        if !g.lanes[i].closed {
            *sizes.entry(*k).or_default() += 1;
        }
    }
    let open = g.lanes.iter().filter(|l| !l.closed).count();
    let (&main, &biggest) = sizes.iter().max_by_key(|(_, s)| **s).unwrap();
    eprintln!(
        "{} lanes ({open} open), {} junctions ({} signalised), {} parking spots; biggest piece {biggest}",
        g.lanes.len(),
        g.junctions.len(),
        g.junctions.iter().filter(|j| j.signal.is_some()).count(),
        g.parking.len()
    );
    // Show what is left out, if anything.
    for (i, k) in comp.iter().enumerate() {
        if !g.lanes[i].closed && *k != main {
            let l = &g.lanes[i];
            eprintln!("  outside: lane {i} {:?} from {:?} to {:?}", l.kind, l.points[0], l.points.last().unwrap());
        }
    }
    assert!(biggest as f32 >= open as f32 * 0.99, "{biggest} of {open} open lanes reach each other");
    let signals = g.junctions.iter().filter(|j| j.signal.is_some()).count();
    assert!(signals >= 25 && signals < g.junctions.len(), "{signals} of {} junctions have lights", g.junctions.len());
    assert!(g.parking.len() >= 400, "{} parking spots", g.parking.len());
}

#[test]
fn routes_cross_the_city_and_use_the_freeway() {
    let c = city();
    let g = &c.lanes;
    // Far-apart street lanes, picked deterministically.
    let streets: Vec<u32> = (0..g.lanes.len() as u32)
        .filter(|&i| {
            let l = &g.lanes[i as usize];
            !l.closed && matches!(l.kind, LaneKind::Street | LaneKind::Avenue | LaneKind::Boulevard)
        })
        .collect();
    let mut found = 0;
    for k in 0..40 {
        let a = streets[(k * 7919) % streets.len()];
        let b = streets[(k * 104_729 + 13) % streets.len()];
        let r = g.route_lanes(a, b);
        assert!(!r.is_empty(), "no route from lane {a} to lane {b}");
        assert_eq!(r[0], a);
        assert_eq!(*r.last().unwrap(), b);
        for w in r.windows(2) {
            let l = &g.lanes[w[0] as usize];
            assert!(l.next.contains(&w[1]) || l.left == Some(w[1]) || l.right == Some(w[1]), "a route follows the links");
        }
        found += 1;
    }
    assert_eq!(found, 40);
    // From Sunset Row up the Pretzel's northbound entrance onto Route 404 (northbound lanes run
    // east of the median)...
    let up = g.route(Vec3::new(300.0, 0.0, -236.0), Vec3::new(478.8, 11.0, -560.0));
    assert!(up.iter().any(|&i| g.lanes[i as usize].kind == LaneKind::Ramp), "the way up uses a ramp");
    assert!(up.iter().any(|&i| g.lanes[i as usize].kind == LaneKind::Freeway));
    // ...and from the southbound deck down to the Harbor Gate street.
    let down = g.route(Vec3::new(465.2, 11.0, -560.0), Vec3::new(400.0, 0.0, 184.0));
    assert!(!down.is_empty(), "southbound deck to the Harbor Gate street");
    assert!(down.iter().any(|&i| g.lanes[i as usize].kind == LaneKind::Ramp), "the way down uses a ramp");
}

#[test]
fn queries_find_the_right_roads_and_lights() {
    let c = city();
    let g = &c.lanes;
    // Sunset Row (the boulevard, rows 24-25 → z -256..-224), eastbound lanes south of the middle.
    let (i, _, d) = g.nearest(Vec3::new(-100.0, 0.0, -236.0), |l| l.kind != LaneKind::Connector).unwrap();
    assert_eq!(g.lanes[i as usize].kind, LaneKind::Boulevard);
    assert!(d < 2.5, "{d}");
    let (_, dir) = g.lanes[i as usize].at(1.0);
    assert!(dir.x > 0.9, "south of the middle runs east: {dir:?}");
    // The freeway deck wins over the street below it at deck height.
    let (i, _, _) = g.nearest(Vec3::new(465.0, 11.0, -100.0), |l| l.kind != LaneKind::Connector).unwrap();
    assert_eq!(g.lanes[i as usize].kind, LaneKind::Freeway);
    // Signalised approaches see red and green over a cycle, never both greens at once.
    let lane = (0..g.lanes.len() as u32).find(|&i| g.light(i, 0.0).is_some()).expect("a signalised lane");
    let mut seen = [false; 3];
    for t in 0..120 {
        match g.light(lane, t as f32) {
            Some(Light::Green) => seen[0] = true,
            Some(Light::Amber) => seen[1] = true,
            Some(Light::Red) => seen[2] = true,
            None => unreachable!(),
        }
    }
    assert!(seen.iter().all(|s| *s), "{seen:?}");
}

#[test]
fn people_walk_the_sidewalks_and_find_cover() {
    use pav_world::walk::Walk;
    let c = city();
    let (side, cross, path, open) = c.walk.counts();
    eprintln!("walk tiles: {side} sidewalk, {cross} crossing, {path} path, {open} open; {} cover points", c.cover.points.len());
    assert!(side > 10_000 && cross > 1_000 && path > 1_000 && open > 10_000);
    // Sunset Row's south sidewalk (z -226..-224) at the start.
    assert_eq!(c.walk.at(glam::Vec2::new(-148.0, -225.0)), Walk::Sidewalk);
    // Across downtown on foot: a path exists and stays on walkable tiles.
    let route = c.walk.path(glam::Vec2::new(-148.0, -225.0), glam::Vec2::new(-20.0, -95.0), 40_000);
    assert!(route.len() > 50, "a walk across downtown ({} tiles)", route.len());
    assert!(route.iter().any(|p| matches!(c.walk.at(*p), Walk::Crossing { .. })), "it uses a crosswalk");
    // On Sunset Row's south sidewalk the shop fronts shield from a threat behind them (south),
    // not from one across the street (north).
    let at = Vec3::new(-300.0, 0.15, -225.0);
    let behind = c.cover.near(at, at + Vec3::new(0.0, 0.0, 40.0), 12.0);
    assert!(!behind.is_empty(), "cover against a threat behind the shops");
    assert!(behind.iter().all(|p| p.normal.y < 0.0), "facing the street");
    assert!(c.cover.points.len() > 3_000);
}

#[test]
fn the_proving_grounds_build_with_a_signalised_crossroads() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/freeway/dev_district.toml")).unwrap();
    let c = City::from_toml(&text).unwrap_or_else(|e| panic!("{e}"));
    let g = &c.lanes;
    let comp = components(g);
    let open: Vec<usize> = (0..g.lanes.len()).filter(|&i| !g.lanes[i].closed).collect();
    let main = comp[open[0]];
    assert!(open.iter().all(|&i| comp[i] == main), "every open lane of the Proving Grounds reaches every other");
    assert!(g.junctions.iter().filter(|j| j.signal.is_some()).count() >= 1);
    assert_eq!(c.def.jump.len(), 2);
    assert!(c.def.sign.len() >= 8, "a sign per station");
}

#[test]
fn the_freeway_runs_both_ways_and_turns_round_short_of_its_closed_north_end() {
    let c = city();
    let g = &c.lanes;
    // The deck is open both ways (only the spur past the last exit is closed, and its ramps).
    let deck: Vec<_> = g.lanes.iter().filter(|l| l.kind == LaneKind::Freeway).collect();
    let open = deck.iter().filter(|l| !l.closed).count();
    assert!(open * 3 >= deck.len() * 2, "{open} of {} freeway lanes open", deck.len());
    let ramps = g.lanes.iter().filter(|l| l.kind == LaneKind::Ramp && l.points.iter().any(|p| p.y > 10.0));
    for r in ramps {
        assert!(!r.closed, "a freeway ramp is open: {:?} -> {:?}", r.points[0], r.points.last());
    }
    // Three U-turns up on the deck, all short of the barrier near the map's north edge (z -640).
    let turns: Vec<_> = g
        .lanes
        .iter()
        .filter(|l| l.turn == Turn::UTurn && l.points.iter().all(|p| p.y > 10.0 && p.z < -560.0 && p.x > 400.0))
        .collect();
    assert_eq!(turns.len(), 3, "a U-turn from each northbound lane");
    for t in &turns {
        assert!(!t.closed, "the turnaround is open");
        assert!(t.points.iter().all(|p| p.z > -625.0), "the turnaround stays clear of the barrier");
    }
}

#[test]
fn dead_ends_turn_round_on_the_road_and_roads_stop_short_of_the_map_edge() {
    let c = city();
    let g = &c.lanes;
    let lo = c.grid.origin;
    let hi = lo + glam::Vec2::new(c.grid.w as f32, c.grid.h as f32) * c.grid.cell;
    for (i, l) in g.lanes.iter().enumerate() {
        if l.closed || l.points.iter().any(|p| p.y > 2.0) {
            continue;
        }
        for p in &l.points {
            let at = glam::Vec2::new(p.x, p.z);
            assert!(
                at.x > lo.x + 6.0 && at.x < hi.x - 6.0 && at.y > lo.y + 6.0 && at.y < hi.y - 6.0,
                "lane {i} ({:?}) reaches {at} by the map's edge",
                l.turn
            );
            if l.turn == Turn::UTurn {
                assert!(c.road_at(at).is_some() && !c.in_building(at, 0.0), "U-turn {i} leaves the road at {at}");
            }
        }
    }
}
