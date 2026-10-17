//! The player's settings (PLAN.md §5.9: audio, display, controls, accessibility): edited in the
//! settings screen, kept by the host (`AppRequests.settings`) and handed back at start
//! (`host.settings <json>`). Each page is a short list (≤ 7 items, the 10-foot rule).

use pav_core::present::{GlyphStyle, ItemValue, MenuItem};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
    pub dialogue: f32,
    pub fps: bool,
    pub hud_scale: f32,
    pub safe_area: f32,
    pub sensitivity: f32,
    pub invert_y: bool,
    /// The camera swings back behind the player's movement after 2 s without look input.
    pub recentre: bool,
    pub vibration: bool,
    pub glyphs: GlyphStyle,
    pub subtitles: bool,
    pub reduce_shake: bool,
    pub high_contrast: bool,
    pub wheel_slowmo: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            master: 0.8,
            music: 0.7,
            sfx: 0.9,
            dialogue: 1.0,
            fps: true,
            hud_scale: 1.0,
            safe_area: 0.05,
            sensitivity: 1.0,
            invert_y: false,
            recentre: true,
            vibration: true,
            glyphs: GlyphStyle::Auto,
            subtitles: true,
            reduce_shake: false,
            high_contrast: false,
            wheel_slowmo: true,
        }
    }
}

/// The settings pages, in tab order.
pub const PAGES: [&str; 4] = ["AUDIO", "DISPLAY", "CONTROLS", "ACCESSIBILITY"];

const GLYPHS: [(GlyphStyle, &str); 4] = [
    (GlyphStyle::Auto, "Automatic"),
    (GlyphStyle::PlayStation, "PlayStation"),
    (GlyphStyle::Xbox, "Xbox"),
    (GlyphStyle::Keyboard, "Keyboard"),
];

fn slider(id: &str, label: &str, v: f32, text: String) -> MenuItem {
    MenuItem {
        id: id.into(),
        label: label.into(),
        value: ItemValue::Slider { value: v.clamp(0.0, 1.0), text },
        enabled: true,
        note: None,
    }
}

fn toggle(id: &str, label: &str, on: bool) -> MenuItem {
    MenuItem { id: id.into(), label: label.into(), value: ItemValue::Toggle { on }, enabled: true, note: None }
}

fn pct(v: f32) -> String {
    format!("{}%", (v * 100.0).round())
}

impl Settings {
    /// The items on settings page `page`.
    pub fn items(&self, page: usize) -> Vec<MenuItem> {
        match page {
            0 => vec![
                slider("master", "Master volume", self.master, pct(self.master)),
                slider("music", "Radio and music", self.music, pct(self.music)),
                slider("sfx", "Sound effects", self.sfx, pct(self.sfx)),
                slider("dialogue", "Dialogue", self.dialogue, pct(self.dialogue)),
            ],
            1 => vec![
                toggle("fps", "Frame rate readout", self.fps),
                slider("hud_scale", "HUD size", (self.hud_scale - 0.8) / 0.4, pct(self.hud_scale)),
                slider("safe_area", "HUD safe area", self.safe_area / 0.1, pct(self.safe_area)),
            ],
            2 => vec![
                slider("sensitivity", "Camera sensitivity", (self.sensitivity - 0.4) / 1.6, format!("{:.1}", self.sensitivity)),
                toggle("invert_y", "Invert look", self.invert_y),
                toggle("recentre", "Camera follows movement", self.recentre),
                toggle("vibration", "Vibration", self.vibration),
                MenuItem {
                    id: "glyphs".into(),
                    label: "Button prompts".into(),
                    value: ItemValue::Choice {
                        options: GLYPHS.iter().map(|(_, n)| n.to_string()).collect(),
                        index: GLYPHS.iter().position(|(g, _)| *g == self.glyphs).unwrap_or(0),
                    },
                    enabled: true,
                    note: None,
                },
            ],
            _ => vec![
                toggle("subtitles", "Subtitles", self.subtitles),
                toggle("reduce_shake", "Reduce camera shake", self.reduce_shake),
                toggle("high_contrast", "High-contrast HUD", self.high_contrast),
                toggle("wheel_slowmo", "Slow motion in weapon wheel", self.wheel_slowmo),
            ],
        }
    }

    /// Changes setting `id` one step (`dir` -1 or 1; 0 toggles or cycles forward). Returns
    /// whether anything changed.
    pub fn adjust(&mut self, id: &str, dir: i32) -> bool {
        let before = self.clone();
        let step = |v: &mut f32, lo: f32, hi: f32, by: f32| {
            let d = if dir == 0 { by } else { by * dir as f32 };
            *v = ((*v + d) * 100.0).round() / 100.0;
            if dir == 0 && *v > hi + 1e-4 {
                *v = lo;
            }
            *v = v.clamp(lo, hi);
        };
        match id {
            "master" => step(&mut self.master, 0.0, 1.0, 0.1),
            "music" => step(&mut self.music, 0.0, 1.0, 0.1),
            "sfx" => step(&mut self.sfx, 0.0, 1.0, 0.1),
            "dialogue" => step(&mut self.dialogue, 0.0, 1.0, 0.1),
            "hud_scale" => step(&mut self.hud_scale, 0.8, 1.2, 0.05),
            "safe_area" => step(&mut self.safe_area, 0.0, 0.1, 0.01),
            "sensitivity" => step(&mut self.sensitivity, 0.4, 2.0, 0.1),
            "fps" => self.fps = !self.fps,
            "invert_y" => self.invert_y = !self.invert_y,
            "recentre" => self.recentre = !self.recentre,
            "vibration" => self.vibration = !self.vibration,
            "subtitles" => self.subtitles = !self.subtitles,
            "reduce_shake" => self.reduce_shake = !self.reduce_shake,
            "high_contrast" => self.high_contrast = !self.high_contrast,
            "wheel_slowmo" => self.wheel_slowmo = !self.wheel_slowmo,
            "glyphs" => {
                let i = GLYPHS.iter().position(|(g, _)| *g == self.glyphs).unwrap_or(0) as i32;
                let n = GLYPHS.len() as i32;
                self.glyphs = GLYPHS[(i + if dir == 0 { 1 } else { dir }).rem_euclid(n) as usize].0;
            }
            _ => {}
        }
        *self != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_stay_short_and_values_step() {
        let mut s = Settings::default();
        for p in 0..PAGES.len() {
            let n = s.items(p).len();
            assert!((1..=7).contains(&n), "page {p} has {n} items");
        }
        assert!(s.adjust("master", 1));
        assert_eq!(s.master, 0.9);
        assert!(s.adjust("master", 1));
        assert!(!s.adjust("master", 1), "clamped at the top: no change");
        assert_eq!(s.master, 1.0);
        assert!(s.adjust("fps", 0));
        assert!(!s.fps);
        assert!(s.adjust("glyphs", -1));
        assert_eq!(s.glyphs, GlyphStyle::Keyboard, "choices wrap");
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
        assert_eq!(serde_json::from_str::<Settings>("{\"fps\":false}").unwrap().master, 0.8, "missing keys default");
    }
}
