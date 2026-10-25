//! The About screen: what the game is, the source information (PLAN.md §1 — the same facts as
//! the README and the single-file HTML build) and the credits, a page at a time (L1/R1), each a
//! few rows of big type.

/// The About pages, as tabs.
pub const PAGES: [&str; 5] = ["GAME", "SOURCE", "ENGINE", "MADE BY", "CREDITS"];

/// Page `i`'s heading and lines (wrapping round).
pub fn page(i: usize) -> (String, Vec<String>) {
    let i = i % PAGES.len();
    let lines: &[&str] = match i {
        0 => &[
            "~o~GRAND THEFT FREEWAY-BX~s~ — an original open-world crime caper in Paloma Bay.",
            "Three criminals, one city, one job on the freeway: Dez, Mack and Roxy.",
            "A spiritual successor to ~y~Grand Theft Auto V~s~. Not affiliated with its makers.",
            "Initial date: 2026-10-17 (the planning session's first prompt, created 00:13 UTC).",
        ],
        1 => &[
            "~y~Grand Theft Auto V~s~ — Rockstar North / Rockstar Games, 2013.",
            "On PlayStation 3 and Xbox 360; later PlayStation 4, Xbox One, PC,",
            "PlayStation 5 and Xbox Series X|S.",
        ],
        2 => &[
            "The ~b~Pavilion engine~s~ from the Shardfall repository:",
            "github.com/michaelcrosato/shardfall at 0098196140d279979b622ee4373cf498d95fc9e1",
            "(\"Co-op plan on hold (#74)\", 2026-10-16), workspace version 0.1.0.",
            "Rust 1.98.1 · wgpu 30 (WebGPU in the browser, Vulkan/DX12 native) · rapier3d 0.36",
            "egui 0.36 · glam 0.33 · cpal 0.18 · gilrs 0.11",
            "Route: START_HERE.md, then docs/MAKE_A_GAME.md, extended for this game (PLAN §6).",
        ],
        3 => &[
            "Claude Code (container build 2.1.302) in an Anthropic-managed cloud session",
            "started from the Claude desktop app: Ubuntu 24.04.5 LTS, 4 vCPU, 15 GB RAM, no GPU.",
            "Pictures on Mesa lavapipe; browser checks in headless Chromium (SwiftShader WebGPU).",
            "Played on Windows (an NVIDIA RTX 3060 Ti) and in WebGPU browsers (Chrome, Edge).",
            "Anthropic Claude — exact model and effort level in the delivery report.",
            "Planning ran at reasoning effort max; building was handed off at Extra.",
        ],
        _ => &[
            "Fonts: Anton (The Anton Project Authors) and Barlow Semi Condensed",
            "(The Barlow Project Authors), under the SIL Open Font License 1.1.",
            "Motion: Quaternius' Universal Animation Library and Mesh2Motion (CC0 1.0);",
            "100STYLE by Mason, Starke and Komura (CC BY 4.0).",
            "Everything else — the city, cars, people, props, sounds, voices, music — is code.",
        ],
    };
    (PAGES[i].to_string(), lines.iter().map(|s| s.to_string()).collect())
}

/// Every page's lines in turn (the README and the tests read the same facts).
pub fn lines() -> Vec<String> {
    (0..PAGES.len()).flat_map(|i| page(i).1).collect()
}
