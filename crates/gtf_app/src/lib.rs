//! Grand Theft Freeway-BX's binaries: the game (`grand_theft_freeway`, the engine's app run with
//! the game's name and defaults) and its agent CLI (`gtf`, every engine tool plus the game's).
//! The game itself is `gtf_game`.

pub mod tools;

/// How the game's executable names itself and starts (`pav_app::main_with`).
pub fn app_config() -> pav_app::AppConfig {
    pav_app::AppConfig {
        title: "Grand Theft Freeway-BX",
        stem: "grand_theft_freeway",
        default_scene: gtf_game::KEY,
        touch: pav_app::TouchMode::AttractOnly,
        dev_keys: cfg!(debug_assertions),
        sound_bank: Some(SOUNDS),
    }
}

/// The game's sound cues (over the engine's default bank).
pub const SOUNDS: &str = include_str!("../../../content/freeway/sounds.toml");

/// The game's agent CLI: `gtf help`.
pub const CLI: pav_tools::cli::Cli =
    pav_tools::cli::Cli { name: "gtf", title: "Grand Theft Freeway-BX agent CLI", default_scene: "freeway/roam" };
