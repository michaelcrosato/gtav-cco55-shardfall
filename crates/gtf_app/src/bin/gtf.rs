//! `gtf` — Grand Theft Freeway-BX's agent CLI: the engine's `pav` with the game and its tools
//! registered (`gtf help`).

fn main() -> anyhow::Result<()> {
    gtf_game::register();
    pav_tools::tools::register(gtf_app::tools::TOOLS);
    pav_tools::tools::register_sounds(gtf_app::SOUNDS);
    pav_tools::cli::run(&gtf_app::CLI)
}
