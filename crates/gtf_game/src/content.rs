//! Game data: TOML under `content/freeway/`, embedded at build time (the browser and a lone
//! executable have no files). When the program runs from the repository's root on a desktop, the
//! file on disk wins, so an edit shows at the next `load` without a rebuild — the engine's
//! `level_text` pattern (PLAN.md §6.8).

use std::borrow::Cow;

/// The text of `content/freeway/<rel>`: the file on disk when running from the repository,
/// else `built_in` (the copy embedded with `include_str!`).
pub fn text(rel: &str, built_in: &'static str) -> Cow<'static, str> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(text) = std::fs::read_to_string(format!("content/freeway/{rel}")) {
        return text.into();
    }
    #[cfg(target_arch = "wasm32")]
    let _ = rel; // the browser has no files
    built_in.into()
}
