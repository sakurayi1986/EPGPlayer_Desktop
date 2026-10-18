#!/bin/sh
# Pack the exact sources of the C libraries statically linked into the Linux build
# (GPLv3 "Corresponding Source"). Attach the result to each GitHub release.
#
#   scripts/pack-sources.sh   ->  dist/EPGPlayer_Desktop-third-party-sources.tar.gz
#
# Requires a previous scripts/build-static.sh run (sources live in vendor/static/src).
set -eu
cd "$(dirname "$0")/.."
SRC=vendor/static/src
[ -d "$SRC/mpv" ] || { echo "run scripts/build-static.sh first" >&2; exit 1; }
mkdir -p dist
tar -C "$SRC" -czf dist/EPGPlayer_Desktop-third-party-sources.tar.gz \
    --exclude=build --exclude=.git \
    aribcaption ffmpeg placebo ass wayland-protocols mpv
ls -l dist/EPGPlayer_Desktop-third-party-sources.tar.gz
