# EPGPlayer For Desktop

A desktop client for [EPGStation](https://github.com/l3tnun/EPGStation) v2 on **Linux and Windows**,
written in Rust with [egui](https://github.com/emilk/egui) and [libmpv](https://mpv.io/).

Its features follow [imxieyi/EPGPlayer](https://github.com/imxieyi/EPGPlayer) (iOS / iPadOS / macOS).
That app is built on SwiftUI, SwiftData and VLCKit and cannot be compiled outside Apple platforms,
so this is an independent reimplementation, not a port.
**It is unofficial and not affiliated with the EPGPlayer author.**

## Features

- **Live TV** – channels grouped by GR / BS / CS / SKY with the current program and progress;
  choose the stream quality (defaults to the unconverted TS, 「無変換」)
- **Program guide** – channel × time grid (each day starts at 04:00, as on Japanese TV),
  colored by genre, with a current-time line, date navigation and zoom.
  Click a program to see its details, watch it live, or add / cancel a manual recording reservation
  (● manual reservation, ◆ rule-based reservation, ⚠ conflict)
- **Recordings** – paged list with thumbnails and title search; play the original TS or encoded files
- **ARIB STD-B24 subtitles** via libaribcaption
- **Hardware decoding** – VAAPI on Linux, D3D11VA on Windows (4K HEVC 10-bit works)
- **Multiple servers** with optional HTTP Basic authentication and a connection test
- **UI languages** – 日本語 / 中文 / English (follows the system by default; switch in Settings)

Playback opens in a separate mpv window with mpv's on-screen controller and keyboard shortcuts
(`Space` pause, `f` fullscreen, `j` cycle subtitles, `i` stats).
Selecting another channel while the player is open switches it in place.

## Download and run

Download from [Releases](../../releases).

**Linux** – `EPGPlayer_Desktop-linux-x86_64.tar.gz`

```sh
tar xf EPGPlayer_Desktop-linux-x86_64.tar.gz
./EPGPlayer_Desktop-linux-x86_64/epgplayer-desktop
```

A single executable; libmpv, FFmpeg and the other media libraries are linked in.
It only needs libraries that every desktop Linux already has
(glibc ≥ 2.35, EGL/OpenGL, X11 or Wayland, libva, PulseAudio/PipeWire/ALSA, fontconfig/FreeType/HarfBuzz),
e.g. Ubuntu 22.04+, Debian 12+, Fedora 36+.

**Windows** – `EPGPlayer_Desktop-windows-x86_64.zip`

Unzip and run `EPGPlayerForDesktop.exe`. Keep `libmpv-2.dll` in the same folder.
Windows 10 / 11 (x64). The executable is not code-signed, so SmartScreen may warn on first launch
(“More info” → “Run anyway”).

On first launch the Settings page opens: add your EPGStation address (e.g. `192.168.0.10:8888`),
press **Test**, then **Save**.

Settings are stored in
`~/.config/epgplayer-desktop/config.toml` (Linux) or
`%APPDATA%\epgplayer-desktop\config\config.toml` (Windows).

## Building

All release builds run in Docker; no toolchain is needed on the host besides Docker and Python 3.

### Linux single-file build

```sh
scripts/build-static.sh   # first run 30–60 min (builds FFmpeg, mpv, ...), cached afterwards
# -> dist/EPGPlayer_Desktop-linux-x86_64.tar.gz
scripts/pack-sources.sh   # -> dist/EPGPlayer_Desktop-third-party-sources.tar.xz
```

Builds static libmpv v0.41.0, FFmpeg n8.0.1 (with libaribcaption and VAAPI), libplacebo and libass
in a Debian 12 container (see [`scripts/static/build-deps.sh`](scripts/static/build-deps.sh)).
Library sources are cached in `vendor/static/`; delete it to rebuild from scratch.

### Windows cross-build

```sh
scripts/build-windows.sh  # -> dist/EPGPlayer_Desktop-windows-x86_64.zip
```

Cross-compiles `x86_64-pc-windows-gnu` with mingw-w64 and bundles `libmpv-2.dll` from
[shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake)
(version and SHA-256 pinned in the script), whose FFmpeg already includes libaribcaption.

### Development build (Linux)

```sh
sudo apt install libmpv-dev          # Ubuntu / Debian
scripts/build-libavcodec-arib.sh     # optional: ARIB subtitles (see below)
cargo run --release
cargo test
```

This links against the system libmpv. Distribution FFmpeg packages usually lack an ARIB caption
decoder, so subtitles are detected but not shown. `scripts/build-libavcodec-arib.sh` builds a
`libavcodec.so` of the same FFmpeg version with libaribcaption into `vendor/linux-x86_64/`;
[`build.rs`](build.rs) adds a DT_RPATH so that libmpv picks it up. Rebuild it whenever the system
FFmpeg changes version. The single-file build above does not need this.

Check that a stream opens and which decoders are used (no window):

```sh
cargo run --example probe -- http://192.168.0.10:8888/api/videos/1
PROBE_PLAY=1 cargo run --example probe -- <url>   # also decode for 20 s: subtitles, hwdec
```

Japanese / Chinese fonts are taken from the system (Noto Sans CJK; Yu Gothic / Meiryo / Microsoft YaHei
on Windows). Set `EPGPLAYER_FONT=/path/to/font` to override.

## License

[GPL-3.0-or-later](LICENSE).

The release binaries include FFmpeg and mpv built in their GPL configuration, plus other
third-party libraries and Rust crates; see [THIRD_PARTY.md](THIRD_PARTY.md).
Each release archive contains `LICENSE`, `THIRD_PARTY.md` and `THIRD_PARTY_LICENSES.txt`,
and the Linux release is accompanied by the corresponding third-party sources.

## Acknowledgements

- [imxieyi/EPGPlayer](https://github.com/imxieyi/EPGPlayer) for the original app and its feature set
- [l3tnun/EPGStation](https://github.com/l3tnun/EPGStation)
- [mpv](https://mpv.io/), [FFmpeg](https://ffmpeg.org/), [libaribcaption](https://github.com/xqq/libaribcaption),
  [egui](https://github.com/emilk/egui), [shinchiro/mpv-winbuild-cmake](https://github.com/shinchiro/mpv-winbuild-cmake)
