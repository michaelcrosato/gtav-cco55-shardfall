//! Static regions reach the renderer as shared, bounded instance lists: night-only decor carries
//! the renderer's night flag (its light follows the street lights), small dressing and detail
//! carry their distance classes, and a region's bounds hold everything in it.

use std::sync::Arc;

use glam::{Quat, Vec3};
use pav_core::shape::{Look, Shape, Sway};
use pav_core::statics::{ChunkKey, Decor, Glow, StaticChunk};
use pav_core::{Color, Sim};
use pav_render::scene::{MeshInstance, Scene, flags};
use pav_view::atmosphere::Atmosphere;
use pav_view::build::{ViewBuilder, ViewSettings};
use pav_view::camera::CameraRig;

fn decor(shape: Shape, pos: Vec3, emissive: f32, glow: Glow) -> Decor {
    Decor {
        shape,
        pos,
        rot: Quat::IDENTITY,
        color: Color::hex("#fff1d0"),
        look: Look::Cel,
        emissive,
        solid: false,
        collider: None,
        sway: Sway::None,
        material: None,
        glow,
    }
}

const LAMP: Vec3 = Vec3::new(0.0, 6.3, 0.0);
const BIN: Vec3 = Vec3::new(2.0, 0.35, 0.0);
const TRUNK: Vec3 = Vec3::new(4.0, 4.0, 0.0);
const TRIM: Vec3 = Vec3::new(0.0, 12.0, 5.0);

fn scene(atmosphere: Atmosphere) -> Scene {
    let mut sim = Sim::new("empty", 1).unwrap();
    let mut frame = sim.frame();
    let chunk = StaticChunk {
        decor: vec![
            decor(Shape::Box { half: Vec3::new(0.2, 0.07, 0.38) }, LAMP, 3.0, Glow::Night),
            decor(Shape::Cylinder { half_height: 0.35, radius: 0.25 }, BIN, 0.0, Glow::Always),
            decor(Shape::Cylinder { half_height: 4.0, radius: 0.2 }, TRUNK, 0.0, Glow::Always),
            decor(Shape::Box { half: Vec3::new(20.0, 0.6, 0.1) }, TRIM, 0.0, Glow::Always),
        ],
        version: 1,
        ..Default::default()
    };
    frame.statics.chunks.insert(ChunkKey::Chunk(7, 7), Arc::new(chunk));
    let view = ViewSettings { atmosphere, ..Default::default() };
    ViewBuilder::new().build(&frame, &frame, 1.0, &CameraRig::default(), 16.0 / 9.0, &view, Vec3::ZERO)
}

fn at(s: &Scene, pos: Vec3) -> MeshInstance {
    *s.all_meshes().find(|m| m.transform.w_axis.truncate().distance(pos) < 1e-4).expect("drawn")
}

#[test]
fn night_glows_and_distance_classes_ride_on_the_instances() {
    let day = scene(Atmosphere::Scene);
    let class = flags::SMALL | flags::DETAIL;
    let lamp = at(&day, LAMP);
    assert!(lamp.flags & flags::NIGHT != 0 && lamp.flags & class == 0, "glowing things never drop out: {}", lamp.flags);
    assert_eq!(at(&day, BIN).flags & class, flags::SMALL);
    assert_eq!(at(&day, TRUNK).flags & class, flags::DETAIL);
    assert_eq!(at(&day, TRIM).flags & class, 0, "big things stay to the fog's end");
    // Statics go to the renderer as regions, not as loose instances.
    assert!(day.meshes.iter().all(|m| m.transform.w_axis.truncate().distance(LAMP) > 1e-4));
    // The lamp's light shows only as far as the street lights are on.
    let lamp_light = |s: &Scene| s.point_lights.iter().find(|l| l.position.distance(LAMP) < 1e-4).map(|l| l.color);
    assert!(lamp_light(&day).is_none(), "off by day");
    let night = scene(Atmosphere::Night);
    assert_eq!(night.street_lights, 1.0);
    let lit = lamp_light(&night).expect("on at night");
    assert!((lit - Vec3::from(Color::hex("#fff1d0").0) * 3.0).length() < 1e-4, "{lit}");
    let dusk = scene(Atmosphere::Dusk);
    let dim = lamp_light(&dusk).expect("on at dusk");
    assert!((dim - lit * dusk.street_lights).length() < 1e-4);
}

#[test]
fn a_regions_bounds_hold_everything_in_it() {
    let s = scene(Atmosphere::Scene);
    assert!(!s.regions.is_empty());
    for r in &s.regions {
        for m in r.meshes.iter() {
            let p = m.transform.w_axis.truncate();
            assert!(p.cmpge(r.min).all() && p.cmple(r.max).all(), "{p} outside {} .. {}", r.min, r.max);
        }
        let inside = |p: Vec3| p.cmpge(r.min).all() && p.cmple(r.max).all();
        for d in r.sdfs.iter() {
            assert!(inside(d.a) && inside(d.b), "{} {} outside {} .. {}", d.a, d.b, r.min, r.max);
        }
    }
    // The trim's ends (20 m either side of its centre) are inside its region's box too.
    let region = s.regions.iter().find(|r| r.meshes.iter().any(|m| m.transform.w_axis.truncate().distance(TRIM) < 1e-4)).unwrap();
    assert!(region.min.x <= TRIM.x - 20.0 && region.max.x >= TRIM.x + 20.0);
    // The same region's lists are shared by the next frame's scene, not copied.
    let mut builder = ViewBuilder::new();
    let mut sim = Sim::new("empty", 1).unwrap();
    let frame = sim.frame();
    let view = ViewSettings::default();
    let a = builder.build(&frame, &frame, 1.0, &CameraRig::default(), 16.0 / 9.0, &view, Vec3::ZERO);
    let b = builder.build(&frame, &frame, 1.0, &CameraRig::default(), 16.0 / 9.0, &view, Vec3::ZERO);
    for (ra, rb) in a.regions.iter().zip(&b.regions) {
        assert!(Arc::ptr_eq(&ra.meshes, &rb.meshes) && Arc::ptr_eq(&ra.sdfs, &rb.sdfs));
    }
}
