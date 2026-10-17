//! A hook game's presentation in the view (`pav_core::present`): its camera, its atmosphere
//! and the things it draws from game state (markers, held items, route ribbons, tracers,
//! decals, extra lights, world text).

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3, Vec4};
use pav_core::Color;
use pav_core::present::{CameraIntent, DrawList, FarView, ViewIntent};
use pav_render::CameraData;
use pav_render::mesh::{MeshData, Vertex};
use pav_render::scene::{self as rs, MeshInstance, MeshKey, Scene, Style};
use pav_render::text::{Anchor, Text3d};

use crate::build::{StyleOverride, style_of};

/// Draw-list outline group: one group for everything a game draws this way.
const GROUP: u32 = 0x7f2;

fn v3(c: Color) -> Vec3 {
    Vec3::from(c.0)
}

/// The game's camera between two ticks (`alpha` 0 = the previous tick, 1 = this one), plus its
/// cosmetic shake at `time`.
pub fn camera(prev: Option<&CameraIntent>, curr: &CameraIntent, alpha: f32, aspect: f32, time: f32) -> CameraData {
    let c = match prev.filter(|_| !curr.cut) {
        Some(p) => CameraIntent {
            eye: p.eye.lerp(curr.eye, alpha),
            target: p.target.lerp(curr.target, alpha),
            up: p.up.lerp(curr.up, alpha).normalize_or(Vec3::Y),
            fov: p.fov + (curr.fov - p.fov) * alpha,
            near: curr.near,
            far: curr.far,
            cut: false,
            shake: p.shake + (curr.shake - p.shake) * alpha,
        },
        None => *curr,
    };
    let mut eye = c.eye;
    let mut target = c.target;
    if c.shake > 0.0 {
        // Two incommensurate wobbles per axis: reads as a handheld shake, never repeats soon.
        let s = c.shake.min(2.0) * 0.06;
        let w = |a: f32, b: f32| (time * a).sin() * 0.6 + (time * b).sin() * 0.4;
        let off = Vec3::new(w(23.0, 37.0), w(29.0, 41.0), w(19.0, 31.0)) * s;
        eye += off;
        target += off * 0.5;
    }
    let forward = (target - eye).normalize_or(Vec3::NEG_Z);
    let up = if forward.cross(c.up).length_squared() < 1e-6 { Vec3::Z } else { c.up };
    let view = glam::camera::rh::view::look_to_mat4(eye, forward, up);
    let mut fov = c.fov.clamp(1.0, 170.0).to_radians();
    if aspect < 1.0 && aspect > 0.0 {
        fov = (2.0 * ((fov * 0.5).tan() / aspect).atan()).min(150f32.to_radians());
    }
    let near = c.near.max(0.01);
    let proj = glam::camera::rh::proj::directx::perspective(fov, aspect.max(1e-3), near, c.far.max(near + 1.0));
    CameraData { view, proj, eye, forward, ortho: false }
}

/// Lays the game's atmosphere over the scene the view settings made: sun, ambient light, sky
/// colour, fog, exposure, bloom, haze, light shafts and grading. `center` is where the camera
/// looks (shadows and fog are centred there).
pub fn apply_view(scene: &mut Scene, v: &ViewIntent, center: Vec3) {
    scene.sun.direction = v.sun_dir.normalize_or(Vec3::NEG_Y);
    scene.sun.color = v3(v.sun_color) * v.sun_intensity.max(0.0);
    scene.sun.shadow_center = center;
    scene.sun.shadow_radius = v.shadow_radius.max(5.0);
    scene.ambient.sky = v3(v.ambient_sky) * v.ambient;
    scene.ambient.ground = v3(v.ambient_ground) * v.ambient;
    if let Some(sky) = &v.sky {
        scene.clear_color = v3(sky.horizon);
    }
    match &v.fog {
        Some(f) => {
            scene.fog = rs::Fog { enabled: true, center, start: f.start, end: f.end.max(f.start + 1.0), color: v3(f.color) };
        }
        None => {
            scene.fog.enabled = false;
        }
    }
    scene.post.exposure = v.exposure;
    scene.post.bloom = v.bloom;
    scene.post.haze = v.haze;
    scene.post.shafts = v.shafts;
    scene.filter.temperature = v.temperature;
    scene.filter.tint = v.tint;
    scene.filter.contrast = v.contrast;
    scene.filter.saturation = v.saturation;
    scene.filter.vignette = v.vignette;
}

