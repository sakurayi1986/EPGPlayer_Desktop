#!/bin/sh
# 交叉编译 Windows 版：dist/EPGPlayer_Desktop-windows-x86_64.zip
#
#   scripts/build-windows.sh
#
# libmpv 用 shinchiro/mpv-winbuild-cmake 的 mpv-dev（含 libaribcaption 的 ffmpeg）。
# 在 debian:bookworm 容器里用 mingw-w64 编译 x86_64-pc-windows-gnu。
set -eu
cd "$(dirname "$0")/.."
ROOT="$PWD"

MPV_TAG=20261004
MPV_ASSET=mpv-dev-x86_64-20261004-git-413ff0b1cd.7z
MPV_SHA256=445fc72a8aa10980504355a5d99c9a0a128c2fa177e029996fb312d9d3193d45

MPV_DIR=vendor/windows/mpv-dev
if [ ! -e "$MPV_DIR/libmpv-2.dll" ]; then
    mkdir -p vendor/windows
    curl -fsSL -o vendor/windows/mpv-dev.7z \
        "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/$MPV_TAG/$MPV_ASSET"
    echo "$MPV_SHA256  vendor/windows/mpv-dev.7z" | sha256sum -c -
fi

mkdir -p dist vendor/static/cargo target
cargo fetch -q
python3 scripts/gen-third-party.py x86_64-pc-windows-gnu > target/THIRD_PARTY_LICENSES-windows.txt
IMAGE=epgplayer-windows-build
docker build -q -t $IMAGE - <<'DOCKERFILE' >/dev/null
FROM debian:bookworm
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
    build-essential gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 curl ca-certificates p7zip-full zip \
    >/dev/null && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain stable \
    --target x86_64-pc-windows-gnu
ENV PATH=/root/.cargo/bin:$PATH
DOCKERFILE

docker run --rm \
    -v "$ROOT:/work" -w /work \
    -v "$ROOT/vendor/static/cargo:/root/.cargo/registry" \
    -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    $IMAGE bash -euc '
        if [ ! -e vendor/windows/mpv-dev/libmpv-2.dll ]; then
            7z x -y -ovendor/windows/mpv-dev vendor/windows/mpv-dev.7z >/dev/null
        fi
        export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
        export EPGPLAYER_MPV_WIN_DIR=/work/vendor/windows/mpv-dev
        cargo build --release --target x86_64-pc-windows-gnu --target-dir target/windows
        cargo build --release --target x86_64-pc-windows-gnu --target-dir target/windows --example probe

        OUT=dist/EPGPlayer_Desktop-windows-x86_64
        rm -rf "$OUT" && mkdir -p "$OUT"
        cp target/windows/x86_64-pc-windows-gnu/release/epgplayer-desktop.exe "$OUT/EPGPlayerForDesktop.exe"
        x86_64-w64-mingw32-strip "$OUT/EPGPlayerForDesktop.exe"
        cp vendor/windows/mpv-dev/libmpv-2.dll "$OUT/"
        cp LICENSE THIRD_PARTY.md "$OUT/"
        cp target/THIRD_PARTY_LICENSES-windows.txt "$OUT/THIRD_PARTY_LICENSES.txt"
        (cd dist && rm -f "$(basename "$OUT").zip" && zip -qr "$(basename "$OUT").zip" "$(basename "$OUT")")
        chown -R "$HOST_UID:$HOST_GID" dist target/windows vendor/windows vendor/static/cargo
    '
ls -l dist/EPGPlayer_Desktop-windows-x86_64 dist/EPGPlayer_Desktop-windows-x86_64.zip
