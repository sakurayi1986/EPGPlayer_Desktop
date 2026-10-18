#!/bin/bash
# 在 debian:bookworm 容器里运行（由 scripts/build-static.sh 调用）。
# 把 libmpv 及其不属于"桌面基础库"的依赖编译成静态库，装到 $PREFIX。
#
# 静态：libmpv ffmpeg libplacebo libass libaribcaption lua openssl
# 动态（系统）：glibc libstdc++ EGL/GL X11 wayland xkbcommon libva libdrm
#              pulse pipewire alsa fontconfig freetype harfbuzz fribidi zlib
set -euxo pipefail

PREFIX=/opt/static
SRC=/opt/src
JOBS=$(nproc)
export PKG_CONFIG_PATH=$PREFIX/lib/pkgconfig:$PREFIX/share/pkgconfig
export PATH=$PREFIX/venv/bin:$PATH
mkdir -p "$PREFIX" "$SRC"

V_ARIB=v1.1.1
V_FFMPEG=n8.0.1
V_PLACEBO=v7.351.0
V_ASS=0.17.3
V_MPV=v0.41.0

stamp() { [ -e "$PREFIX/.done-$1" ]; }
done_() { touch "$PREFIX/.done-$1"; }

fetch() { # url dir [git-ref]
    [ -d "$SRC/$2" ] && return
    if [ $# -ge 3 ]; then
        git clone -q --depth 1 --recurse-submodules --shallow-submodules -b "$3" "$1" "$SRC/$2"
    else
        mkdir -p "$SRC/$2"
        curl -fsSL "$1" | tar -xz --strip-components=1 -C "$SRC/$2"
    fi
}

# meson 新版本（mpv 要求 >= 1.3），libplacebo 的代码生成要 jinja2
if ! stamp venv; then
    python3 -m venv "$PREFIX/venv"
    "$PREFIX/venv/bin/pip" install -q meson==1.6.1 jinja2
    done_ venv
fi

# ---- openssl 的静态库：bookworm 的 libssl-dev 已带 libssl.a，直接用 ----

if ! stamp aribcaption; then
    fetch https://github.com/xqq/libaribcaption aribcaption $V_ARIB
    cmake -S "$SRC/aribcaption" -B "$SRC/aribcaption/build" -G Ninja \
        -DCMAKE_BUILD_TYPE=Release -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
        -DARIBCC_SHARED_LIBRARY=OFF -DCMAKE_INSTALL_PREFIX=$PREFIX -DCMAKE_INSTALL_LIBDIR=lib
    cmake --build "$SRC/aribcaption/build" -j$JOBS
    cmake --install "$SRC/aribcaption/build"
    done_ aribcaption
fi

# libaribcaption 自带的 md5.c 用了与 OpenSSL 相同的 MD5_Init/Update/Final，
# 和静态 libcrypto 一起链接会重复定义。它们只在库内部使用，改名即可。
if ! stamp aribcaption-md5; then
    for s in MD5_Init MD5_Update MD5_Final; do
        objcopy --redefine-sym $s=aribcc_$s $PREFIX/lib/libaribcaption.a
    done
    done_ aribcaption-md5
fi

if ! stamp ffmpeg; then
    fetch https://github.com/FFmpeg/FFmpeg.git ffmpeg $V_FFMPEG
    cd "$SRC/ffmpeg"
    ./configure --prefix=$PREFIX --pkg-config-flags=--static \
        --enable-static --disable-shared --enable-pic \
        --disable-programs --disable-doc --disable-debug \
        --disable-avdevice \
        --enable-gpl --enable-version3 \
        --disable-autodetect \
        --enable-zlib --enable-vaapi --enable-libdrm \
        --enable-openssl \
        --enable-libaribcaption \
        --disable-encoders --enable-encoder=png,mjpeg \
        --disable-muxers --enable-muxer=mpegts,matroska,mp4,image2
    make -j$JOBS
    make install
    cd /
    done_ ffmpeg
fi

if ! stamp placebo; then
    fetch https://github.com/haasn/libplacebo placebo $V_PLACEBO
    meson setup "$SRC/placebo/build" "$SRC/placebo" --prefix=$PREFIX --libdir=lib \
        --buildtype=release -Ddefault_library=static -Db_staticpic=true \
        -Dvulkan=disabled -Dopengl=enabled -Dd3d11=disabled \
        -Dshaderc=disabled -Dglslang=disabled -Dlcms=disabled -Ddovi=enabled \
        -Dlibdovi=disabled -Dunwind=disabled -Dxxhash=disabled \
        -Ddemos=false -Dtests=false -Dbench=false -Dfuzz=false
    meson install -C "$SRC/placebo/build"
    done_ placebo
fi

if ! stamp ass; then
    fetch https://github.com/libass/libass/releases/download/$V_ASS/libass-$V_ASS.tar.gz ass
    cd "$SRC/ass"
    ./configure --prefix=$PREFIX --enable-static --disable-shared --with-pic
    make -j$JOBS
    make install
    cd /
    done_ ass
fi

# bookworm 的 wayland-protocols 1.31 没有 color-management-v1，mpv 0.41 的代码默认它存在。
# 只是协议 XML，编译期生成代码，运行时不产生依赖。
if ! stamp wayland-protocols; then
    fetch https://gitlab.freedesktop.org/wayland/wayland-protocols/-/archive/1.45/wayland-protocols-1.45.tar.gz wayland-protocols
    meson setup "$SRC/wayland-protocols/build" "$SRC/wayland-protocols" --prefix=$PREFIX -Dtests=false
    meson install -C "$SRC/wayland-protocols/build"
    rm -rf "$SRC/mpv/build"
    done_ wayland-protocols
fi

if ! stamp mpv; then
    fetch https://github.com/mpv-player/mpv mpv $V_MPV
    meson setup "$SRC/mpv/build" "$SRC/mpv" --prefix=$PREFIX --libdir=lib \
        --buildtype=release -Ddefault_library=static -Db_staticpic=true \
        -Dlibmpv=true -Dcplayer=false -Dtests=false \
        -Dlua=lua5.2 -Djavascript=disabled \
        -Dlibarchive=disabled -Dlibbluray=disabled -Ddvdnav=disabled -Dcdda=disabled \
        -Drubberband=disabled -Duchardet=disabled -Dvapoursynth=disabled -Dzimg=disabled \
        -Dlcms2=disabled -Djpeg=disabled -Diconv=enabled \
        -Djack=disabled -Dopenal=disabled -Dsndio=disabled -Doss-audio=disabled \
        -Dsdl2-audio=disabled -Dsdl2-video=disabled -Dsdl2-gamepad=disabled \
        -Dcaca=disabled -Dsixel=disabled \
        -Dvulkan=disabled -Dshaderc=disabled -Dspirv-cross=disabled \
        -Dcuda-hwaccel=disabled -Dcuda-interop=disabled -Dvdpau=disabled \
        -Ddmabuf-wayland=enabled -Dvaapi=enabled \
        -Dx11=enabled -Dwayland=enabled -Degl=enabled -Dgl=enabled \
        -Dpulse=enabled -Dpipewire=enabled -Dalsa=enabled \
        -Dmanpage-build=disabled -Dhtml-build=disabled
    meson install -C "$SRC/mpv/build"
    done_ mpv
fi

pkg-config --static --libs mpv > "$PREFIX/mpv-static-libs.txt"
cat "$PREFIX/mpv-static-libs.txt"
