//! The player's settings (PLAN.md §5.9: audio, display, the HUD, controls, accessibility):
//! edited in the settings screen, kept by the host (`AppRequests.settings`) and handed back at
//! start (`host.settings <json>`); the display's are the host's to apply (`AppRequests.display`).
//! Each page is a short list (≤ 7 items, the 10-foot rule).

use pav_core::present::{GlyphStyle, ItemValue, MenuItem};
use serde::{Deserialize, Serialize};

/// The graphics quality presets (the host's renderer settings, and how many people and cars).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
}

impl Quality {
    pub const ALL: [Quality; 4] = [Quality::Low, Quality::Medium, Quality::High, Quality::Ultra];

    pub fn name(self) -> &'static str {
        match self {
            Quality::Low => "Low",
            Quality::Medium => "Medium",
            Quality::High => "High",
            Quality::Ultra => "Ultra",
        }
    }

    /// The share of the population the streets get (people and traffic).
    pub fn population(self) -> f32 {
        match self {
            Quality::Low => 0.55,
            Quality::Medium => 0.8,
            Quality::High | Quality::Ultra => 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Audio
    pub master: f32,
    pub music: f32,
    pub sfx: f32,
    pub dialogue: f32,
    /// The station a car's radio comes on with (none: the car's own).
    pub station: Option<String>,
    // Display (the host applies these)
    pub fullscreen: bool,
    pub quality: Quality,
    pub render_scale: f32,
    pub dynamic_res: bool,
    pub vsync: bool,
    pub brightness: f32,
    // HUD
    pub fps: bool,
    pub hud_scale: f32,
    pub safe_area: f32,
    // Controls
    pub aim_assist: f32,
    pub sensitivity: f32,
    pub sensitivity_y: f32,
    pub invert_y: bool,
    pub sprint_toggle: bool,
    pub vibration: bool,
    pub glyphs: GlyphStyle,
    // Accessibility
    pub subtitles: bool,
    pub subtitle_size: f32,
    pub colorblind: bool,
    pub reduce_shake: bool,
    pub high_contrast: bool,
    pub wheel_slowmo: bool,
    /// The camera swings back behind the player's movement after 2 s without look input.
    pub recentre: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            master: 0.8,
            music: 0.7,
            sfx: 0.9,
            dialogue: 1.0,
            station: None,
            fullscreen: false,
            quality: Quality::High,
            render_scale: 1.0,
            dynamic_res: true,
            vsync: true,
            brightness: 1.0,
            fps: true,
            hud_scale: 1.0,
            safe_area: 0.05,
            aim_assist: 1.0,
            sensitivity: 1.0,
            sensitivity_y: 1.0,
            invert_y: false,
            sprint_toggle: false,
            vibration: true,
            glyphs: GlyphStyle::Auto,
            subtitles: true,
            subtitle_size: 1.0,
            colorblind: false,
            reduce_shake: false,
            high_contrast: false,
            wheel_slowmo: true,
            recentre: true,
        }
    }
}

/// The settings pages, in tab order.
pub const PAGES: [&str; 5] = ["AUDIO", "DISPLAY", "HUD", "CONTROLS", "ACCESSIBILITY"];

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

fn choice(id: &str, label: &str, options: Vec<String>, index: usize) -> MenuItem {
    MenuItem { id: id.into(), label: label.into(), value: ItemValue::Choice { options, index }, enabled: true, note: None }
}

fn pct(v: f32) -> String {
    format!("{}%", (v * 100.0).round())
}

/// The radio's stations (id, name), for the default station's choices.
fn stations() -> Vec<(String, String)> {
    crate::radio::def().map(|d| d.station.iter().map(|s| (s.id.clone(), s.name.clone())).collect()).unwrap_or_default()
}

