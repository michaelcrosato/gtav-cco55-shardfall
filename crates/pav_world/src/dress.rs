//! Street dressing (PLAN.md §6.6.5): along every sidewalk, by the rules of the zone behind it —
//! street lamps, palms and street trees, benches, bins, hydrants and billboards with satirical
//! ads; trees and benches in parks, palms on the beach. Deterministic per cell. Simple shapes
//! for now; library props (`LIB/gtf_city`) replace them as they are authored.

use glam::{IVec2, Quat, Vec2, Vec3};
use pav_core::Color;
use pav_core::rng::Rng;
use pav_core::shape::{Look, Shape, Sway};
use pav_core::statics::Decor;
use pav_core::zones::{Label, LabelMode};

use crate::def::{DistrictDef, RoadClass};
use crate::grid::{Cell, DIRS, Grid};
use crate::ground::{LOT_Y, SIDEWALK};
use crate::{Out, rng};

fn decor(shape: Shape, pos: Vec3, rot: Quat, color: &str) -> Decor {
    Decor {
        shape,
        pos,
        rot,
        color: Color::hex(color),
        look: Look::Cel,
        emissive: 0.0,
        solid: false,
        collider: None,
        sway: Sway::None,
        material: None,
    }
}

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

pub fn lamp(at: Vec3, toward: Vec3, out: &mut Vec<Decor>) {
    let h = 6.4;
    out.push(decor(Shape::Cylinder { half_height: h * 0.5, radius: 0.09 }, at + Vec3::Y * h * 0.5, Quat::IDENTITY, "#4b5157"));
    let arm = at + Vec3::Y * h + toward * 0.75;
    let yaw = toward.x.atan2(toward.z);
    out.push(decor(Shape::Box { half: Vec3::new(0.05, 0.05, 0.75) }, arm, Quat::from_rotation_y(yaw), "#4b5157"));
    let mut head = decor(
        Shape::Box { half: Vec3::new(0.2, 0.07, 0.38) },
        arm + toward * 0.7 - Vec3::Y * 0.08,
        Quat::from_rotation_y(yaw),
        "#fff1d0",
    );
    head.emissive = 0.4;
    out.push(head);
}

pub fn palm(at: Vec3, r: &mut Rng, out: &mut Vec<Decor>) {
    let h = r.range(7.5, 11.5);
    out.push(decor(Shape::Cylinder { half_height: h * 0.5, radius: 0.19 }, at + Vec3::Y * h * 0.5, Quat::IDENTITY, "#8a6f50"));
    let top = at + Vec3::Y * h;
    out.push(decor(Shape::Sphere { radius: 0.38 }, top, Quat::IDENTITY, "#6a5638"));
    let n = 7;
    let spin = r.range(0.0, std::f32::consts::TAU);
    for i in 0..n {
        let a = spin + i as f32 / n as f32 * std::f32::consts::TAU;
        let dir = Vec3::new(a.sin(), 0.0, a.cos());
        let rot = Quat::from_rotation_y(a) * Quat::from_rotation_x(0.42);
        let mut frond = decor(Shape::Box { half: Vec3::new(0.32, 0.04, 2.0) }, top + dir * 1.75 - Vec3::Y * 0.65, rot, "#4d7a37");
        frond.sway = Sway::Leaves;
        out.push(frond);
    }
}

pub fn tree(at: Vec3, r: &mut Rng, out: &mut Vec<Decor>) {
    let h = r.range(2.4, 3.4);
    out.push(decor(Shape::Cylinder { half_height: h * 0.5, radius: 0.17 }, at + Vec3::Y * h * 0.5, Quat::IDENTITY, "#6b553e"));
    let rad = r.range(1.6, 2.5);
    let green = ["#4e7a3a", "#5b8a40", "#6a8f46", "#45703a"][r.below(4) as usize];
    let mut crown = decor(Shape::Sphere { radius: rad }, at + Vec3::Y * (h + rad * 0.8), Quat::IDENTITY, green);
    crown.sway = Sway::Leaves;
    out.push(crown);
}

