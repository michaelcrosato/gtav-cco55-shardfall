//! What the executable calls itself and how it starts. The engine's own app is Shardfall; a game
//! built on the engine runs this app from its own binary with its own [`AppConfig`]
//! (`pav_app::main_with`).

use std::sync::OnceLock;

/// What phones and tablets get.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchMode {
    /// Touch controls: an invisible stick, a button fan, auto-attack (the demo and Shardfall).
    Controls,
    /// Phones and tablets only watch the game's attract loop: no touch controls.
    AttractOnly,
}

#[derive(Clone, Debug)]
pub struct AppConfig {
    /// Window title, error boxes and the log's first line.
    pub title: &'static str,
    /// File stem for the settings (`<stem>.toml`), the log, saves, looks and screenshots, and for
    /// the browser's storage keys.
    pub stem: &'static str,
    /// The scene to start in when neither the settings nor the command line name one.
    pub default_scene: &'static str,
    pub touch: TouchMode,
    /// The engine's developer keys and menus (tuning panel, rooms, rewind, diagnostics).
    pub dev_keys: bool,
    /// The game's sound cues (a `pav_audio::Bank` file) over the engine's.
    pub sound_bank: Option<&'static str>,
}

impl AppConfig {
    /// The engine's own app: Shardfall, starting in its town.
    pub fn shardfall() -> Self {
        Self {
            title: "Shardfall",
            stem: "shardfall",
            default_scene: "town",
            touch: TouchMode::Controls,
            dev_keys: true,
            sound_bank: None,
        }
    }

    /// A file next to the executable named for this app: `file("toml")` is `<stem>.toml`,
    /// `file("_save.json")` is `<stem>_save.json`.
    pub fn file(&self, suffix: &str) -> String {
        if suffix.starts_with('_') || suffix.starts_with('-') {
            format!("{}{suffix}", self.stem)
        } else {
            format!("{}.{suffix}", self.stem)
        }
    }
}

static CONFIG: OnceLock<AppConfig> = OnceLock::new();

/// Sets the app's configuration: once, before anything else runs.
pub fn set(config: AppConfig) {
    let _ = CONFIG.set(config);
}

/// The app's configuration (Shardfall's when none was set).
pub fn get() -> &'static AppConfig {
    CONFIG.get_or_init(AppConfig::shardfall)
}
