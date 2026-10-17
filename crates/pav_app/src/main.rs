//! Shardfall — the engine's own game executable (the app itself is the `pav_app` library).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    pav_app::main_with(pav_app::AppConfig::shardfall());
}
