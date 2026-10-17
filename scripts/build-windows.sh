#!/usr/bin/env bash
# Cross-compiles the Windows builds. Output: target/x86_64-pc-windows-gnu/dist/{shardfall,pav,
# grand_theft_freeway,gtf}.exe
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --profile dist --target x86_64-pc-windows-gnu -p pav_app -p pav_tools -p gtf_app
ls -la target/x86_64-pc-windows-gnu/dist/{shardfall,pav,grand_theft_freeway,gtf}.exe
