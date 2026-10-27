//! Drawing vehicles. A car or a helicopter with a body prop (`VehicleView::body_asset`) is that
//! prop, placed where the ground under its centre would be: its door parts swing by the doors'
//! open amounts, parts in a dented crumple zone sit pushed in, and its lamp parts glow by the
//! lamps' states (`head_*` with the headlights, `tail_*` brighter braking, `rev_*` reversing,
//! `siren_*` flashing red and blue in turn while the siren is on). A helicopter's `rotor` group
//! turns by `VehicleView::rotor` about the mast (the body's up, through the middle of the
//! group) and its `tail_rotor` group 1.7 times as fast about its own axle (the way the group is
//! thinnest, through its middle); while it is crewed its `beacon*` parts blink red and its
//! `nav*` lights glow. A car's wheels are the wheel prop, spun and steered; occupants are simple
//! busts in their seats; a damaged vehicle smokes, a burning one burns, a wreck is charred.
//! Without a body prop (or when it is missing) a car is the collider's box with a cabin, wheels
//! and lamps, and a helicopter the box with a cockpit, tail, crossed rotor blades and skids. No
//! lights are added except the driven car's headlight beam.

use std::collections::BTreeMap;
use std::sync::Mutex;

use glam::{Quat, Vec3};
use pav_core::color::Color;
use pav_core::frame::RenderObject;
use pav_core::props::ResolvedPart;
use pav_core::shape::Shape;
use pav_core::vehicle::{Side, VehicleKind, VehicleView, WheelView};
use pav_render::{self as rs, Scene, Style};

use crate::assets::{Palette, PartTweak, Placement, emit_asset, emit_asset_tweaked};
use crate::build::emit_shape;

fn hex(s: &str, fallback: &str) -> Vec3 {
    Vec3::from(Color::try_hex(s).unwrap_or(Color::hex(fallback)).0)
}

/// The wheel prop's authored radius (m): a wheel is drawn at its own radius over this.
pub const WHEEL_PROP_RADIUS: f32 = 0.33;

/// Beyond this far from the camera (m) a wheel is a plain dark cylinder, not the wheel prop.
const WHEEL_DETAIL: f32 = 60.0;
/// How far a door swings open (radians).
const DOOR_SWING: f32 = 1.15;
/// A helicopter body's spinning groups and how fast each turns against `VehicleView::rotor`.
const ROTORS: [(&str, f32); 2] = [("rotor", 1.0), ("tail_rotor", 1.7)];

/// Asset names that failed to draw (reported once each).
static FAILED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn report(name: &str, error: &str) {
    let mut failed = FAILED.lock().unwrap_or_else(|e| e.into_inner());
    if !failed.iter().any(|n| n == name) {
        failed.push(name.to_string());
        eprintln!("vehicle prop {name}: {error}");
    }
}

/// A guard's vision cone as a flat fan on the floor: yellow, turning red as the alert rises.
pub fn emit_cone(scene: &mut Scene, c: &pav_core::stealth::ConeView, group: u32) {
    if c.points.len() < 2 {
        return;
    }
    let calm = Vec3::new(1.0, 0.85, 0.3);
    let alarm = Vec3::new(1.0, 0.18, 0.12);
    let color = calm.lerp(alarm, c.alert.clamp(0.0, 1.0));
    let mut data = pav_render::MeshData::default();
    let v = |p: Vec3, a: f32| pav_render::mesh::Vertex {
        pos: p.to_array(),
        normal: [0.0, 1.0, 0.0],
        uv: [0.0, 0.0],
        color: [a, a, a, 1.0],
    };
    data.vertices.push(v(c.origin, 1.0));
    for p in &c.points {
        data.vertices.push(v(*p, 0.75));
    }
    for i in 1..c.points.len() as u32 {
        data.indices.extend_from_slice(&[0, i, i + 1]);
    }
    scene.dynamic.push(rs::DynamicMesh {
        data,
        color,
        emissive: 0.15,
        style: Style::Unlit,
        flags: rs::flags::NO_SHADOW | rs::flags::TWO_SIDED | rs::flags::NO_CUT,
        group,
        material: 0,
    });
}