fn quad(transform: Mat4, color: Vec3, emissive: f32, style: Style, flags: u32) -> MeshInstance {
    MeshInstance { mesh: MeshKey::Cube, transform, color, emissive, style, flags, group: GROUP, material: 0, ext: Vec3::ZERO }
}

/// A thin box from `a` to `b`, `width` wide, lying flat (ribbons) or round-ish (tracers).
fn strip(a: Vec3, b: Vec3, width: f32, height: f32) -> Option<Mat4> {
    let d = b - a;
    let len = d.length();
    if len < 1e-4 {
        return None;
    }
    let dir = d / len;
    let yaw = dir.x.atan2(dir.z);
    let pitch = -dir.y.clamp(-1.0, 1.0).asin();
    let rot = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
    Some(Mat4::from_scale_rotation_translation(Vec3::new(width, height, len), rot, (a + b) * 0.5))
}

/// Draws a game's draw list into the scene. `cam` faces billboard text; `ov` is the scene's
/// style override.
pub fn emit_draw(scene: &mut Scene, draw: &DrawList, cam: &CameraData, ov: StyleOverride) -> Vec<String> {
    let mut errors = Vec::new();
    for a in &draw.assets {
        let at = crate::assets::Placement { pos: a.pos, rot: a.rot, scale: a.scale, id: a.id };
        if let Err(e) = crate::assets::emit_asset(scene, &a.asset, at, a.seed, &Default::default()) {
            errors.push(e);
        }
    }
    for p in &draw.prims {
        let flags = rs::flags::NO_CUT | if p.shadow { 0 } else { rs::flags::NO_SHADOW };
        let style = style_of(p.look, ov);
        crate::build::emit_shape(
            &mut scene.meshes,
            &mut scene.sdfs,
            &mut scene.point_lights,
            &p.shape,
            p.pos,
            p.rot,
            v3(p.color),
            p.emissive,
            style,
            GROUP,
            flags,
            0,
        );
    }
    for l in &draw.lights {
        scene.point_lights.push(rs::PointLight {
            position: l.pos,
            color: v3(l.color) * l.intensity,
            radius: l.radius,
            shadows: l.shadows,
        });
    }
    for s in &draw.spots {
        // Until the renderer has fake cones (E7): light where the beam lands.
        let dir = s.dir.normalize_or(Vec3::NEG_Y);
        let reach = s.range * 0.6;
        scene.point_lights.push(rs::PointLight {
            position: s.pos + dir * reach,
            color: v3(s.color) * s.intensity,
            radius: (reach * s.angle.tan()).max(2.0) * 2.0,
            shadows: false,
        });
    }
    let flat = rs::flags::NO_SHADOW | rs::flags::NO_CUT;
    for d in &draw.decals {
        let n = d.normal.normalize_or(Vec3::Y);
        let rot = Quat::from_rotation_arc(Vec3::Y, n) * Quat::from_rotation_y(d.yaw);
        let color = v3(d.color) * d.alpha.clamp(0.0, 1.0);
        let t = Mat4::from_scale_rotation_translation(Vec3::new(d.size.x, 0.01, d.size.y), rot, d.pos + n * 0.012);
        scene.meshes.push(quad(t, color, 0.0, style_of(pav_core::Look::Flat, ov), flat));
    }
    for r in &draw.ribbons {
        let style = if r.emissive > 0.0 { Style::Unlit } else { style_of(pav_core::Look::Flat, ov) };
        for w in r.points.windows(2) {
            if let Some(t) = strip(w[0], w[1], r.width, 0.02) {
                scene.meshes.push(quad(t, v3(r.color), r.emissive, style, flat));
            }
        }
    }
    for t in &draw.tracers {
        if let Some(m) = strip(t.from, t.to, t.width, t.width) {
            scene.meshes.push(quad(m, v3(t.color), 1.0, Style::Unlit, flat));
        }
    }
    for t in &draw.texts {
        let (right, up) = if t.billboard {
            let right = cam.forward.cross(Vec3::Y).normalize_or(Vec3::X);
            (right, right.cross(cam.forward).normalize_or(Vec3::Y))
        } else {
            (t.rot * Vec3::X, t.rot * Vec3::Y)
        };
        scene.texts.push(Text3d {
            text: t.text.clone(),
            origin: t.pos,
            right,
            up,
            size: t.size,
            color: v3(t.color) * (1.0 + t.emissive),
            anchor: Anchor::Center,
            flags: rs::flags::NO_SHADOW | rs::flags::NO_CUT,
            weight: 0.0,
        });
    }
    errors
}

