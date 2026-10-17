#!/usr/bin/env bash
# Vendors the Pavilion engine (the michaelcrosato/shardfall repository) into this repository at a
# pinned commit: milestone M0 of PLAN.md. Run once from a clean working tree:
#
#   scripts/vendor-engine.sh            # the pin below
#   scripts/vendor-engine.sh <commit>   # another upstream commit (record why in PROGRESS.md)
#
# What it does:
#   - fetches exactly one upstream commit (shallow, anonymous read);
#   - copies every tracked file except pavilion-lite/ (a frozen older engine the upstream leaves out
#     of handoffs) into this repository's root, keeping upstream paths so the engine's scripts,
#     tests and docs work unchanged;
#   - keeps this repository's own files: README.md, CLAUDE.md, PLAN.md, PROGRESS.md and scripts/
#     vendor-engine.sh are never overwritten. The engine's README.md and CLAUDE.md land in
#     docs/ENGINE_README.md and docs/ENGINE_CLAUDE.md instead;
#   - records the pin in docs/ENGINE_UPSTREAM.md.
# It refuses to run twice (docs/ENGINE_UPSTREAM.md exists) or on a dirty tree.
set -euo pipefail

UPSTREAM=https://github.com/michaelcrosato/shardfall
PIN=${1:-0098196140d279979b622ee4373cf498d95fc9e1}

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
if [[ -e docs/ENGINE_UPSTREAM.md ]]; then
	echo "vendor-engine.sh: already vendored (docs/ENGINE_UPSTREAM.md exists)" >&2
	exit 1
fi
if [[ -n $(git status --porcelain) ]]; then
	echo "vendor-engine.sh: commit or stash your changes first" >&2
	exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git init -q "$tmp/src"
git -C "$tmp/src" remote add origin "$UPSTREAM"
# LFS objects are not served on the anonymous lane; the engine tracks none, but never let a smudge
# filter abort the fetch.
GIT_LFS_SKIP_SMUDGE=1 git -C "$tmp/src" fetch -q --depth 1 origin "$PIN"
GIT_LFS_SKIP_SMUDGE=1 git -C "$tmp/src" checkout -q FETCH_HEAD
full=$(git -C "$tmp/src" rev-parse HEAD)
subject=$(git -C "$tmp/src" log -1 --format='%s')
date=$(git -C "$tmp/src" log -1 --format='%ad' --date=iso)

mkdir -p "$tmp/stage"
(cd "$tmp/src" && git ls-files -z -- . ':!:pavilion-lite' | tar --null --no-recursion -T - -cf -) | tar -C "$tmp/stage" -xf -

# Files this repository owns: the engine's copies move aside.
mkdir -p "$tmp/stage/docs"
mv "$tmp/stage/README.md" "$tmp/stage/docs/ENGINE_README.md"
mv "$tmp/stage/CLAUDE.md" "$tmp/stage/docs/ENGINE_CLAUDE.md"
for own in README.md CLAUDE.md PLAN.md PROGRESS.md scripts/vendor-engine.sh; do
	rm -f "$tmp/stage/$own"
done

# Refuse to clobber anything else that already exists here (a second engine, a stray file).
clash=$(cd "$tmp/stage" && find . -type f | sed 's|^\./||' | while read -r f; do [[ -e "$root/$f" ]] && echo "$f"; done || true)
if [[ -n $clash ]]; then
	echo "vendor-engine.sh: these paths already exist here; move them first:" >&2
	echo "$clash" >&2
	exit 1
fi
cp -a "$tmp/stage/." "$root/"

cat >docs/ENGINE_UPSTREAM.md <<EOF
# Engine upstream

The Pavilion engine in this repository (crates/pav_*, anim/, assets/, rooms/, game/, store/, tools/,
web/, scripts/ and the engine docs in docs/) was vendored from the Shardfall repository and is
modified here for Grand Theft Freeway-BX (see PLAN.md, "Engine extensions").

- Upstream: $UPSTREAM
- Commit: $full
- Commit date: $date
- Commit subject: $subject
- Workspace version: 0.1.0 (Rust 1.98.1, wgpu 30, rapier 0.36, egui 0.36, glam 0.33, cpal 0.18)
- Left out: pavilion-lite/ (frozen; the upstream leaves it out of handoffs too)
- Moved: README.md -> docs/ENGINE_README.md, CLAUDE.md -> docs/ENGINE_CLAUDE.md

Engine changes made here are committed separately with an \`engine:\` prefix so they can be offered
upstream as a patch series (\`git log --grep '^engine:'\`).
EOF

echo "Vendored $full ($subject) into $root"
echo "Files: $(cd "$tmp/stage" && find . -type f | wc -l)"
