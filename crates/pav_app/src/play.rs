//! The shell for hook games that present themselves (`PlayFrame`, PLAN.md §6.5 E13): the game
//! owns the camera, the UI, menus and Esc; the host maps every device onto the controller
//! (the game's key map), holds the pointer for mouse look, sends the game what it can't know
//! itself as recorded commands, and keeps the saves and settings it asks for.
//!
//! Commands the host sends (one tick each, in `InputFrame.cmds`):
//! - `host.platform <native|web>[ touch]` once, before anything else;
//! - `host.settings <json>` and `host.save <slot> <json>` for what was kept from last time;
//! - `ui.focus <id>` / `ui.accept <id>` for the element under the pointer and clicks on it;
//! - `ui.gear` for the touch screens' settings button.

use std::sync::Arc;

use pav_core::present::{AppRequests, KeyMap, PadTarget};

use crate::input::{KeyOrMouse, resolve_keys};

pub struct PlayShell {
    /// The key map in use (by `Arc` address) and its resolved keys.
    keys: Option<(usize, Vec<(KeyOrMouse, PadTarget)>)>,
    standard: Vec<(KeyOrMouse, PadTarget)>,
    /// Commands for the next tick.
    pending: Vec<String>,
    /// Requests already carried out.
    save_serial: u64,
    settings_serial: u64,
    rumble_serial: u64,
    started: bool,
    /// The pointer is held for mouse look.
    pub grabbed: bool,
    /// Smoothed frame rate and frame time for the readout.
    pub fps: f32,
    pub frame_ms: f32,
    /// The master volume last set from the game's audio scene.
    pub master: f32,
    /// The element the pointer was over last frame.
    hovered: Option<String>,
    /// The 10-foot UI kit drawing the game's `UiModel`.
    pub ui: pav_ui::Ui,
}

impl Default for PlayShell {
    fn default() -> Self {
        Self {
            keys: None,
            standard: resolve_keys(&KeyMap::standard()).0,
            pending: Vec::new(),
            save_serial: 0,
            settings_serial: 0,
            rumble_serial: 0,
            started: false,
            grabbed: false,
            fps: 60.0,
            frame_ms: 16.7,
            master: -1.0,
            hovered: None,
            ui: pav_ui::Ui::new(),
        }
    }
}

impl PlayShell {
    /// The keys the game's current key map binds (the standard map when it names none).
    pub fn keys(&mut self, map: Option<&Arc<KeyMap>>) -> &[(KeyOrMouse, PadTarget)] {
        let Some(map) = map else { return &self.standard };
        let addr = Arc::as_ptr(map) as usize;
        if self.keys.as_ref().is_none_or(|(a, _)| *a != addr) {
            let (keys, unknown) = resolve_keys(map);
            for k in unknown {
                log::warn!("key map '{}': unknown key '{k}'", map.name);
            }
            self.keys = Some((addr, keys));
        }
        &self.keys.as_ref().unwrap().1
    }

    /// The first time a game presents itself: tell it the platform and hand it what was kept.
    pub fn start(&mut self, touch: bool) {
        if self.started {
            return;
        }
        self.started = true;
        let platform = if cfg!(target_arch = "wasm32") { "web" } else { "native" };
        self.pending.push(format!("host.platform {platform}{}", if touch { " touch" } else { "" }));
        if let Some(settings) = store::read("_settings") {
            self.pending.push(format!("host.settings {settings}"));
        }
        for (slot, json) in store::saves() {
            self.pending.push(format!("host.save {slot} {json}"));
        }
    }

    /// The pointer over the game's UI: focus what it hovers (when that changes), accept what it
    /// clicks, open the settings from the touch gear.
    pub fn pointer(&mut self, r: &pav_ui::UiResponse) {
        if r.hovered != self.hovered {
            if let Some(id) = &r.hovered {
                self.pending.push(format!("ui.focus {id}"));
            }
            self.hovered = r.hovered.clone();
        }
        if let Some(id) = &r.clicked {
            self.pending.push(format!("ui.accept {id}"));
        }
        if r.gear {
            self.pending.push("ui.gear".into());
        }
    }

    pub fn take_cmds(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending)
    }

    /// A new rumble the game asks for: (low, high, seconds).
    pub fn rumble(&mut self, app: &AppRequests) -> Option<(f32, f32, f32)> {
        let r = app.vibrate.filter(|r| r.serial != self.rumble_serial)?;
        self.rumble_serial = r.serial;
        Some((r.low, r.high, r.secs))
    }

    /// Keeps new saves and settings. Returns true when the game asks to quit.
    pub fn requests(&mut self, app: &AppRequests) -> bool {
        if let Some(s) = app.save.as_ref().filter(|s| s.serial != self.save_serial) {
            self.save_serial = s.serial;
            let slot: String = s.slot.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
            if slot.is_empty() {
                log::warn!("save slot '{}' has no usable characters", s.slot);
            } else {
                store::write(&format!("_save_{slot}"), &s.json);
            }
        }
        if let Some(s) = app.settings.as_ref().filter(|s| s.serial != self.settings_serial) {
            self.settings_serial = s.serial;
            store::write("_settings", &s.json);
        }
        app.quit
    }

    /// Smooths the frame time (seconds) for the readout.
    pub fn frame_time(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let k = 0.05;
        self.frame_ms += (dt * 1000.0 - self.frame_ms) * k;
        self.fps = 1000.0 / self.frame_ms.max(0.1);
    }
}

/// Small text files named for the app (`<stem><suffix>.json` next to the executable, or the
/// browser's local storage).
mod store {
    fn name(suffix: &str) -> String {
        crate::config::get().file(suffix)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn read(suffix: &str) -> Option<String> {
        std::fs::read_to_string(crate::boot::exe_dir().join(name(suffix) + ".json")).ok()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn write(suffix: &str, text: &str) {
        let p = crate::boot::exe_dir().join(name(suffix) + ".json");
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &p)).is_err() {
            log::warn!("could not write {}", p.display());
        }
    }

    /// Every kept save: (slot, json), by slot.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn saves() -> Vec<(String, String)> {
        let prefix = name("_save_");
        let mut out: Vec<(String, String)> = std::fs::read_dir(crate::boot::exe_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let file = e.file_name().into_string().ok()?;
                let slot = file.strip_prefix(&prefix)?.strip_suffix(".json")?.to_string();
                Some((slot, std::fs::read_to_string(e.path()).ok()?))
            })
            .collect();
        out.sort();
        out
    }

    #[cfg(target_arch = "wasm32")]
    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    #[cfg(target_arch = "wasm32")]
    pub fn read(suffix: &str) -> Option<String> {
        storage()?.get_item(&name(suffix)).ok()?
    }

    #[cfg(target_arch = "wasm32")]
    pub fn write(suffix: &str, text: &str) {
        if storage().and_then(|s| s.set_item(&name(suffix), text).ok()).is_none() {
            log::warn!("could not save to local storage");
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn saves() -> Vec<(String, String)> {
        let Some(s) = storage() else { return Vec::new() };
        let prefix = name("_save_");
        let n = s.length().unwrap_or(0);
        let mut out: Vec<(String, String)> = (0..n)
            .filter_map(|i| {
                let key = s.key(i).ok()??;
                let slot = key.strip_prefix(&prefix)?.to_string();
                Some((slot, s.get_item(&key).ok()??))
            })
            .collect();
        out.sort();
        out
    }
}