/// Far-view meshes already converted for the renderer: chunk → (far view version, key, data).
#[derive(Default)]
pub struct FarCache {
    meshes: HashMap<(i32, i32), (u64, MeshKey, Arc<MeshData>)>,
}

/// The six planes of a view-projection (Gribb–Hartmann; depth 0..1).
fn frustum(vp: Mat4) -> [Vec4; 6] {
    let r = |i: usize| vp.row(i);
    [r(3) + r(0), r(3) - r(0), r(3) + r(1), r(3) - r(1), r(2), r(3) - r(2)]
}

fn visible(planes: &[Vec4; 6], min: Vec3, max: Vec3) -> bool {
    let c = (min + max) * 0.5;
    let radius = (max - min).length() * 0.5;
    planes.iter().all(|p| p.truncate().dot(c) + p.w >= -radius * p.truncate().length())
}

/// Draws the far view's chunks that streaming has not loaded (`active`) and the camera sees.
pub fn emit_far(scene: &mut Scene, far: &FarView, cache: &mut FarCache, active: impl Fn(i32, i32) -> bool) {
    let planes = frustum(scene.camera.proj * scene.camera.view);
    for ((x, z), mesh) in &far.chunks {
        if active(*x, *z) || mesh.indices.is_empty() || !visible(&planes, mesh.min, mesh.max) {
            continue;
        }
        let entry = cache.meshes.entry((*x, *z)).or_insert((u64::MAX, MeshKey::Cube, Arc::new(MeshData::default())));
        if entry.0 != far.version {
            // Packed, not hashed: no two chunks (or versions) can share a key.
            let h = 0xfa70_0000_0000_0000u64
                | (((*x + 0x8000) as u64 & 0xffff) << 32)
                | (((*z + 0x8000) as u64 & 0xffff) << 16)
                | (far.version & 0xffff);
            let data = MeshData {
                vertices: mesh
                    .positions
                    .iter()
                    .zip(&mesh.normals)
                    .zip(&mesh.colors)
                    .map(|((p, n), c)| Vertex { pos: *p, normal: *n, uv: [0.0, 0.0], color: [c[0], c[1], c[2], 1.0] })
                    .collect(),
                indices: mesh.indices.clone(),
            };
            *entry = (far.version, MeshKey::Custom(h), Arc::new(data));
        }
        scene.custom_meshes.push((entry.1, entry.2.clone()));
        scene.meshes.push(MeshInstance {
            mesh: entry.1,
            transform: Mat4::IDENTITY,
            color: Vec3::ONE,
            emissive: 0.0,
            style: Style::Cel,
            flags: rs::flags::NO_SHADOW | rs::flags::NO_CUT,
            group: 1,
            material: 0,
            ext: Vec3::ZERO,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frustum_keeps_what_the_camera_sees() {
        let c = CameraIntent {
            eye: Vec3::new(0.0, 100.0, 200.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov: 45.0,
            near: 0.5,
            far: 2500.0,
            cut: true,
            shake: 0.0,
        };
        let cam = camera(None, &c, 1.0, 16.0 / 9.0, 0.0);
        let planes = frustum(cam.proj * cam.view);
        let b = |x: f32, z: f32| visible(&planes, Vec3::new(x - 16.0, 0.0, z - 16.0), Vec3::new(x + 16.0, 0.2, z + 16.0));
        assert!(b(0.0, 0.0), "the target");
        assert!(b(0.0, -400.0), "far ahead");
        assert!(b(150.0, -300.0), "ahead to the right");
        assert!(!b(0.0, 600.0), "behind the camera");
        assert!(!b(2000.0, 0.0), "far to the side");
    }
}