/// A point of the body frame (the ground under the centre at rest is the origin) in the world.
fn world(o: &RenderObject, v: &VehicleView, p: Vec3) -> Vec3 {
    o.pos + o.rot * (p - Vec3::Y * v.base)
}

/// The last segment of a part id (`lamps.head_l` → `head_l`, `head~x` → `head~x`).
fn leaf(id: &str) -> &str {
    id.rsplit('.').next().unwrap_or(id)
}

/// Whether part `id` is the node `name` or inside that group (`door_l`, `door_l.glass`).
fn in_group(id: &str, name: &str) -> bool {
    id.strip_prefix(name).is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
}

/// The box round the parts of group `name` (body frame), or None when it has none.
fn group_bounds(parts: &[ResolvedPart], name: &str) -> Option<(Vec3, Vec3)> {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in parts.iter().filter(|p| in_group(&p.id, name)) {
        lo = lo.min(p.centre - p.extent);
        hi = hi.max(p.centre + p.extent);
    }
    (lo.x <= hi.x).then_some((lo, hi))
}

/// The swing of each door, by part name: its pivot (body frame) and turn.
fn door_turns(v: &VehicleView, parts: &[ResolvedPart]) -> BTreeMap<String, (Vec3, Quat)> {
    let mut out = BTreeMap::new();
    for d in v.doors.iter().filter(|d| d.open > 0.001) {
        let left = d.side == Side::Left;
        let pivot = d.hinge.unwrap_or_else(|| {
            // The front edge of the door's parts, on its outer face, half way up.
            let Some((lo, hi)) = group_bounds(parts, &d.part) else { return Vec3::ZERO };
            Vec3::new(if left { hi.x } else { lo.x }, (lo.y + hi.y) * 0.5, hi.z)
        });
        // The back of a left door swings out to +x (a turn about -y), a right door's to -x.
        let angle = d.open.clamp(0.0, 1.0) * DOOR_SWING * if left { -1.0 } else { 1.0 };
        out.insert(d.part.clone(), (pivot, Quat::from_rotation_y(angle)));
    }
    out
}

/// The turn of each rotor group of a helicopter's body (`ROTORS`), by group name: its pivot
/// (body frame, the middle of the group's parts) and turn. The main rotor turns about the
/// mast (the body's up); the tail rotor about its own axle, the way its parts are thinnest.
fn rotor_turns(v: &VehicleView, parts: &[ResolvedPart]) -> BTreeMap<String, (Vec3, Quat)> {
    let mut out = BTreeMap::new();
    if v.kind != VehicleKind::Helicopter {
        return out;
    }
    for (name, rate) in ROTORS {
        let Some((lo, hi)) = group_bounds(parts, name) else { continue };
        let size = hi - lo;
        let axis = if name == "rotor" || (size.y <= size.x && size.y <= size.z) {
            Vec3::Y
        } else if size.x <= size.z {
            Vec3::X
        } else {
            Vec3::Z
        };
        out.insert(name.to_string(), ((lo + hi) * 0.5, Quat::from_axis_angle(axis, v.rotor * rate)));
    }
    out
}

/// What moves and glows on a body prop this frame.
struct BodyFrame {
    doors: BTreeMap<String, (Vec3, Quat)>,
    rotors: BTreeMap<String, (Vec3, Quat)>,
    /// Light bars alternate four times a second (`lit_red`: the red lamps' beat), each colour
    /// flashing twice (`flash`: lit this instant).
    lit_red: bool,
    flash: bool,
    /// A crewed helicopter's beacons: lit this instant (a short blink each second).
    beacon: bool,
}

impl BodyFrame {
    fn new(v: &VehicleView, parts: &[ResolvedPart], time: f32) -> Self {
        let t = time * 4.0;
        Self {
            doors: door_turns(v, parts),
            rotors: rotor_turns(v, parts),
            lit_red: (t.floor() as i64) % 2 == 0,
            flash: (t * 2.0).fract() < 0.7,
            beacon: v.driven && time.fract() < 0.14,
        }
    }
}

