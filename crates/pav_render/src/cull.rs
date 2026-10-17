//! CPU culling of static regions (`Scene::regions`): frustum planes from a view-projection and
//! box tests against them, plus the horizontal distance the fog measures.

use glam::{Mat4, Vec2, Vec3, Vec4};

/// The six planes of a view-projection (Gribb-Hartmann, depth 0..1), pointing inward.
pub(crate) fn planes(vp: Mat4) -> [Vec4; 6] {
    let r = |i: usize| vp.row(i);
    [r(3) + r(0), r(3) - r(0), r(3) + r(1), r(3) - r(1), r(2), r(3) - r(2)]
}

/// Whether the box lies wholly outside one of the planes (then nothing in it can be seen). Tests
/// the corner furthest along each plane's normal, so a box straddling a plane is kept.
pub(crate) fn box_outside(planes: &[Vec4; 6], min: Vec3, max: Vec3) -> bool {
    planes.iter().any(|p| {
        let n = p.truncate();
        let far = Vec3::select(n.cmpge(Vec3::ZERO), max, min);
        n.dot(far) + p.w < 0.0
    })
}

/// The horizontal (x, z) distance from `center` to the nearest point of the box.
pub(crate) fn horizontal_distance(center: Vec3, min: Vec3, max: Vec3) -> f32 {
    let c = Vec2::new(center.x, center.z);
    let lo = Vec2::new(min.x, min.z);
    let hi = Vec2::new(max.x, max.z);
    (c.clamp(lo, hi) - c).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Mat4 {
        let eye = Vec3::new(0.0, 2.0, 0.0);
        let view = glam::camera::rh::view::look_to_mat4(eye, Vec3::NEG_Z, Vec3::Y);
        let proj = glam::camera::rh::proj::directx::perspective(60f32.to_radians(), 16.0 / 9.0, 0.1, 500.0);
        proj * view
    }

    #[test]
    fn boxes_in_front_are_kept_and_boxes_behind_or_beyond_are_culled() {
        let p = planes(camera());
        let chunk = |x: f32, z: f32| (Vec3::new(x, 0.0, z), Vec3::new(x + 32.0, 20.0, z + 32.0));
        let (lo, hi) = chunk(-16.0, -48.0);
        assert!(!box_outside(&p, lo, hi), "straight ahead");
        let (lo, hi) = chunk(-16.0, -8.0);
        assert!(!box_outside(&p, lo, hi), "around the camera");
        let (lo, hi) = chunk(-16.0, 40.0);
        assert!(box_outside(&p, lo, hi), "behind");
        let (lo, hi) = chunk(400.0, -48.0);
        assert!(box_outside(&p, lo, hi), "far to the side");
        let (lo, hi) = chunk(-16.0, -600.0);
        assert!(box_outside(&p, lo, hi), "past the far plane");
        // A tall tower off to the side whose top pokes into view is kept.
        assert!(!box_outside(&p, Vec3::new(30.0, 0.0, -60.0), Vec3::new(50.0, 150.0, -40.0)));
    }

    #[test]
    fn horizontal_distance_is_zero_inside_and_ignores_height() {
        let (lo, hi) = (Vec3::new(0.0, 0.0, 0.0), Vec3::new(32.0, 100.0, 32.0));
        assert_eq!(horizontal_distance(Vec3::new(10.0, 500.0, 10.0), lo, hi), 0.0);
        assert!((horizontal_distance(Vec3::new(-3.0, 0.0, -4.0), lo, hi) - 5.0).abs() < 1e-5);
        assert!((horizontal_distance(Vec3::new(40.0, 0.0, 16.0), lo, hi) - 8.0).abs() < 1e-5);
    }
}
