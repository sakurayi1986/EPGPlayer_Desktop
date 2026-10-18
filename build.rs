//! Linux 的两种链接方式：
//!
//! - 默认（开发用）：链接系统 libmpv。另外用 DT_RPATH 让 libmpv 优先加载
//!   vendor/linux-x86_64 里自带 libaribcaption 的 libavcodec（见 scripts/build-libavcodec-arib.sh）。
//! - EPGPLAYER_STATIC_PREFIX=<dir>（发布用，scripts/build-static.sh）：
//!   把 <dir> 里的静态 libmpv/ffmpeg 等链接进程序，其余（显卡、显示、音频、字体）用系统的。
//!
//! Windows（scripts/build-windows.sh）：链接 shinchiro/mpv-winbuild-cmake 的 libmpv.dll.a，
//! 运行时把 libmpv-2.dll 放在 exe 旁边。目录由 EPGPLAYER_MPV_WIN_DIR 指定。

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=EPGPLAYER_STATIC_PREFIX");
    println!("cargo:rerun-if-env-changed=EPGPLAYER_MPV_WIN_DIR");
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => {}
        Ok("windows") => return link_windows(),
        _ => return,
    }
    match std::env::var("EPGPLAYER_STATIC_PREFIX") {
        Ok(prefix) => link_static(Path::new(&prefix)),
        Err(_) => link_system(),
    }
}

fn link_system() {
    let vendor = Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/linux-x86_64");
    println!("cargo:rerun-if-changed={}", vendor.display());
    println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags,-rpath,$ORIGIN/lib:{}", vendor.display());
}

fn link_static(prefix: &Path) {
    let lib = prefix.join("lib");
    let list = prefix.join("mpv-static-libs.txt");
    println!("cargo:rerun-if-changed={}", list.display());
    let flags = std::fs::read_to_string(&list)
        .unwrap_or_else(|e| panic!("{}: {e}（先运行 scripts/static/build-deps.sh）", list.display()));

    // 不在 prefix 里、但多数发行版默认没装（或主版本号不一）的库，用构建环境自带的 .a 静态链接
    const SYSTEM_STATIC: &[&str] = &["ssl", "crypto", "lua5.2", "Xss", "Xpresent"];
    let system_lib = Path::new("/usr/lib/x86_64-linux-gnu");

    println!("cargo:rustc-link-search=native={}", lib.display());
    // libmpv2-sys 只输出 -lmpv；这里按 pkg-config --static 的顺序补上全部依赖。
    // 有 .a 的库用 -l:libX.a 强制静态，其余是系统动态库。
    let mut args = Vec::new();
    for f in flags.split_whitespace() {
        if let Some(dir) = f.strip_prefix("-L") {
            println!("cargo:rustc-link-search=native={dir}");
        } else if let Some(name) = f.strip_prefix("-l") {
            let archive = format!("lib{name}.a");
            let in_system = SYSTEM_STATIC.contains(&name) && system_lib.join(&archive).exists();
            if lib.join(&archive).exists() || in_system {
                args.push(format!("-l:{archive}"));
            } else {
                args.push(f.to_string());
            }
        } else {
            args.push(f.to_string());
        }
    }
    // libplacebo / libaribcaption 是 C++
    args.push("-lstdc++".into());
    for a in args {
        println!("cargo:rustc-link-arg={a}");
    }
}

fn link_windows() {
    let dir = std::env::var("EPGPLAYER_MPV_WIN_DIR").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/windows/mpv-dev").display().to_string()
    });
    println!("cargo:rerun-if-changed={dir}");
    println!("cargo:rustc-link-search=native={dir}");
}
