//! The game's own agent tools, registered into `gtf` beside every engine tool
//! (`pav_tools::tools::register`). They need the game's data, so they live here rather than in
//! `pav_tools` (PLAN.md §6.5 E12): `perf`, `soak`, `traffic`, `peds`, `vehicle_feel` arrive with
//! the systems they measure.

use pav_tools::Tool;

pub static TOOLS: &[Tool] = &[];
