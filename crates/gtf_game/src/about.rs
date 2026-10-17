//! The About screen: what the game is, what it is built on, and the source information
//! (PLAN.md §1 — the same facts as the README and the single-file HTML build).

/// The About screen's lines (big type, a few rows).
pub fn lines() -> Vec<String> {
    [
        "~o~GRAND THEFT FREEWAY-BX~s~ — an original open-world crime caper in Paloma Bay.",
        "A spiritual successor to ~y~Grand Theft Auto V~s~ (Rockstar North, 2013). Not affiliated.",
        "Built on the ~b~Pavilion engine~s~ (github.com/michaelcrosato/shardfall, 0098196) in Rust.",
        "Started 2026-10-17 in Claude Code, Anthropic's coding agent, in a cloud Linux session.",
        "Made by Anthropic Claude: planning at max effort, building at Extra effort.",
        "Fonts under the SIL Open Font License. Every model, sound and song is generated.",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}
