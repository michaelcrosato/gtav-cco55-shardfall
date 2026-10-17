#!/usr/bin/env bash
# Builds Grand Theft Freeway-BX for the browser as one self-contained page that runs from file://
# in Chrome or Edge (WebGPU): dist/GrandTheftFreeway-BX.html (PLAN.md §6.11). Needs the
# wasm32-unknown-unknown target and wasm-bindgen-cli at Cargo.lock's version (scripts/build-web.sh).
# Extra arguments go to scripts/build-web.sh (e.g. --out PATH).
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p dist
scripts/build-web.sh --single-file --package gtf_app --bin grand_theft_freeway --scene freeway \
	--title "Grand Theft Freeway-BX" \
	--description "Grand Theft Freeway-BX: an open-world crime caper in Paloma Bay, built on the Pavilion engine." \
	--theme-color "#ff7a3d" --out dist/GrandTheftFreeway-BX.html "$@"
