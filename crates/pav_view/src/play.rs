//! A hook game's presentation in the view (`pav_core::present`): its camera, its atmosphere
//! and the things it draws from game state (markers, held items, route ribbons, tracers,
//! decals, extra lights, world text).

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use pav_core::Color;
use pav_core::present::{CameraIntent, DrawList, FarView, Glow, SurfaceStyle, ToneCurve, ViewIntent};
use pav_render::CameraData;
use pav_render::mesh::{MeshData, Vertex};
use pav_render::scene::{self as rs, MeshInstance, MeshKey, Scene, Style};
use pav_render::text::{Anchor, Text3d};

use crate::atmosphere::Atmosphere;
use crate::build::{StyleOverride, TonemapChoice, ViewSettings, style_of};

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

/// The atmosphere a frame is drawn with: a preview preset (`view.atmosphere`) in place of the
/// game's, keeping the game's look and shadow radius, or the game's own.
pub fn atmosphere_for(game: Option<&ViewIntent>, preset: Atmosphere) -> Option<Cow<'_, ViewIntent>> {
    let Some(mut v) = preset.intent() else { return game.map(Cow::Borrowed) };
    if let Some(g) = game {
        v.shadow_radius = g.shadow_radius;
        v.style = g.style;
        v.style_objects = g.style_objects;
        v.style_environment = g.style_environment;
        v.outlines = g.outlines;
        v.tonemap = g.tonemap;
        v.bloom_threshold = g.bloom_threshold;
        v.haze_height = g.haze_height;
        v.gi = g.gi;
    }
    Some(Cow::Owned(v))
}

fn style_override(s: SurfaceStyle) -> StyleOverride {
    match s {
        SurfaceStyle::PerObject => StyleOverride::PerObject,
        SurfaceStyle::Flat => StyleOverride::Flat,
        SurfaceStyle::Cel => StyleOverride::Cel,
        SurfaceStyle::Lit => StyleOverride::Lit,
    }
}

/// The view settings with the game's look laid over them (`ViewIntent::style`, outlines,
/// tonemap, bloom threshold, haze height, GI), or None when the intent sets none of it.
pub fn with_look(settings: &ViewSettings, v: &ViewIntent) -> Option<ViewSettings> {
    let looks = v.style.is_some()
        || v.style_objects.is_some()
        || v.style_environment.is_some()
        || v.outlines.is_some()
        || v.tonemap.is_some()
        || v.bloom_threshold.is_some()
        || v.haze_height.is_some()
        || v.gi.is_some()
        || v.stylize.is_some();
    if !looks {
        return None;
    }
    let mut s = settings.clone();
    if let Some(style) = v.style {
        s.style = style_override(style);
    }
    if let Some(style) = v.style_objects {
        s.style_objects = style_override(style);
    }
    if let Some(style) = v.style_environment {
        s.style_environment = style_override(style);
    }
    if let Some(on) = v.outlines {
        s.outlines = on;
    }
    if let Some(curve) = v.tonemap {
        s.tonemap = match curve {
            ToneCurve::Clamp => TonemapChoice::Clamp,
            ToneCurve::SoftKnee => TonemapChoice::SoftKnee,
            ToneCurve::Aces => TonemapChoice::Aces,
        };
    }
    if let Some(t) = v.bloom_threshold {
        s.bloom_threshold = t.max(0.0);
    }
    if let Some(h) = v.haze_height {
        s.haze_height = h.max(0.1);
    }
    if let Some(gi) = v.gi {
        s.gi = gi.max(0.0);
    }
    if let Some(st) = v.stylize {
        use pav_core::present::StylizeKind;
        s.filter.stylize = match st.kind {
            StylizeKind::Paint => crate::build::StylizeChoice::Paint,
            StylizeKind::Halftone => crate::build::StylizeChoice::Halftone,
            StylizeKind::Sketch => crate::build::StylizeChoice::Sketch,
        };
        s.filter.stylize_on = crate::build::PartChoice::All;
        s.filter.stylize_size = st.size.max(1.0);
        s.filter.stylize_mix = st.mix.clamp(0.0, 1.0);
        s.filter.stylize_color = st.color.clamp(0.0, 1.0);
    }
    Some(s)
}

/// Where the sun's shadow square goes: ahead of the camera, half its radius past the point it
/// looks at, along the view's ground direction (the renderer snaps it to shadow texels).
pub fn shadow_center(cam: &CameraData, center: Vec3, radius: f32) -> Vec3 {
    let ahead = Vec3::new(cam.forward.x, 0.0, cam.forward.z).normalize_or_zero();
    center + ahead * (radius * 0.5)
}