fn bench(at: Vec3, toward: Vec3, out: &mut Vec<Decor>) {
    let yaw = toward.x.atan2(toward.z) + std::f32::consts::FRAC_PI_2;
    let rot = Quat::from_rotation_y(yaw);
    out.push(decor(Shape::Box { half: Vec3::new(0.9, 0.05, 0.24) }, at + Vec3::Y * 0.46, rot, "#6f5b47"));
    out.push(decor(Shape::Box { half: Vec3::new(0.9, 0.22, 0.04) }, at + Vec3::Y * 0.78 - toward * 0.24, rot, "#6f5b47"));
    for s in [-0.75f32, 0.75] {
        out.push(decor(Shape::Box { half: Vec3::new(0.04, 0.23, 0.22) }, at + rot * Vec3::new(s, 0.23, 0.0), rot, "#3d4247"));
    }
}

fn billboard(at: Vec3, toward: Vec3, text: &str, out: &mut Out) {
    let yaw = toward.x.atan2(toward.z);
    let rot = Quat::from_rotation_y(yaw);
    let side = rot * Vec3::X;
    for s in [-2.6f32, 2.6] {
        out.decor.push(decor(
            Shape::Cylinder { half_height: 3.4, radius: 0.16 },
            at + side * s + Vec3::Y * 3.4,
            Quat::IDENTITY,
            "#5a6066",
        ));
    }
    out.decor.push(decor(Shape::Box { half: Vec3::new(3.8, 1.7, 0.12) }, at + Vec3::Y * 8.2, rot, "#f4f1ea"));
    out.labels.push(Label {
        text: text.to_string(),
        pos: at + Vec3::Y * 8.2 + toward * 0.15,
        size: 0.55,
        color: Color::hex("#e8433a"),
        mode: LabelMode::Billboard,
        facing: Default::default(),
    });
}

pub fn build(def: &DistrictDef, grid: &Grid, out: &mut Out) {
    let cs = grid.cell;
    for z in 0..grid.h {
        for x in 0..grid.w {
            let c = IVec2::new(x, z);
            let lo = grid.corner(c);
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
                            lamp(kerb(cs * 0.5, 0.45), inward, &mut out.decor);
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
                                        "palm" => palm(kerb(t, 0.7), &mut r, &mut out.decor),
                                        "tree" => tree(kerb(t, 0.8), &mut r, &mut out.decor),
                                        "bench" => bench(kerb(t, 1.5), inward, &mut out.decor),
                                        "bin" => out.decor.push(decor(
                                            Shape::Cylinder { half_height: 0.45, radius: 0.27 },
                                            kerb(t, 1.6) + Vec3::Y * 0.45,
                                            Quat::IDENTITY,
                                            "#3b5a3f",
                                        )),
                                        "hydrant" => out.decor.push(decor(
                                            Shape::Cylinder { half_height: 0.35, radius: 0.13 },
                                            kerb(t, 0.5) + Vec3::Y * 0.35,
                                            Quat::IDENTITY,
                                            "#c0392b",
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
                                        tree(Vec3::new(p.x, LOT_Y, p.y), &mut r, &mut out.decor);
                                    }
                                }
                            }
                            if r.f32() < 0.3 {
                                let p = lo + Vec2::splat(cs * 0.5);
                                bench(Vec3::new(p.x, LOT_Y, p.y), Vec3::Z, &mut out.decor);
                            }
                        }
                        // The beach: a few palms.
                        "sand" if r.f32() < 0.25 => {
                            let p = lo + Vec2::new(r.range(3.0, 13.0), r.range(3.0, 13.0));
                            palm(Vec3::new(p.x, LOT_Y, p.y), &mut r, &mut out.decor);
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
