//! Graphics quality: how much of the screen's resolution the scene is drawn at and which costly
//! effects run. Auto is High on a desktop (everything, as the game always drew), Medium on a
//! phone or tablet and Low on a software renderer (no GPU), where the resolution also follows the
//! frame rate (dynamic resolution) and materials draw their first layer only (plan D7).

use pav_view::materials::MaterialDetail;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    #[default]
    Auto,
    Low,
    Medium,
    High,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Auto, Quality::Low, Quality::Medium, Quality::High];

    pub fn name(self) -> &'static str {
        match self {
            Quality::Auto => "Auto",
            Quality::Low => "Low",
            Quality::Medium => "Medium",
            Quality::High => "High",
        }
    }

    /// What Auto means on this device (`software`: the adapter is a CPU renderer).
    fn resolve(self, touch: bool, software: bool) -> Quality {
        match self {
            Quality::Auto if software => Quality::Low,
            Quality::Auto if touch => Quality::Medium,
            Quality::Auto => Quality::High,
            q => q,
        }
    }

    /// Turns off what this quality cannot afford (on top of the scene's and the look's own
    /// settings).
    pub fn trim(self, touch: bool, software: bool, v: &mut pav_view::ViewSettings) {
        match self.resolve(touch, software) {
            Quality::High | Quality::Auto => {}
            Quality::Medium => {
                v.gi = 0.0;
                v.shafts = 0.0;
                v.material_detail = v.material_detail.min(MaterialDetail::FirstLayer);
            }
            Quality::Low => {
                v.material_detail = v.material_detail.min(MaterialDetail::FirstLayer);
                v.gi = 0.0;
                v.shafts = 0.0;
                v.haze = 0.0;
                v.halos = 0.0;
                v.bloom = 0.0;
                v.distortion = false;
            }
        }
    }
}

/// The render scale, following the frame rate when the quality is Auto on a touch screen.
pub struct Scaler {
    /// The dynamic part (0.6..1) and the frames measured since it last changed.
    dynamic: f32,
    ms: f32,
    frames: u32,
    /// Seconds of smooth frames in a row (raising the resolution waits for a while of them).
    calm: f32,
    pub scale: f32,
    /// A game's display settings: the render scale it multiplies by, and dynamic resolution on
    /// any device (`AppRequests::display`).
    pub user_scale: f32,
    pub user_dynamic: bool,
}

impl Default for Scaler {
    fn default() -> Self {
        Self { dynamic: 1.0, ms: 0.0, frames: 0, calm: 0.0, scale: 1.0, user_scale: 1.0, user_dynamic: false }
    }
}

impl Scaler {
    /// Called once per drawn frame with how long the frame took (ms), the target's size in
    /// pixels and the device pixel ratio. Returns (render scale, point lights with shadows).
    pub fn update(&mut self, q: Quality, touch: bool, software: bool, frame_ms: f32, size: (u32, u32), dpr: f32) -> (f32, usize) {
        let resolved = q.resolve(touch, software);
        let pixels = (size.0 as f32 * size.1 as f32).max(1.0);
        // At most this many megapixels (High: on a touch screen, two device pixels to a point
        // at most; a desktop draws every pixel, as it always has; a software renderer a quarter).
        let (base, lights) = match resolved {
            Quality::Low if software && q == Quality::Auto => ((0.25e6 / pixels).sqrt(), 0),
            Quality::Low => ((0.5e6 / pixels).sqrt(), 0),
            Quality::Medium => ((1.0e6 / pixels).sqrt(), 1),
            _ if touch => (2.0 / dpr.max(1.0), pav_render::shadows::MAX_SHADOW_LIGHTS),
            _ => (1.0, pav_render::shadows::MAX_SHADOW_LIGHTS),
        };
        if (q == Quality::Auto && (touch || software)) || self.user_dynamic {
            // A second at a time: slower than 50 fps drops a step, a few smooth seconds raise one.
            self.ms += frame_ms;
            self.frames += 1;
            if self.ms >= 1000.0 {
                let avg = self.ms / self.frames as f32;
                if avg > 20.0 {
                    self.dynamic = (self.dynamic * 0.88).max(0.6);
                    self.calm = 0.0;
                } else if avg < 17.6 {
                    self.calm += self.ms / 1000.0;
                    if self.calm >= 4.0 {
                        self.dynamic = (self.dynamic * 1.06).min(1.0);
                        self.calm = 0.0;
                    }
                } else {
                    self.calm = 0.0;
                }
                self.ms = 0.0;
                self.frames = 0;
            }
        } else {
            self.dynamic = 1.0;
        }
        self.scale = (base.min(1.0) * self.user_scale.clamp(0.25, 1.0) * self.dynamic).clamp(0.25, 1.0);
        (self.scale, lights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktops_draw_everything_and_phones_draw_less() {
        let mut s = Scaler::default();
        // A 1600x900 desktop window: full resolution, every shadow (at any display scaling).
        assert_eq!(s.update(Quality::Auto, false, false, 16.0, (1600, 900), 1.0), (1.0, pav_render::shadows::MAX_SHADOW_LIGHTS));
        assert_eq!(s.update(Quality::Auto, false, false, 16.0, (3840, 2160), 3.0).0, 1.0);
        // A software renderer: a quarter of a megapixel, no shadowed lamps.
        let (k, lights) = Scaler::default().update(Quality::Auto, false, true, 16.0, (960, 540), 1.0);
        assert!((0.65..0.72).contains(&k) && lights == 0, "{k} {lights}");
        // A phone held sideways (2532x1170 device pixels): about a megapixel, one shadow.
        let (k, lights) = s.update(Quality::Auto, true, false, 16.0, (2532, 1170), 3.0);
        assert!((0.55..0.6).contains(&k), "{k}");
        assert_eq!(lights, 1);
        // Slow frames lower it, a while of smooth ones raise it back.
        for _ in 0..40 {
            s.update(Quality::Auto, true, false, 30.0, (2532, 1170), 3.0);
        }
        let low = s.scale;
        assert!(low < k * 0.9, "{low}");
        for _ in 0..60 * 30 {
            s.update(Quality::Auto, true, false, 16.0, (2532, 1170), 3.0);
        }
        assert!(s.scale > low, "{} {low}", s.scale);
        let mut v = pav_view::ViewSettings { gi: 0.5, ..Default::default() };
        Quality::Auto.trim(false, false, &mut v);
        assert_eq!((v.gi, v.material_detail), (0.5, MaterialDetail::Full));
        Quality::Auto.trim(true, false, &mut v);
        assert_eq!((v.gi, v.material_detail), (0.0, MaterialDetail::FirstLayer));
        let mut v = pav_view::ViewSettings { bloom: 0.5, ..Default::default() };
        Quality::Auto.trim(false, true, &mut v);
        assert_eq!(v.bloom, 0.0, "a software renderer draws Low");
    }
}
