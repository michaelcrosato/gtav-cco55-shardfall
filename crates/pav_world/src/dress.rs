//! Street dressing (PLAN.md §6.6.5) from the `LIB/gtf_city` props: along every sidewalk, by the
//! rules of the zone behind it — street lamps, palms and street trees, benches, bins, hydrants
//! and billboards with satirical ads; trees and benches in parks; shrubs and lamps on the hills;
//! containers and gantry cranes at the docks; lifeguard towers and umbrellas on the beach; mast-arm
//! traffic signals at signalised junctions. Deterministic per cell; baked into the chunks.

use glam::{IVec2, Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::rng::Rng;
use pav_core::statics::StaticProp;
use pav_core::zones::{Label, LabelMode};

use crate::def::{DistrictDef, RoadClass};
use crate::grid::{Cell, DIRS, Grid};
use crate::ground::{LOT_Y, ROAD_Y, SIDEWALK};
use crate::terrain::Terrain;
use crate::{Out, rng};

/// Satirical billboard lines (original brands, PLAN.md §5.13).
const ADS: &[&str] = &[
    "LIFEGRAM — BE SEEN OR BE NOBODY",
    "FIZZLE COLA: NOW WITH 40% MORE FIZZ",
    "HOLLOWAY HEIGHTS LOFTS — FROM $4M",
    "PALOMA SAVINGS & TRUST: YOUR MONEY, PROBABLY",
    "DOUBLE THE HORSEPOWER, HALF THE MANNERS",
    "DR. KALE'S JUICE CLEANSE: TRUST US",
    "FREEWAY FM 99.4 — STUCK IN TRAFFIC? US TOO",
    "EGO CHASER ENERGY BARS",
    "CRYPTO? NOT MAD. JUST DISAPPOINTED",
    "BRISA GT: PARKING IS FOR OTHER PEOPLE",
];

/// A library prop standing at `at`, turned to `yaw` (radians; 0 keeps its authored +z).
pub fn prop(asset: &str, at: Vec3, yaw: f32, scale: f32, seed: u32) -> StaticProp {
    StaticProp { asset: asset.into(), pos: at, rot: Quat::from_rotation_y(yaw), scale, seed, solid: false, collider: None }
}

/// The yaw that turns a prop's +z toward `dir` (horizontal).
pub fn yaw_to(dir: Vec3) -> f32 {
    dir.x.atan2(dir.z)
}

/// A cobra-head street lamp whose arm reaches toward the road.
pub fn lamp(at: Vec3, toward: Vec3, out: &mut Vec<StaticProp>) {
    out.push(prop("LIB/gtf_city/street_lamp", at, yaw_to(toward), 1.0, 0));
}

pub fn palm(at: Vec3, r: &mut Rng, out: &mut Vec<StaticProp>) {
    let tall = r.f32() < 0.75;
    let asset = if tall { "LIB/gtf_city/palm_tall" } else { "LIB/gtf_city/palm_short" };
    let scale = if tall { r.range(0.55, 0.85) } else { r.range(0.85, 1.15) };
    out.push(prop(asset, at, r.range(0.0, std::f32::consts::TAU), scale, r.below(1000)));
}

pub fn tree(at: Vec3, r: &mut Rng, out: &mut Vec<StaticProp>) {
    out.push(prop("LIB/gtf_city/street_tree", at, r.range(0.0, std::f32::consts::TAU), r.range(0.75, 1.1), r.below(1000)));
}

fn bench(at: Vec3, toward: Vec3, out: &mut Vec<StaticProp>) {
    out.push(prop("LIB/gtf_city/bench", at, yaw_to(toward), 1.0, 0));
}

fn billboard(at: Vec3, toward: Vec3, text: &str, out: &mut Out) {
    out.props.push(prop("LIB/gtf_city/billboard", at, yaw_to(toward), 1.0, 0));
    out.labels.push(Label {
        text: text.to_string(),
        pos: at + Vec3::Y * 11.4 + toward * 1.0,
        size: 0.85,
        color: Color::hex("#e8433a"),
        mode: LabelMode::Billboard,
        facing: Default::default(),
    });
}

/// Chaparral on the hills.
fn shrub(at: Vec3, r: &mut Rng, out: &mut Vec<StaticProp>) {
    out.push(prop("LIB/gtf_city/shrub", at, r.range(0.0, std::f32::consts::TAU), r.range(0.8, 1.6), r.below(1000)));
}

/// Buildings' footprints by map cell (to keep trees out of houses on the hills).
fn footprints(grid: &Grid, out: &Out) -> std::collections::BTreeMap<(i32, i32), Vec<(Vec2, Vec2)>> {
    let mut m: std::collections::BTreeMap<(i32, i32), Vec<(Vec2, Vec2)>> = Default::default();
    for b in &out.buildings {
        let (a, z) = (grid.cell_of(b.min), grid.cell_of(b.max));
        for cz in a.y..=z.y {
            for cx in a.x..=z.x {
                m.entry((cx, cz)).or_default().push((b.min - Vec2::splat(1.2), b.max + Vec2::splat(1.2)));
            }
        }
    }
    m
}

/// Dressing on the hills: trees and shrubs on the slopes, lamps along the hill roads.
fn hills(def: &DistrictDef, grid: &Grid, terrain: &Terrain, out: &mut Out) {
    let taken = footprints(grid, out);
    let cs = grid.cell;
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
            if !terrain.is_terrain(c) {
                continue;
            }
            let lo = grid.corner(c);
            let ground = |p: Vec2, y0: f32| Vec3::new(p.x, y0 + terrain.height_at(p), p.y);
            match grid.at(c).unwrap() {
                Cell::Zone(zi) => {
                    if grid.landmark_at(c).is_some() {
                        continue;
                    }
                    let rules = &def.zones[&grid.zones[zi as usize]];
                    let trees = rules.props.get("tree").copied().unwrap_or(0.0);
                    if trees <= 0.0 {
                        continue;
                    }
                    let mut r = rng(def.seed, 0x4111, x, z);
                    let near = taken.get(&(x, z));
                    let free =
                        |p: Vec2| near.is_none_or(|v| !v.iter().any(|(a, b)| p.x > a.x && p.y > a.y && p.x < b.x && p.y < b.y));
                    let n = (trees * 0.5).round() as i32 + 2;
                    for _ in 0..n {
                        let p = lo + Vec2::new(r.range(1.5, cs - 1.5), r.range(1.5, cs - 1.5));
                        if !free(p) {
                            continue;
                        }
                        if r.f32() < 0.35 {
                            tree(ground(p, LOT_Y - 0.1), &mut r, &mut out.props);
                        } else {
                            shrub(ground(p, LOT_Y - 0.15), &mut r, &mut out.props);
                        }
                    }
                }
                Cell::Road(_) => {
                    // A lamp every other cell on alternating sides, at the road's edge.
                    if (x + z) % 2 != 0 {
                        continue;
                    }
                    for (i, d) in DIRS.iter().enumerate() {
                        if grid.road(c + *d).is_some() || (x / 2 + i as i32) % 2 != 0 {
                            continue;
                        }
                        let inward = -Vec3::new(d.x as f32, 0.0, d.y as f32);
                        let (a, b) = side(lo, cs, i);
                        let p = (a + b) * 0.5 + Vec2::new(inward.x, inward.z) * 0.5;
                        lamp(ground(p, ROAD_Y), inward, &mut out.props);
                        break;
                    }
                }
            }
        }
    }
}

