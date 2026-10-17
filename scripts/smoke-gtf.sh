#!/usr/bin/env bash
# Runs Grand Theft Freeway-BX's native window under Xvfb + lavapipe and saves a screenshot of the
# whole window (egui UI included) to out/smoke-gtf/ (PLAN.md §7.6).
# Usage: scripts/smoke-gtf.sh [seconds] [game args...]
#   e.g. scripts/smoke-gtf.sh 20 --scene freeway/roam --pad-script out/drive.pad
set -euo pipefail
cd "$(dirname "$0")/.."
secs="${1:-12}"
shift || true
out=out/smoke-gtf
mkdir -p "$out"
cargo build -q -p gtf_app --bin grand_theft_freeway
export DISPLAY=:97
Xvfb :97 -screen 0 1600x900x24 >/dev/null 2>&1 &
xvfb=$!
trap 'kill $xvfb 2>/dev/null || true' EXIT
sleep 1
(timeout $((secs + 5)) ./target/debug/grand_theft_freeway "$@" >"$out/game.log" 2>&1 &)
sleep "$secs"
xwd -root -silent | convert xwd:- "$out/screen.png"
echo "screenshot: $out/screen.png"
grep -E "boot|ERROR|CRASH|panicked" "$out/game.log" | head -40
