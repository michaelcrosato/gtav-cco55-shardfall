//! `pav` — the engine's headless agent CLI (`pav_tools::cli` documents the commands).

fn main() -> anyhow::Result<()> {
    pav_tools::cli::run(&pav_tools::cli::Cli::PAV)
}