/// The sun disc's radiance per unit of sunlight (it must bloom).
const SUN_DISC: f32 = 14.0;

/// Lays the game's atmosphere over the scene the view settings made: sun and its shadows,
/// ambient light, the sky dome, fog, exposure, bloom, haze, light shafts, grading, wet surfaces,
/// lightning, street lights and lit windows. `center` is where the camera looks (the shadow
/// square sits ahead of it); fog is measured from the camera.
pub fn apply_view(scene: &mut Scene, v: &ViewIntent, center: Vec3) {
    scene.sun.direction = v.sun_dir.normalize_or(Vec3::NEG_Y);
    scene.sun.color = v3(v.sun_color) * v.sun_intensity.max(0.0);
    scene.sun.shadow_radius = v.shadow_radius.max(5.0);
    scene.sun.shadow_center = shadow_center(&scene.camera, center, scene.sun.shadow_radius);
    // A lightning flash lights everything for a moment, a little blue.
    let flash = v.sky.as_ref().map_or(0.0, |s| s.lightning.clamp(0.0, 1.0));
    let boost = Vec3::new(0.55, 0.6, 0.75) * (flash * 0.7);
    scene.ambient.sky = v3(v.ambient_sky) * v.ambient + boost;
    scene.ambient.ground = v3(v.ambient_ground) * v.ambient + boost * 0.6;
    if let Some(sky) = &v.sky {
        scene.clear_color = v3(sky.horizon);
        let wind = if v.wind == Vec2::ZERO { Vec2::new(3.0, 1.0) } else { v.wind * 3.0 };
        scene.sky = Some(rs::Sky {
            zenith: v3(sky.zenith),
            horizon: v3(sky.horizon),
            ground: v3(sky.ground),
            to_sun: -scene.sun.direction,
            sun_size: sky.sun_size.max(0.0),
            sun_color: v3(v.sun_color) * (v.sun_intensity.max(0.0) * SUN_DISC),
            glow: v3(sky.glow),
            to_moon: sky.moon_dir.map(|m| m.normalize_or(Vec3::Y)),
            stars: sky.stars,
            clouds: sky.clouds,
            cloud_color: v3(sky.cloud_color),
            wind,
            lightning: flash,
        });
    }
    if v.wind != Vec2::ZERO {
        // Foliage, banners and grass lean with the game's wind.
        scene.wind.direction = v.wind.normalize();
        scene.wind.strength = (v.wind.length() * 0.25).min(3.0);
    }
    scene.wetness = v.wetness.clamp(0.0, 1.0);
    scene.street_lights = v.street_lights.clamp(0.0, 1.0);
    scene.windows = v.windows.clamp(0.0, 1.0);
    match &v.fog {
        Some(f) => {
            // Measured from the camera: haze grows with distance from the viewer, whatever the
            // camera looks at (a drone shot's target is hundreds of metres away).
            let eye = scene.camera.eye;
            scene.fog = rs::Fog { enabled: true, center: eye, start: f.start, end: f.end.max(f.start + 1.0), color: v3(f.color) };
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
        let night = if p.glow == Glow::Night { rs::flags::NIGHT } else { 0 };
        let flags = rs::flags::NO_CUT | night | if p.shadow { 0 } else { rs::flags::NO_SHADOW };
        let style = style_of(p.look, ov);
        let mut glows = Vec::new();
        crate::build::emit_shape(
            &mut scene.meshes,
            &mut scene.sdfs,
            &mut glows,
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
        // A night glow lights the street only as far as the street lights are on.
        let k = if night != 0 { scene.street_lights } else { 1.0 };
        scene.point_lights.extend(glows.into_iter().filter(|_| k > 0.0).map(|l| rs::PointLight { color: l.color * k, ..l }));
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

/// Draws the far view's chunks that streaming has not loaded (`active`) and the camera sees, in
/// the environment's surface `style`.
pub fn emit_far(scene: &mut Scene, far: &FarView, cache: &mut FarCache, style: Style, active: impl Fn(i32, i32) -> bool) {
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
            style,
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

    fn street_camera() -> (CameraIntent, Scene) {
        let c = CameraIntent {
            eye: Vec3::new(0.0, 2.0, 10.0),
            target: Vec3::new(0.0, 1.0, 0.0),
            up: Vec3::Y,
            fov: 55.0,
            near: 0.1,
            far: 1500.0,
            cut: true,
            shake: 0.0,
        };
        let scene = Scene { camera: camera(None, &c, 1.0, 16.0 / 9.0, 0.0), ..Default::default() };
        (c, scene)
    }

    #[test]
    fn the_atmosphere_reaches_the_scene() {
        let (c, mut scene) = street_camera();
        let v = Atmosphere::Storm.intent().unwrap();
        apply_view(&mut scene, &v, c.target);
        let sky = scene.sky.expect("a sky");
        let s = v.sky.as_ref().unwrap();
        assert!((sky.to_sun + v.sun_dir.normalize()).length() < 1e-5, "the sun opposite the light");
        assert_eq!((sky.clouds, sky.sun_size, sky.zenith), (s.clouds, s.sun_size, v3(s.zenith)));
        assert_eq!(scene.clear_color, v3(s.horizon));
        assert_eq!((scene.wetness, scene.street_lights, scene.windows), (v.wetness, v.street_lights, v.windows));
        // The wind drifts the clouds and bends the foliage.
        assert!(sky.wind.length() > v.wind.length() && scene.wind.strength > 0.0);
        // The shadow square sits ahead of the camera, half its radius past the target.
        let ahead = c.target + Vec3::NEG_Z * (v.shadow_radius * 0.5);
        assert!((scene.sun.shadow_center - ahead).length() < 1e-3, "{}", scene.sun.shadow_center);
        assert_eq!(scene.sun.shadow_radius, v.shadow_radius);
        // Fog fades into the sky; without a sky the clear colour stays the view's own.
        assert!(scene.fog.enabled && scene.fog.center == scene.camera.eye, "fog grows from the camera");
        let (_, mut plain) = street_camera();
        let clear = plain.clear_color;
        apply_view(&mut plain, &ViewIntent::default(), c.target);
        assert!(plain.sky.is_none() && plain.clear_color == clear);
    }

    #[test]
    fn lightning_flashes_the_ambient_light() {
        let (c, mut calm) = street_camera();
        let (_, mut flash) = street_camera();
        let storm = Atmosphere::Storm.intent().unwrap();
        apply_view(&mut calm, &storm, c.target);
        apply_view(&mut flash, &Atmosphere::Lightning.intent().unwrap(), c.target);
        assert_eq!(flash.sky.unwrap().lightning, 1.0);
        assert!(flash.ambient.sky.min_element() > calm.ambient.sky.max_element());
    }

    #[test]
    fn presets_replace_the_atmosphere_and_keep_the_games_look() {
        let game =
            ViewIntent { style: Some(SurfaceStyle::Lit), outlines: Some(false), shadow_radius: 99.0, ..Default::default() };
        assert_eq!(atmosphere_for(Some(&game), Atmosphere::Scene).as_deref(), Some(&game));
        assert!(atmosphere_for(None, Atmosphere::Scene).is_none());
        let night = atmosphere_for(Some(&game), Atmosphere::Night).unwrap();
        assert_eq!((night.style, night.outlines, night.shadow_radius), (Some(SurfaceStyle::Lit), Some(false), 99.0));
        assert_eq!(night.street_lights, 1.0);
        assert!(night.sky.as_ref().unwrap().moon_dir.is_some() && night.sky.as_ref().unwrap().stars > 0.0);
        // Every preset has a sky, and fog in its horizon's colour.
        for (i, name) in <Atmosphere as pav_core::params::ChoiceParam>::NAMES.iter().enumerate().skip(1) {
            let v = <Atmosphere as pav_core::params::ChoiceParam>::from_index(i).intent().unwrap();
            let (sky, fog) = (v.sky.unwrap(), v.fog.unwrap());
            assert_eq!(fog.color, sky.horizon, "{name}");
        }
    }

    #[test]
    fn the_games_look_overrides_the_view_settings() {
        let settings = ViewSettings::default();
        assert!(with_look(&settings, &ViewIntent::default()).is_none(), "nothing to change");
        let v = ViewIntent {
            style: Some(SurfaceStyle::PerObject),
            style_environment: Some(SurfaceStyle::Lit),
            outlines: Some(false),
            tonemap: Some(ToneCurve::Aces),
            bloom_threshold: Some(1.0),
            haze_height: Some(10.0),
            gi: Some(0.5),
            ..Default::default()
        };
        let s = with_look(&settings, &v).unwrap();
        assert_eq!(
            (s.style, s.style_environment, s.style_objects),
            (StyleOverride::PerObject, StyleOverride::Lit, settings.style_objects)
        );
        assert_eq!((s.outlines, s.tonemap, s.bloom_threshold, s.haze_height, s.gi), (false, TonemapChoice::Aces, 1.0, 10.0, 0.5));
    }
}
