//! 无窗口地打开一个 URL，打印 mpv 识别到的音视频参数。
//!   cargo run --example probe -- http://host:8888/api/videos/75
use libmpv2::{events::Event, Mpv};

fn main() -> libmpv2::Result<()> {
    let url = std::env::args().nth(1).expect("usage: probe <url>");
    let mpv = Mpv::with_initializer(|i| {
        i.set_property("vo", "null")?;
        i.set_property("ao", "null")?;
        i.set_property("pause", true)?;
        i.set_property("subs-fallback", "yes")?;
        i.set_property("keep-open", "yes")?;
        Ok(())
    })?;
    mpv.command("loadfile", &[url.as_str()])?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        match mpv.wait_event(0.5) {
            Some(Ok(Event::FileLoaded)) => {
                for p in ["file-format", "video-codec", "width", "height", "audio-codec-name", "duration"] {
                    let v: String = mpv.get_property(p).unwrap_or_else(|e| format!("<{e}>"));
                    println!("{p}: {v}");
                }
                let n: i64 = mpv.get_property("track-list/count").unwrap_or(0);
                for t in 0..n {
                    let get = |k: &str| -> String {
                        mpv.get_property(&format!("track-list/{t}/{k}")).unwrap_or_else(|_| "-".into())
                    };
                    println!(
                        "track {t}: type={} codec={} lang={} selected={} decoder={}",
                        get("type"), get("codec"), get("lang"), get("selected"), get("decoder-desc")
                    );
                }
                if std::env::var_os("PROBE_PLAY").is_some() {
                    // 实际解码几秒：确认硬解和字幕
                    mpv.set_property("hwdec", "vaapi-copy")?;
                    if std::env::var_os("PROBE_SEEK0").is_none() { mpv.command("seek", &["600", "absolute"])?; }
                    mpv.set_property("pause", false)?;
                    let mut subs = 0;
                    let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
                    while std::time::Instant::now() < end {
                        mpv.wait_event(0.2);
                        let start: Option<f64> = mpv.get_property("sub-start").ok();
                        if let Some(s) = start {
                            if subs == 0 || subs % 20 == 0 {
                                println!("sub-start={s:.2} sub-end={:?}", mpv.get_property::<f64>("sub-end").ok());
                            }
                            subs += 1;
                        }
                    }
                    let hw: String = mpv.get_property("hwdec-current").unwrap_or_else(|_| "-".into());
                    let pos: f64 = mpv.get_property("time-pos").unwrap_or(-1.0);
                    println!("hwdec-current: {hw}  time-pos: {pos:.1}  frames-with-sub: {subs}");
                }
                return Ok(());
            }
            Some(Ok(Event::EndFile(r))) => {
                println!("end-file reason {r}");
                std::process::exit(1);
            }
            _ => {}
        }
    }
    println!("timeout");
    std::process::exit(1)
}