impl Settings {
    /// The items on settings page `page` (`web`: the browser's, where the page owns full screen
    /// and v-sync).
    pub fn items(&self, page: usize, web: bool) -> Vec<MenuItem> {
        let stations = stations();
        match page {
            0 => {
                let mut names = vec!["The car's own".to_string()];
                names.extend(stations.iter().map(|s| s.1.clone()));
                let at = self.station.as_ref().and_then(|s| stations.iter().position(|x| &x.0 == s)).map_or(0, |i| i + 1);
                vec![
                    slider("master", "Master volume", self.master, pct(self.master)),
                    slider("music", "Radio and music", self.music, pct(self.music)),
                    slider("sfx", "Sound effects", self.sfx, pct(self.sfx)),
                    slider("dialogue", "Dialogue", self.dialogue, pct(self.dialogue)),
                    choice("station", "Radio station", names, at),
                ]
            }
            1 => {
                let mut v = vec![
                    choice(
                        "quality",
                        "Graphics quality",
                        Quality::ALL.iter().map(|q| q.name().to_string()).collect(),
                        self.quality as usize,
                    ),
                    slider("render_scale", "Render scale", (self.render_scale - 0.5) / 0.5, pct(self.render_scale)),
                    toggle("dynamic_res", "Dynamic resolution", self.dynamic_res),
                    slider("brightness", "Brightness", (self.brightness - 0.7) / 0.6, pct(self.brightness)),
                ];
                if !web {
                    v.insert(0, toggle("fullscreen", "Full screen", self.fullscreen));
                    v.push(toggle("vsync", "V-sync", self.vsync));
                }
                v
            }
            2 => vec![
                toggle("fps", "Frame rate readout", self.fps),
                slider("hud_scale", "HUD size", (self.hud_scale - 0.8) / 0.4, pct(self.hud_scale)),
                slider("safe_area", "HUD safe area", self.safe_area / 0.1, pct(self.safe_area)),
            ],
            3 => vec![
                slider(
                    "aim_assist",
                    "Aim assist",
                    self.aim_assist,
                    if self.aim_assist <= 0.0 { "Off".into() } else { pct(self.aim_assist) },
                ),
                slider("sensitivity", "Camera speed X", (self.sensitivity - 0.4) / 1.6, format!("{:.1}", self.sensitivity)),
                slider("sensitivity_y", "Camera speed Y", (self.sensitivity_y - 0.4) / 1.6, format!("{:.1}", self.sensitivity_y)),
                toggle("invert_y", "Invert look", self.invert_y),
                choice("sprint_toggle", "Sprint", vec!["Hold".into(), "Toggle".into()], self.sprint_toggle as usize),
                toggle("vibration", "Vibration", self.vibration),
                choice(
                    "glyphs",
                    "Button prompts",
                    GLYPHS.iter().map(|(_, n)| n.to_string()).collect(),
                    GLYPHS.iter().position(|(g, _)| *g == self.glyphs).unwrap_or(0),
                ),
            ],
            _ => vec![
                toggle("subtitles", "Subtitles", self.subtitles),
                slider("subtitle_size", "Subtitle size", (self.subtitle_size - 0.8) / 0.6, pct(self.subtitle_size)),
                toggle("colorblind", "Colourblind blips", self.colorblind),
                toggle("reduce_shake", "Reduce camera shake", self.reduce_shake),
                toggle("high_contrast", "High-contrast HUD", self.high_contrast),
                toggle("wheel_slowmo", "Wheel slow motion", self.wheel_slowmo),
                toggle("recentre", "Camera recentres", self.recentre),
            ],
        }
    }

