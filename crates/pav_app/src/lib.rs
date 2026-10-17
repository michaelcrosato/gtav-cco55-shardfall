//! The engine's app shell: window, boot diagnostics, input, the simulation thread, the live
//! bridge, egui UI and touch, on Linux, Windows and in the browser. It is a library so a game's
//! own binary can run it with its own name and defaults ([`AppConfig`], [`main_with`]); the
//! engine's binary is Shardfall (`src/main.rs`).

mod animation_ui;
mod animation_watch;
mod app;
mod arpg_items;
mod arpg_tree;
mod arpg_ui;
mod asset_ui;
mod asset_watch;
mod boot;
#[cfg(not(target_arch = "wasm32"))]
mod bridge;
pub mod config;
mod creature_ui;
mod creature_watch;
mod edit;
mod gfx;
mod guide_ui;
mod hud;
mod input;
mod look_ui;
mod panel;
mod platform;
mod quality;
mod rooms;
mod save;
mod settings;
mod simhost;
mod touch;
mod ui;
mod uiinput;

pub use config::{AppConfig, TouchMode};

fn fatal(msg: &str) -> ! {
    let path = boot::diag().log_path.display().to_string();
    log::error!("fatal: {msg}");
    let title = format!("{} could not start", config::get().title);
    platform::error_box(&title, &format!("{msg}\n\nThe full log is in:\n{path}"));
    std::process::exit(1);
}

/// Runs the app with this configuration: boot, settings, window and game loop. Call it from a
/// binary's `main`.
pub fn main_with(config: AppConfig) {
    config::set(config);
    let cfg = config::get();
    platform::attach_console();
    let d = boot::init();
    boot::install_panic_hook();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "{stem} [--room NAME | --scene NAME] [--seed N] [--backend vulkan|dx12] [--no-vsync] [--fullscreen] [--bridge [ADDR]]\n\
             --bridge opens the live agent bridge (default 127.0.0.1:7878; drive it with `pav live`).\n\
             --animation-studio [SET/Clip] opens the animation workspace and the live agent bridge.\n\
             --creature-studio [CREATURE/name] opens the creature workspace and the live agent bridge.\n\
             --asset-studio [SET/name] opens the object workspace and the live agent bridge.\n\
             Startup settings live in {toml} next to the executable.",
            stem = cfg.stem,
            toml = cfg.file("toml"),
        );
        return;
    }
    log::info!(
        "{} {} ({} {}, {} build) — log file: {}",
        cfg.title,
        env!("CARGO_PKG_VERSION"),
        if cfg!(target_arch = "wasm32") { "browser" } else { std::env::consts::OS },
        std::env::consts::ARCH,
        if cfg!(debug_assertions) { "debug" } else { "release" },
        d.log_path.display()
    );
    let settings = match boot::stage("settings", || {
        let (s, note) = settings::Settings::load(&args);
        Ok((s, note))
    }) {
        Ok(s) => s,
        Err(e) => fatal(&format!("{e:#}")),
    };
    // The browser's event loop runs on after `run` returns; errors show on the page.
    #[cfg(target_arch = "wasm32")]
    if let Err(e) = app::run(settings) {
        fatal(&format!("{e:#}"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    match std::panic::catch_unwind(|| app::run(settings)) {
        Ok(Ok(())) => log::info!("clean exit"),
        Ok(Err(e)) => fatal(&format!("{e:#}")),
        Err(_) => {
            let msg = boot::LAST_CRASH.lock().ok().and_then(|c| c.clone()).unwrap_or_else(|| "unknown crash".into());
            fatal(&format!("{} crashed:\n{msg}", cfg.title));
        }
    }
}
