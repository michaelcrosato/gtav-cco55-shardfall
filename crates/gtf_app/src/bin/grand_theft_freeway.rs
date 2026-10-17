//! Grand Theft Freeway-BX — the game executable.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    gtf_game::register();
    pav_app::main_with(gtf_app::app_config());
}