/// What a part of a body prop does this frame: doors, rotors, dents, lamps, a wreck's char.
fn body_tweak(v: &VehicleView, p: &ResolvedPart, f: &BodyFrame) -> Option<PartTweak> {
    let mut t = PartTweak::default();
    let mut changed = false;
    let door = f.doors.iter().find(|(name, _)| in_group(&p.id, name));
    if let Some((_, turn)) = door {
        t.turn = Some(*turn);
        changed = true;
    }
    let rotor = f.rotors.iter().find(|(name, _)| in_group(&p.id, name));
    if let Some((_, turn)) = rotor {
        t.turn = Some(*turn);
        changed = true;
    }
    // Dents: parts in a crumpled zone's outer part are pushed in (and a little down); a
    // spinning rotor keeps its shape.
    let h = v.half;
    let c = p.centre;
    let mut push = Vec3::ZERO;
    if v.crumple[0] > 0.0 && c.z > 0.55 * h.z {
        push += Vec3::new(0.0, -0.04, -0.22) * v.crumple[0];
    }
    if v.crumple[1] > 0.0 && c.z < -0.55 * h.z {
        push += Vec3::new(0.0, -0.04, 0.22) * v.crumple[1];
    }
    if v.crumple[2] > 0.0 && c.x > 0.55 * h.x {
        push += Vec3::new(-0.15, -0.02, 0.0) * v.crumple[2];
    }
    if v.crumple[3] > 0.0 && c.x < -0.55 * h.x {
        push += Vec3::new(0.15, -0.02, 0.0) * v.crumple[3];
    }
    if push != Vec3::ZERO && rotor.is_none() {
        t.offset = push;
        changed = true;
    }
    // Lamps.
    let name = leaf(&p.id);
    let left = c.x > 0.0;
    let lamps = &v.lamps;
    let dark = Vec3::splat(0.06);
    if name.starts_with("head") {
        let broken = lamps.broken[if left { 0 } else { 1 }];
        t.emissive = Some(if broken {
            0.0
        } else if lamps.head {
            5.0
        } else {
            0.15
        });
        if broken {
            t.color = Some(dark);
        }
        changed = true;
    } else if name.starts_with("tail") {
        let broken = lamps.broken[if left { 2 } else { 3 }];
        t.emissive = Some(if broken {
            0.0
        } else if lamps.brake {
            4.0
        } else if lamps.head {
            1.4
        } else {
            0.25
        });
        if broken {
            t.color = Some(dark);
        }
        changed = true;
    } else if name.starts_with("rev") {
        t.emissive = Some(if lamps.reverse { 3.0 } else { 0.0 });
        changed = true;
    } else if name.starts_with("siren") {
        // Red lamps on one beat, blue on the other.
        let red = p.color.as_deref().and_then(Color::try_hex).map(|c| c.0[0] > c.0[2]).unwrap_or(left);
        let on = lamps.siren_on && (red == f.lit_red) && f.flash;
        t.emissive = Some(if on { 7.0 } else { 0.2 });
        changed = true;
    } else if v.kind == VehicleKind::Helicopter && name.starts_with("beacon") {
        t.emissive = Some(if f.beacon { 6.0 } else { 0.15 });
        changed = true;
    } else if v.kind == VehicleKind::Helicopter && name.starts_with("nav") {
        t.emissive = Some(if v.driven { 2.5 } else { 0.1 });
        changed = true;
    }
    if v.wrecked {
        t.color = Some(t.color.unwrap_or(Vec3::splat(0.07)));
        t.emissive = Some(0.0);
        changed = true;
    }
    changed.then_some(t)
}

/// Draws a body prop; Err when the prop is missing (the caller draws a box instead).
fn emit_body(scene: &mut Scene, o: &RenderObject, v: &VehicleView, name: &str) -> Result<(), String> {
    let (_, def, _) = pav_core::props::resolve_placed(name, &pav_core::props::Params::new())?;
    let frame = BodyFrame::new(v, &def.compiled.parts, scene.time);
    let mut palette = Palette::new();
    palette.insert("$accent".into(), v.color.clone());
    palette.insert("$pillar".into(), v.accent.clone());
    let at = Placement::at(world(o, v, Vec3::ZERO)).rot(o.rot).id(o.id.0);
    emit_asset_tweaked(scene, name, at, 0, &palette, &|p| body_tweak(v, p, &frame))
}

