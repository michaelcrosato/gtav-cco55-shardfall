#!/usr/bin/env bash
# Cross-compiles the Windows builds. Output: target/x86_64-pc-windows-gnu/dist/{shardfall,pav,
# grand_theft_freeway,gtf}.exe, and the game for players zipped as
# dist/GrandTheftFreeway-BX-windows.zip (GrandTheftFreeway-BX.exe, a read-me and the fonts'
# licences).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --profile dist --target x86_64-pc-windows-gnu -p pav_app -p pav_tools -p gtf_app
bin=target/x86_64-pc-windows-gnu/dist
ls -la "$bin"/{shardfall,pav,grand_theft_freeway,gtf}.exe

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
pkg="$stage/GrandTheftFreeway-BX"
mkdir -p "$pkg/licences"
cp "$bin/grand_theft_freeway.exe" "$pkg/GrandTheftFreeway-BX.exe"
cp assets/fonts/anton/OFL.txt "$pkg/licences/Anton-OFL.txt"
cp assets/fonts/barlowsemicondensed/OFL.txt "$pkg/licences/BarlowSemiCondensed-OFL.txt"
cat > "$pkg/README.txt" <<'EOF'
Grand Theft Freeway-BX
======================
An open-world crime caper in Paloma Bay, built on the Pavilion engine.

Run GrandTheftFreeway-BX.exe (a GPU with Vulkan or DirectX 12). A controller is recommended
(DualSense, DualShock or Xbox); keyboard and mouse work too: WASD to move, the mouse to look,
F to get in and out of cars, W/S to drive and brake, Space to jump or handbrake, Esc to pause.
Settings and saves are kept next to the .exe.

Leave the title screen alone for ten seconds and the game plays itself; any key brings you back.

Fonts: Anton and Barlow Semi Condensed, SIL Open Font License 1.1 (see licences/).
EOF
mkdir -p dist
rm -f dist/GrandTheftFreeway-BX-windows.zip
(cd "$stage" && zip -qr9 - GrandTheftFreeway-BX) > dist/GrandTheftFreeway-BX-windows.zip
ls -la dist/GrandTheftFreeway-BX-windows.zip
