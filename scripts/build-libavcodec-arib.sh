#!/bin/sh
# 生成 vendor/linux-x86_64/{libavcodec.so.62,libaribcaption.so}
#
# Ubuntu 的 ffmpeg 没有启用 ARIB 字幕解码器。这里在 Docker 里用同版本（8.0.1）
# 编译一个启用 libaribcaption 的 libavcodec，只替换这一个库；
# 其余 ffmpeg 库和 libmpv 都用系统的。版本必须和系统 libavcodec.so.62 一致。
set -eu
cd "$(dirname "$0")/.."
OUT="$PWD/vendor/linux-x86_64"
mkdir -p "$OUT"

docker run --rm -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" -v "$OUT:/out" ubuntu:26.04 bash -euxc '
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq build-essential nasm pkg-config cmake git ca-certificates patchelf \
    libva-dev libdrm-dev libfreetype-dev libfontconfig-dev >/dev/null
cd /tmp
git clone -q --depth 1 -b v1.1.1 https://github.com/xqq/libaribcaption
cmake -S libaribcaption -B arib-build -DCMAKE_BUILD_TYPE=Release \
    -DARIBCC_SHARED_LIBRARY=ON -DCMAKE_INSTALL_PREFIX=/opt/ff -DCMAKE_INSTALL_LIBDIR=lib
cmake --build arib-build -j"$(nproc)"
cmake --install arib-build
git clone -q --depth 1 -b n8.0.1 https://github.com/FFmpeg/FFmpeg.git ffmpeg
cd ffmpeg
PKG_CONFIG_PATH=/opt/ff/lib/pkgconfig ./configure --prefix=/opt/ff \
    --enable-shared --disable-static --disable-programs --disable-doc \
    --enable-gpl --enable-version3 \
    --enable-libaribcaption --enable-vaapi --enable-libdrm
make -j"$(nproc)"
make install
patchelf --set-rpath "\$ORIGIN" /opt/ff/lib/libavcodec.so.62.*.*
patchelf --set-rpath "\$ORIGIN" /opt/ff/lib/libaribcaption.so
cp -a /opt/ff/lib/libavcodec.so.62* /opt/ff/lib/libaribcaption.so /out/
chown -R "$HOST_UID:$HOST_GID" /out
'
ls -l "$OUT"