/// One wheel from the wheel prop: steered, spun, the hub facing out.
fn emit_wheel_prop(scene: &mut Scene, o: &RenderObject, v: &VehicleView, w: &WheelView, name: &str) -> Result<(), String> {
    let scale = (w.radius / WHEEL_PROP_RADIUS).clamp(0.3, 4.0);
    let out = if w.left { Quat::IDENTITY } else { Quat::from_rotation_y(std::f32::consts::PI) };
    let spin = if w.left { w.spin } else { -w.spin };
    let rot = o.rot * Quat::from_rotation_y(w.steer) * out * Quat::from_rotation_x(spin);
    let centre = world(o, v, w.center);
    let origin = centre - rot * Vec3::Y * (WHEEL_PROP_RADIUS * scale);
    emit_asset(scene, name, Placement::at(origin).rot(rot).scale(scale).id(o.id.0), 0, &Palette::new())
}

/// Adds a vehicle's parts (and smoke, fire, tyre smoke) to the scene.
pub fn emit_vehicle(scene: &mut Scene, o: &RenderObject, v: &VehicleView, style: Style, particles: bool) {
    let group = o.id.0 + 2;
    let rot = o.rot;
    let (fwd, right, up) = (rot * Vec3::Z, rot * Vec3::X, rot * Vec3::Y);
    let h = v.half;
    let body = hex(&v.color, "#e8443a");
    let accent = hex(&v.accent, "#2b2f3a");
    let mut lights = Vec::new();
    let mut part = |scene: &mut Scene, shape: Shape, pos: Vec3, r: Quat, color: Vec3, emissive: f32| {
        emit_shape(&mut scene.meshes, &mut scene.sdfs, &mut lights, &shape, pos, r, color, emissive, style, group, 0, 0);
    };
    let capsule = |scene: &mut Scene, a: Vec3, b: Vec3, ra: f32, rb: f32, color: Vec3| {
        scene.sdfs.push(rs::SdfInstance { a, b, ra, rb, color, emissive: 0.0, style, flags: 0, group, material: 0 });
    };
    let char_tint = |c: Vec3| if v.wrecked { c * 0.15 } else { c };
    // The body prop (a car's or a helicopter's); when it is missing the collider's box stands in.
    let drawn = match v.body_asset.as_deref() {
        Some(name) => match emit_body(scene, o, v, name) {
            Ok(()) => true,
            Err(e) => {
                report(name, &e);
                false
            }
        },
        None => false,
    };
    if !drawn && v.body_asset.is_some() {
        part(scene, Shape::RoundedBox { half: h, radius: h.y.min(h.x) * 0.45 }, o.pos, rot, char_tint(body), 0.0);
    }
    match v.kind {
        VehicleKind::Car => {
            if !drawn {
                // Cabin with dark glass, set back a little.
                let cabin = o.pos + up * (h.y * 1.55) - fwd * (h.z * 0.12);
                part(
                    scene,
                    Shape::RoundedBox { half: Vec3::new(h.x * 0.82, h.y * 0.7, h.z * 0.46), radius: h.y * 0.35 },
                    cabin,
                    rot,
                    char_tint(accent),
                    0.0,
                );
                // Head and tail lamps: glowing, not lights themselves (emissive up to 0.5).
                let lamps = &v.lamps;
                let head_glow = if lamps.head { 0.5 } else { 0.3 };
                let tail_glow = if lamps.brake { 0.5 } else { 0.25 };
                let heads: Vec<Vec3> = if v.head.is_empty() {
                    [-1.0f32, 1.0]
                        .iter()
                        .map(|s| o.pos + fwd * (h.z * 0.98) + right * (s * h.x * 0.62) + up * (h.y * 0.15))
                        .collect()
                } else {
                    v.head.iter().map(|p| world(o, v, *p)).collect()
                };
                let tails: Vec<Vec3> = if v.tail.is_empty() {
                    [-1.0f32, 1.0]
                        .iter()
                        .map(|s| o.pos - fwd * (h.z * 0.98) + right * (s * h.x * 0.62) + up * (h.y * 0.15))
                        .collect()
                } else {
                    v.tail.iter().map(|p| world(o, v, *p)).collect()
                };
                for p in heads {
                    part(scene, Shape::Sphere { radius: 0.11 }, p, rot, Vec3::new(1.0, 0.95, 0.8) * 1.6, head_glow);
                }
                for p in tails {
                    part(scene, Shape::Sphere { radius: 0.09 }, p, rot, Vec3::new(1.0, 0.12, 0.08) * 1.4, tail_glow);
                }
            }
            // Wheels: the wheel prop, else cylinders turned onto the axle with a hub; far off
            // (a few pixels), the cylinder alone.
            let far = o.pos.distance(scene.camera.eye) > WHEEL_DETAIL;
            for w in &v.wheels {
                if far {
                    let q = rot * Quat::from_rotation_y(w.steer) * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
                    part(
                        scene,
                        Shape::Cylinder { half_height: 0.13, radius: w.radius },
                        world(o, v, w.center),
                        q,
                        Vec3::splat(0.08),
                        0.0,
                    );
                    continue;
                }
                let drawn = match v.wheel_asset.as_deref() {
                    Some(name) => match emit_wheel_prop(scene, o, v, w, name) {
                        Ok(()) => true,
                        Err(e) => {
                            report(name, &e);
                            false
                        }
                    },
                    None => false,
                };
                if !drawn {
                    let c = world(o, v, w.center);
                    let q = rot
                        * Quat::from_rotation_y(w.steer)
                        * Quat::from_rotation_x(w.spin)
                        * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
                    let r = if w.burst { w.radius * 0.95 } else { w.radius };
                    part(scene, Shape::Cylinder { half_height: 0.13, radius: r }, c, q, Vec3::splat(0.08), 0.0);
                    let hub = c + right * if w.left { 0.14 } else { -0.14 };
                    part(scene, Shape::Sphere { radius: r * 0.35 }, hub, rot, Vec3::splat(0.75), 0.0);
                }
            }
            // The driven car's headlight beam (the engine demo's cars: always while driven).
            if v.driven && (v.lamps.head || v.body_asset.is_none()) && !v.wrecked {
                scene.point_lights.push(rs::PointLight {
                    position: o.pos + fwd * (h.z + 2.0) + up * 0.4,
                    color: Vec3::new(1.0, 0.92, 0.75) * 0.9,
                    radius: if v.lamps.head { 9.0 } else { 5.0 },
                    shadows: false,
                });
            }
        }
        VehicleKind::Helicopter if !drawn => {
            // Cockpit bubble.
            let glass = o.pos + fwd * (h.z * 0.55) + up * (h.y * 0.25);
            part(scene, Shape::Sphere { radius: h.y * 0.82 }, glass, rot, Vec3::new(0.55, 0.75, 0.9), 0.0);
            // Tail boom, fin and tail rotor.
            let boom_a = o.pos - fwd * (h.z * 0.7) + up * (h.y * 0.2);
            let boom_b = o.pos - fwd * (h.z * 2.7) + up * (h.y * 0.45);
            capsule(scene, boom_a, boom_b, 0.16, 0.09, body);
            part(scene, Shape::Box { half: Vec3::new(0.04, 0.38, 0.22) }, boom_b + up * 0.3, rot, accent, 0.0);
            let tail_rot = rot * Quat::from_rotation_x(v.rotor * ROTORS[1].1);
            part(
                scene,
                Shape::Box { half: Vec3::new(0.02, 0.45, 0.05) },
                boom_b + right * 0.12 + up * 0.15,
                tail_rot,
                Vec3::splat(0.2),
                0.0,
            );
            // Mast and main rotor (two crossed blades).
            let mast = o.pos + up * (h.y + 0.25);
            capsule(scene, o.pos + up * (h.y * 0.8), mast, 0.07, 0.07, Vec3::splat(0.25));
            for k in 0..2 {
                let q = rot * Quat::from_rotation_y(v.rotor + k as f32 * std::f32::consts::FRAC_PI_2);
                part(scene, Shape::Box { half: Vec3::new(3.1, 0.025, 0.11) }, mast, q, Vec3::splat(0.18), 0.0);
            }
            // Skids on struts.
            for s in [-1.0f32, 1.0] {
                let base = o.pos + right * (s * h.x * 0.85) - up * (h.y + 0.28);
                capsule(scene, base - fwd * (h.z * 0.95), base + fwd * (h.z * 0.95), 0.06, 0.06, accent);
                for z in [-0.5f32, 0.5] {
                    let top = o.pos + right * (s * h.x * 0.7) - up * (h.y * 0.8) + fwd * (z * h.z);
                    capsule(scene, top, base + fwd * (z * h.z), 0.04, 0.04, accent);
                }
            }
            // Beacon; a police unit's light bar flashes red and blue under the nose.
            let blink = if v.driven && (v.rotor * 0.3).sin() > 0.6 { 0.5 } else { 0.1 };
            part(scene, Shape::Sphere { radius: 0.07 }, o.pos - up * (h.y * 0.9), rot, Vec3::new(1.0, 0.15, 0.1), blink);
            if v.lamps.siren_on {
                let red = (scene.time * 4.0).floor() as i64 % 2 == 0;
                for (s, c) in [(1.0f32, Vec3::new(1.0, 0.1, 0.1)), (-1.0, Vec3::new(0.1, 0.25, 1.0))] {
                    let on = (s > 0.0) == red;
                    let p = o.pos + fwd * (h.z * 0.7) - up * (h.y * 0.6) + right * (s * 0.25);
                    part(scene, Shape::Sphere { radius: 0.08 }, p, rot, c, if on { 0.5 } else { 0.05 });
                }
            }
        }
        VehicleKind::Helicopter => {}
    }
    // Occupants: a torso and a head in each taken seat.
    for (i, s) in v.seated.iter().enumerate() {
        let hip = world(o, v, *s);
        let shirt =
            [Vec3::new(0.18, 0.2, 0.26), Vec3::new(0.45, 0.12, 0.1), Vec3::new(0.85, 0.82, 0.75), Vec3::new(0.12, 0.3, 0.2)]
                [(o.id.0 as usize + i) % 4];
        capsule(scene, hip + up * 0.08, hip + up * 0.45, 0.17, 0.15, shirt);
        let head = hip + up * 0.7;
        capsule(scene, head, head, 0.11, 0.11, Vec3::new(0.62, 0.44, 0.33));
    }
    if particles {
        // Tyre smoke from a car's rear wheels while sliding.
        if v.kind == VehicleKind::Car && v.drift > 2.5 {
            for w in v.wheels.iter().skip(2) {
                let (mut b, _) = crate::fx::preset("smoke");
                b.pos = world(o, v, w.center) - Vec3::Y * (w.radius * 0.7);
                b.count = ((v.drift - 2.0) * 0.6).clamp(1.0, 6.0) as u32;
                b.size = (0.25, 1.0);
                b.life = (0.8, 1.4);
                b.color0 = glam::Vec4::new(0.85, 0.85, 0.88, 0.35);
                b.color1 = glam::Vec4::new(0.9, 0.9, 0.92, 0.0);
                b.vel = Vec3::Y * 0.5;
                scene.particles.push(b);
            }
        }
        // Damage: smoke from the engine (a car's under the bonnet, a helicopter's behind the
        // mast), then fire; a wreck smoulders.
        let engine = match v.kind {
            VehicleKind::Car => o.pos + fwd * (h.z * 0.6) + up * (h.y * 0.9),
            VehicleKind::Helicopter => o.pos - fwd * (h.z * 0.45) + up * (h.y * 1.3),
        };
        if v.burning {
            let (mut b, _) = crate::fx::preset("fire");
            b.pos = engine;
            b.count = 2;
            b.area = Vec3::new(h.x * 0.6, 0.1, h.z * 0.3);
            b.size = (0.5, 0.08);
            scene.particles.push(b);
        }
        if v.health < pav_core::vehicle::SMOKE_BELOW || v.wrecked {
            let (mut b, _) = crate::fx::preset("smoke");
            b.pos = engine;
            b.count = 1;
            let dark = if v.burning || v.wrecked { 0.12 } else { 0.5 - v.health };
            b.color0 = glam::Vec4::new(dark, dark, dark, 0.6);
            b.size = (0.3, 1.2);
            scene.particles.push(b);
        }
    }
    scene.point_lights.extend(lights);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pav_core::entity::EntityId;
    use pav_core::props::{self, Doc, PropDef};
    use pav_core::shape::Visual;
    use pav_core::vehicle::LampView;
    use std::sync::Arc;

    /// A small helicopter body: a cabin, a two-bladed main rotor over it, a tail rotor on the
    /// left of the fin, a beacon and a red/blue pair of siren lamps.
    const HELI: &str = r##"{"format":2,"name":"heli","unit":"cm","parts":{
        "cabin":{"type":"box","size":[160,140,380],"pos":[0,110,0],"color":"#2f6fb0"},
        "fin":{"type":"box","size":[6,90,40],"pos":[0,150,-500],"color":"#2f6fb0"},
        "rotor":{"color":"#333333","parts":{
            "hub":{"type":"cylinder","radius":14,"height":9,"pos":[0,248,20]},
            "blade":{"type":"box","size":[22,3,300],"pos":[0,249,20],"radial":{"count":2,"radius":162}}}},
        "tail_rotor":{"color":"#333333","parts":{
            "hub":{"type":"cylinder","radius":5,"height":8,"pos":[6,170,-500],"axis":"x"},
            "blade_a":{"type":"box","size":[2,100,10],"pos":[10,170,-500]},
            "blade_b":{"type":"box","size":[2,10,100],"pos":[10,170,-500]}}},
        "beacon":{"type":"box","size":[8,8,8],"pos":[0,184,-60],"color":"#e02424"},
        "siren_l":{"type":"box","size":[4,6,30],"pos":[82,60,40],"color":"#e02424"},
        "siren_r":{"type":"box","size":[4,6,30],"pos":[-82,60,40],"color":"#2050e8"}}}"##;

    /// The fixture installed as `WORKSHOP/<name>` (each test its own: tests run at once).
    fn install(name: &str) -> Arc<PropDef> {
        let mut doc = Doc::read(HELI).unwrap();
        doc.name = name.into();
        props::install(&format!("WORKSHOP/{name}"), PropDef::new(doc).unwrap()).unwrap();
        props::get(&format!("WORKSHOP/{name}")).unwrap()
    }

    fn heli(name: &str, rotor: f32) -> VehicleView {
        VehicleView {
            kind: VehicleKind::Helicopter,
            half: Vec3::new(0.9, 0.85, 2.0),
            base: 0.85,
            color: "#2f6fb0".into(),
            accent: "#f2f2f2".into(),
            body_asset: Some(format!("WORKSHOP/{name}")),
            wheel_asset: None,
            wheels: Vec::new(),
            rotor,
            drift: 0.0,
            driven: true,
            doors: Vec::new(),
            head: Vec::new(),
            tail: Vec::new(),
            lamps: LampView::default(),
            crumple: [0.0; 4],
            seated: Vec::new(),
            health: 1.0,
            burning: false,
            wrecked: false,
        }
    }

    fn object() -> RenderObject {
        RenderObject {
            id: EntityId(7),
            pos: Vec3::new(10.0, 0.85, -4.0),
            rot: Quat::from_rotation_y(0.6),
            visual: Visual::new(Shape::Sphere { radius: 0.0 }, Color::WHITE),
            prop: None,
            puppet: None,
            pulse: -1.0,
            soft: None,
            vehicle: None,
            cone: None,
            scenery: false,
        }
    }

    #[test]
    fn a_helicopter_body_turns_its_rotors_with_the_rotor_angle() {
        let def = install("view_heli_rotor");
        let parts = &def.compiled.parts;
        let angle = 0.7;
        let v = heli("view_heli_rotor", angle);
        let frame = BodyFrame::new(&v, parts, 0.0);
        let turn = |id: &str| {
            let p = parts.iter().find(|p| p.id == id).unwrap_or_else(|| panic!("no part {id}"));
            body_tweak(&v, p, &frame).and_then(|t| t.turn)
        };
        // The main rotor turns about the mast (up through the hub), the whole group alike.
        let (pivot, q) = turn("rotor.blade[0]").expect("the main rotor turns");
        assert_eq!(turn("rotor.blade[1]"), Some((pivot, q)));
        assert_eq!(turn("rotor.hub"), Some((pivot, q)));
        assert!(pivot.x.abs() < 1e-4 && (pivot.z - 0.2).abs() < 1e-4, "about the hub: {pivot}");
        let (axis, a) = q.to_axis_angle();
        assert!(axis.distance(Vec3::Y) < 1e-4 && (a - angle).abs() < 1e-4, "{axis} {a}");
        // The tail rotor turns faster, about its own axle (x, through its middle).
        let (pivot, q) = turn("tail_rotor.blade_a").expect("the tail rotor turns");
        assert_eq!(turn("tail_rotor.blade_b"), Some((pivot, q)));
        assert!((pivot.y - 1.7).abs() < 1e-4 && (pivot.z + 5.0).abs() < 1e-4, "about its hub: {pivot}");
        let (axis, a) = q.to_axis_angle();
        assert!(axis.distance(Vec3::X) < 1e-4 && (a - angle * 1.7).abs() < 1e-4, "{axis} {a}");
        // The rest of the body holds still.
        assert_eq!(turn("cabin"), None);
        assert_eq!(turn("fin"), None);

        // Drawn: between two rotor angles only the rotor parts move (the hub and two blades, the
        // tail rotor's hub and two blades).
        let draw = |rotor: f32| {
            let mut scene = Scene::default();
            emit_vehicle(&mut scene, &object(), &heli("view_heli_rotor", rotor), Style::Cel, false);
            scene
        };
        let (a, b) = (draw(0.0), draw(1.1));
        assert_eq!(a.meshes.len(), b.meshes.len());
        let moved = a.meshes.iter().zip(&b.meshes).filter(|(x, y)| x.transform != y.transform).count();
        assert_eq!(moved, 6, "the rotor parts, and nothing else");
        // A car's parts named alike do not spin.
        let car = VehicleView { kind: VehicleKind::Car, ..heli("view_heli_rotor", 1.1) };
        assert!(rotor_turns(&car, parts).is_empty());
        props::remove("WORKSHOP/view_heli_rotor").unwrap();
    }

    #[test]
    fn a_crewed_helicopter_blinks_its_beacon_and_flashes_its_sirens() {
        let def = install("view_heli_lamps");
        let parts = &def.compiled.parts;
        let glow = |v: &VehicleView, id: &str, time: f32| {
            let p = parts.iter().find(|p| p.id == id).unwrap();
            body_tweak(v, p, &BodyFrame::new(v, parts, time)).and_then(|t| t.emissive).unwrap_or(0.0)
        };
        let mut v = heli("view_heli_lamps", 0.0);
        assert!(glow(&v, "beacon", 2.05) > 3.0 && glow(&v, "beacon", 2.5) < 1.0, "a short blink each second");
        v.driven = false;
        assert!(glow(&v, "beacon", 2.05) < 1.0, "dark while nobody flies it");
        v.lamps.siren_on = true;
        // Four beats a second: red on the even ones, blue on the odd.
        assert!(glow(&v, "siren_l", 0.05) > 3.0 && glow(&v, "siren_r", 0.05) < 1.0);
        assert!(glow(&v, "siren_r", 0.3) > 3.0 && glow(&v, "siren_l", 0.3) < 1.0);
        props::remove("WORKSHOP/view_heli_lamps").unwrap();
    }
}