pub(crate) fn build(def: &DistrictDef, grid: &Grid, terrain: &Terrain, out: &mut Out) {
    hills(def, grid, terrain, out);
    let cs = grid.cell;
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
            let lo = grid.corner(c);
            if terrain.is_terrain(c) {
                continue;
            }
            match grid.at(c).unwrap() {
                Cell::Road(class) if class != RoadClass::Path && class != RoadClass::Alley && class != RoadClass::Channel => {
                    // Each sidewalk edge, dressed by the zone behind it.
                    for (i, d) in DIRS.iter().enumerate() {
                        if grid.road(c + *d).is_some_and(|r| r != RoadClass::Path) {
                            continue;
                        }
                        let Some(zone) = grid.zone(c + *d) else { continue };
                        let rules = &def.zones[zone];
                        if rules.ground == "water" {
                            continue;
                        }
                        let mut r = rng(def.seed, 0xd2e55 + i as u64, x, z);
                        // The kerb line on this side and the direction toward the road.
                        let inward = -Vec3::new(d.x as f32, 0.0, d.y as f32);
                        let (a, b) = side(lo, cs, i);
                        let along = (b - a).normalize();
                        let road = Vec2::new(inward.x, inward.z);
                        // A point `t` along the edge, `from_kerb` metres back from the kerb.
                        let kerb = |t: f32, from_kerb: f32| {
                            let p = a + along * t + road * (SIDEWALK - from_kerb);
                            Vec3::new(p.x, LOT_Y, p.y)
                        };
                        let dens = |k: &str| rules.props.get(k).copied().unwrap_or(0.0);
                        // Lamps on a regular beat (every other cell, alternating sides).
                        if dens("lamp") > 0.0 && (x + z + i as i32) % 2 == 0 {
                            lamp(kerb(cs * 0.5, 0.45), inward, &mut out.props);
                        }
                        // Four slots per edge for the rest.
                        for slot in 0..4 {
                            let t = cs * (slot as f32 + 0.5) / 4.0;
                            let roll = r.f32() * 100.0 / (cs / 4.0);
                            let mut acc = 0.0;
                            for (k, density) in [
                                ("palm", dens("palm")),
                                ("tree", dens("tree")),
                                ("bench", dens("bench")),
                                ("bin", dens("bin")),
                                ("hydrant", dens("hydrant")),
                                ("billboard", dens("billboard")),
                            ] {
                                acc += density;
                                if roll < acc {
                                    match k {
                                        "palm" => palm(kerb(t, 0.7), &mut r, &mut out.props),
                                        "tree" => tree(kerb(t, 0.8), &mut r, &mut out.props),
                                        "bench" => bench(kerb(t, 1.5), inward, &mut out.props),
                                        "bin" => {
                                            out.props.push(prop("LIB/gtf_city/trash_bin", kerb(t, 1.6), yaw_to(inward), 1.0, 0))
                                        }
                                        "hydrant" => out.props.push(prop(
                                            "LIB/gtf_city/fire_hydrant",
                                            kerb(t, 0.5),
                                            yaw_to(inward),
                                            1.0,
                                            0,
                                        )),
                                        _ => {
                                            let ad = ADS[r.below(ADS.len() as u32) as usize];
                                            billboard(kerb(t, SIDEWALK - 0.3), inward, ad, out);
                                        }
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
                Cell::Zone(zi) => {
                    let rules = &def.zones[&grid.zones[zi as usize]];
                    if grid.landmark_at(c).is_some() {
                        continue;
                    }
                    let mut r = rng(def.seed, 0x9a7c, x, z);
                    match rules.ground.as_str() {
                        // Parks: trees on a jittered grid, a bench or two.
                        "grass" if rules.styles.is_empty() => {
                            for gx in 0..2 {
                                for gz in 0..2 {
                                    if r.f32() < 0.7 {
                                        let p = lo
                                            + Vec2::new(4.0 + gx as f32 * 8.0, 4.0 + gz as f32 * 8.0)
                                            + Vec2::new(r.range(-2.0, 2.0), r.range(-2.0, 2.0));
                                        tree(Vec3::new(p.x, LOT_Y, p.y), &mut r, &mut out.props);
                                    }
                                }
                            }
                            if r.f32() < 0.3 {
                                let p = lo + Vec2::splat(cs * 0.5);
                                bench(Vec3::new(p.x, LOT_Y, p.y), Vec3::Z, &mut out.props);
                            }
                        }
                        // The beach: a few palms.
                        "sand" if r.f32() < 0.25 => {
                            let p = lo + Vec2::new(r.range(3.0, 13.0), r.range(3.0, 13.0));
                            palm(Vec3::new(p.x, LOT_Y, p.y), &mut r, &mut out.props);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

/// The cell edge on side `i` (N, E, S, W) as a line from one end to the other.
fn side(lo: Vec2, cs: f32, i: usize) -> (Vec2, Vec2) {
    let hi = lo + Vec2::splat(cs);
    match i {
        0 => (lo, Vec2::new(hi.x, lo.y)),
        1 => (Vec2::new(hi.x, lo.y), hi),
        2 => (Vec2::new(lo.x, hi.y), hi),
        _ => (lo, Vec2::new(lo.x, hi.y)),
    }
}

/// A traffic light's head facing one approach of a junction: where its three lenses are and the
/// way they face, so the game can light the right one (signal heads change, so their lamps are
/// drawn live, not baked).
#[derive(Clone, Debug)]
pub struct SignalHead {
    pub junction: u32,
    /// The approach's road runs north-south.
    pub ns: bool,
    pub red: Vec3,
    pub amber: Vec3,
    pub green: Vec3,
    pub facing: Vec3,
}

/// Mast-arm signals at every signalised junction: a pole on the far-right corner of each
/// approach, its arm reaching over the lanes, the head facing the drivers.
pub(crate) fn signals(grid: &Grid, lanes: &crate::lanes::LaneGraph, out: &mut Out) -> Vec<SignalHead> {
    let mut heads = Vec::new();
    for (j, jn) in lanes.junctions.iter().enumerate() {
        if jn.signal.is_none() {
            continue;
        }
        let centre = jn.centre();
        let half = (jn.max - jn.min) * 0.5;
        for (s, d) in DIRS.iter().enumerate() {
            if !jn.sides[s] {
                continue;
            }
            // Outward normal of the side, the driver's heading and right.
            let n = Vec2::new(d.x as f32, d.y as f32);
            let h = -n;
            let r = Vec2::new(-h.y, h.x);
            let half_h = if h.x != 0.0 { half.x } else { half.y };
            let half_r = if r.x != 0.0 { half.x } else { half.y };
            let pole = centre + h * (half_h - 1.0) + r * (half_r - 1.0);
            let y = ROAD_Y + LOT_Y;
            let at = Vec3::new(pole.x, y, pole.y);
            let facing = Vec3::new(n.x, 0.0, n.y);
            let yaw = yaw_to(facing);
            // Wide approaches get the long arm.
            let wide = half_r * 2.0 >= grid.cell * 2.0 - 0.1;
            let (asset, reach) =
                if wide { ("LIB/gtf_city/traffic_signal:long", 10.267) } else { ("LIB/gtf_city/traffic_signal", 7.0) };
            out.props.push(prop(asset, at, yaw, 1.0, 0));
            let rot = Quat::from_rotation_y(yaw);
            let lens = |h_cm: f32| at + rot * Vec3::new(-reach, h_cm / 100.0, 0.15);
            heads.push(SignalHead {
                junction: j as u32,
                ns: s == 0 || s == 2,
                red: lens(594.0),
                amber: lens(562.0),
                green: lens(530.0),
                facing,
            });
        }
    }
    heads
}

/// The docks: stacks of containers in the open yard, gantry cranes along the quay.
pub(crate) fn docks(def: &DistrictDef, grid: &Grid, out: &mut Out) {
    let taken = footprints(grid, out);
    let presets = ["", ":blue", ":green", ":grey", ":orange"];
    let mut quay_x = f32::MIN;
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
            let Some(zone) = grid.zone(c) else { continue };
            if zone != "docks" || grid.landmark_at(c).is_some() {
                continue;
            }
            let lo = grid.corner(c);
            let mut r = rng(def.seed, 0xd0c5, x, z);
            // A gantry crane every ~90 m where the yard meets the water to the south.
            let water_south = grid.zone(c + IVec2::Y).is_some_and(|n| def.zones[n].ground == "water");
            if water_south && lo.x > quay_x + 90.0 {
                quay_x = lo.x;
                out.props.push(prop("LIB/gtf_city/gantry_crane", Vec3::new(lo.x + 8.0, LOT_Y, lo.y + 6.0), 0.0, 1.0, 0));
                continue;
            }
            let near = taken.get(&(x, z));
            let free = |p: Vec2| {
                near.is_none_or(|v| {
                    !v.iter().any(|(a, b)| p.x > a.x - 7.0 && p.y > a.y - 2.0 && p.x < b.x + 7.0 && p.y < b.y + 2.0)
                })
            };
            let mid = lo + Vec2::splat(grid.cell * 0.5);
            if !free(mid) || r.f32() < 0.35 {
                continue;
            }
            // A stack: one or two rows of containers, one to three high, along x.
            let rows = 1 + r.below(2) as i32;
            for row in 0..rows {
                let high = 1 + r.below(3);
                for k in 0..high {
                    let preset = presets[r.below(presets.len() as u32) as usize];
                    let p = Vec3::new(mid.x, LOT_Y + k as f32 * 2.59, mid.y - 1.3 + row as f32 * 2.6);
                    out.props.push(prop(
                        &format!("LIB/gtf_city/shipping_container{preset}"),
                        p,
                        std::f32::consts::FRAC_PI_2,
                        1.0,
                        0,
                    ));
                }
            }
            if r.f32() < 0.4 {
                out.props.push(prop(
                    "LIB/gtf_city/pallet_stack",
                    Vec3::new(lo.x + 2.0, LOT_Y, lo.y + 2.0),
                    r.range(0.0, 3.0),
                    1.0,
                    0,
                ));
            }
        }
    }
}

/// The beach: lifeguard towers along the water, umbrellas, a volleyball net or two.
pub(crate) fn beach(def: &DistrictDef, grid: &Grid, out: &mut Out) {
    let colours = ["", ":coral", ":mint", ":pink", ":yellow"];
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
            let Some(zone) = grid.zone(c) else { continue };
            if def.zones[zone].ground != "sand" || grid.landmark_at(c).is_some() {
                continue;
            }
            let lo = grid.corner(c);
            let mut r = rng(def.seed, 0xbeac, x, z);
            let by_water = grid.zone(c + IVec2::Y).is_some_and(|n| def.zones[n].ground == "water");
            if by_water && x % 6 == 2 {
                let tint = colours[r.below(colours.len() as u32) as usize];
                out.props.push(prop(
                    &format!("LIB/gtf_city/lifeguard_tower{tint}"),
                    Vec3::new(lo.x + 8.0, LOT_Y, lo.y + 6.0),
                    std::f32::consts::PI,
                    1.0,
                    0,
                ));
            } else if r.f32() < 0.22 {
                let p = lo + Vec2::new(r.range(3.0, 13.0), r.range(3.0, 13.0));
                out.props.push(prop(
                    "LIB/gtf_city/beach_umbrella",
                    Vec3::new(p.x, LOT_Y, p.y),
                    r.range(0.0, 6.3),
                    1.0,
                    r.below(1000),
                ));
            } else if r.f32() < 0.02 {
                out.props.push(prop("LIB/gtf_city/volleyball_net", Vec3::new(lo.x + 8.0, LOT_Y, lo.y + 8.0), 0.0, 1.0, 0));
            }
            if r.f32() < 0.05 {
                out.props.push(prop("LIB/gtf_city/trash_can_beach", Vec3::new(lo.x + 2.0, LOT_Y, lo.y + 2.0), 0.0, 1.0, 0));
            }
        }
    }
}