    pub fn quality_name(&self) -> &'static str {
        self.quality.name()
    }

    /// Changes setting `id` one step (`dir` -1 or 1; 0 toggles or cycles forward). Returns
    /// whether anything changed.
    pub fn adjust(&mut self, id: &str, dir: i32) -> bool {
        let before = self.clone();
        let stations = stations();
        let step = |v: &mut f32, lo: f32, hi: f32, by: f32| {
            let d = if dir == 0 { by } else { by * dir as f32 };
            *v = ((*v + d) * 100.0).round() / 100.0;
            if dir == 0 && *v > hi + 1e-4 {
                *v = lo;
            }
            *v = v.clamp(lo, hi);
        };
        let cycle = |i: usize, n: usize| (i as i32 + if dir == 0 { 1 } else { dir }).rem_euclid(n as i32) as usize;
        match id {
            "master" => step(&mut self.master, 0.0, 1.0, 0.1),
            "music" => step(&mut self.music, 0.0, 1.0, 0.1),
            "sfx" => step(&mut self.sfx, 0.0, 1.0, 0.1),
            "dialogue" => step(&mut self.dialogue, 0.0, 1.0, 0.1),
            "station" => {
                let at = self.station.as_ref().and_then(|s| stations.iter().position(|x| &x.0 == s)).map_or(0, |i| i + 1);
                let next = cycle(at, stations.len() + 1);
                self.station = (next > 0).then(|| stations[next - 1].0.clone());
            }
            "quality" => self.quality = Quality::ALL[cycle(self.quality as usize, Quality::ALL.len())],
            "render_scale" => step(&mut self.render_scale, 0.5, 1.0, 0.05),
            "brightness" => step(&mut self.brightness, 0.7, 1.3, 0.05),
            "hud_scale" => step(&mut self.hud_scale, 0.8, 1.2, 0.05),
            "safe_area" => step(&mut self.safe_area, 0.0, 0.1, 0.01),
            "aim_assist" => step(&mut self.aim_assist, 0.0, 1.0, 0.1),
            "sensitivity" => step(&mut self.sensitivity, 0.4, 2.0, 0.1),
            "sensitivity_y" => step(&mut self.sensitivity_y, 0.4, 2.0, 0.1),
            "subtitle_size" => step(&mut self.subtitle_size, 0.8, 1.4, 0.1),
            "fullscreen" => self.fullscreen = !self.fullscreen,
            "dynamic_res" => self.dynamic_res = !self.dynamic_res,
            "vsync" => self.vsync = !self.vsync,
            "fps" => self.fps = !self.fps,
            "invert_y" => self.invert_y = !self.invert_y,
            "sprint_toggle" => self.sprint_toggle = !self.sprint_toggle,
            "recentre" => self.recentre = !self.recentre,
            "vibration" => self.vibration = !self.vibration,
            "subtitles" => self.subtitles = !self.subtitles,
            "colorblind" => self.colorblind = !self.colorblind,
            "reduce_shake" => self.reduce_shake = !self.reduce_shake,
            "high_contrast" => self.high_contrast = !self.high_contrast,
            "wheel_slowmo" => self.wheel_slowmo = !self.wheel_slowmo,
            "glyphs" => {
                let i = GLYPHS.iter().position(|(g, _)| *g == self.glyphs).unwrap_or(0);
                self.glyphs = GLYPHS[cycle(i, GLYPHS.len())].0;
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
            for web in [false, true] {
                let n = s.items(p, web).len();
                assert!((1..=7).contains(&n), "page {p} has {n} items");
            }
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
        assert!(s.adjust("quality", 1));
        assert_eq!(s.quality, Quality::Ultra);
        assert!(s.adjust("quality", 1));
        assert_eq!(s.quality, Quality::Low, "the presets wrap");
        // The default station: the car's own, then each station in turn, then the car's own again.
        let n = crate::radio::def().unwrap().station.len();
        for _ in 0..n {
            assert!(s.adjust("station", 1));
            assert!(s.station.is_some());
        }
        assert!(s.adjust("station", 1));
        assert_eq!(s.station, None);
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
        assert_eq!(serde_json::from_str::<Settings>("{\"fps\":false}").unwrap().master, 0.8, "missing keys default");
    }

    #[test]
    fn the_browser_leaves_full_screen_and_vsync_to_the_page() {
        let s = Settings::default();
        let ids = |web| s.items(1, web).into_iter().map(|i| i.id).collect::<Vec<_>>();
        assert!(ids(false).contains(&"fullscreen".to_string()) && ids(false).contains(&"vsync".to_string()));
        assert!(!ids(true).contains(&"fullscreen".to_string()) && !ids(true).contains(&"vsync".to_string()));
    }
}
