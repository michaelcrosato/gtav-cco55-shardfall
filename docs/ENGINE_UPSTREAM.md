# Engine upstream

The Pavilion engine in this repository (crates/pav_*, anim/, assets/, rooms/, game/, store/, tools/,
web/, scripts/ and the engine docs in docs/) was vendored from the Shardfall repository and is
modified here for Grand Theft Freeway-BX (see PLAN.md, "Engine extensions").

- Upstream: https://github.com/michaelcrosato/shardfall
- Commit: 0098196140d279979b622ee4373cf498d95fc9e1
- Commit date: 2026-10-16 17:00:25 -0700
- Commit subject: Co-op plan on hold (#74)
- Workspace version: 0.1.0 (Rust 1.98.1, wgpu 30, rapier 0.36, egui 0.36, glam 0.33, cpal 0.18)
- Left out: pavilion-lite/ (frozen; the upstream leaves it out of handoffs too)
- Moved: README.md -> docs/ENGINE_README.md, CLAUDE.md -> docs/ENGINE_CLAUDE.md

Engine changes made here are committed separately with an `engine:` prefix so they can be offered
upstream as a patch series (`git log --grep '^engine:'`).
