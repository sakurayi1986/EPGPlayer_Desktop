#!/bin/sh
# 构建自带 libmpv/ffmpeg 的单文件 Linux 版：
#   dist/epgplayer-linux-x86_64                 可执行文件
#   dist/EPGPlayer_Desktop-linux-x86_64.tar.gz  发布包（含 LICENSE、THIRD_PARTY.md）
#
#   scripts/build-static.sh          依赖库（首次 30~60 分钟，之后有缓存）+ 程序
#
# 全部在 debian:bookworm 容器里完成，生成的程序需要 glibc >= 2.36。
# 依赖库缓存在 vendor/static/，删掉即可从头重编。
set -eu
cd "$(dirname "$0")/.."
ROOT="$PWD"
mkdir -p vendor/static/prefix vendor/static/src vendor/static/cargo dist

IMAGE=epgplayer-static-build
docker build -q -t $IMAGE - <<'EOF' >/dev/null
FROM debian:bookworm
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
    build-essential git curl ca-certificates pkg-config nasm cmake ninja-build \
    python3 python3-venv file patchelf \
    libssl-dev zlib1g-dev liblua5.2-dev \
    libfreetype-dev libfontconfig-dev libfribidi-dev libharfbuzz-dev \
    libx11-dev libxext-dev libxrandr-dev libxinerama-dev libxss-dev libxpresent-dev \
    libxkbcommon-dev libwayland-dev wayland-protocols libegl-dev libgl-dev \
    libdrm-dev libva-dev libpulse-dev libpipewire-0.3-dev libasound2-dev \
    >/dev/null && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y -q --profile minimal --default-toolchain stable
ENV PATH=/root/.cargo/bin:$PATH
EOF

docker run --rm \
    -v "$ROOT:/work" -w /work \
    -v "$ROOT/vendor/static/prefix:/opt/static" \
    -v "$ROOT/vendor/static/src:/opt/src" \
    -v "$ROOT/vendor/static/cargo:/root/.cargo/registry" \
    -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    $IMAGE bash -euc '
        bash scripts/static/build-deps.sh
        EPGPLAYER_STATIC_PREFIX=/opt/static cargo build --release --target-dir target/static
        # 先写临时文件再改名：旧版本正在运行时也能替换（直接 cp 会 ETXTBSY）
        cp target/static/release/epgplayer-desktop dist/.epgplayer-linux-x86_64.new
        strip dist/.epgplayer-linux-x86_64.new
        mv -f dist/.epgplayer-linux-x86_64.new dist/epgplayer-linux-x86_64
        chown -R "$HOST_UID:$HOST_GID" dist target/static vendor/static
    '
PKG=EPGPlayer_Desktop-linux-x86_64
rm -rf "dist/$PKG" && mkdir -p "dist/$PKG"
cp dist/epgplayer-linux-x86_64 "dist/$PKG/epgplayer-desktop"
cp LICENSE THIRD_PARTY.md "dist/$PKG/"
python3 scripts/gen-third-party.py x86_64-unknown-linux-gnu > "dist/$PKG/THIRD_PARTY_LICENSES.txt"
mkdir -p "dist/$PKG/licenses"
for lib in aribcaption ass ffmpeg mpv placebo wayland-protocols; do
    for f in vendor/static/src/$lib/COPYING* vendor/static/src/$lib/LICENSE*; do
        [ -f "$f" ] && cp "$f" "dist/$PKG/licenses/$lib-$(basename "$f")"
    done
done
tar -C dist -czf "dist/$PKG.tar.gz" "$PKG"
ls -l dist/epgplayer-linux-x86_64 "dist/$PKG.tar.gz"
