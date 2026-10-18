//! egui 自带字体没有中日文字。从系统里找日文和简体中文字体加到回退列表，
//! 按界面语言决定哪个优先（同一个汉字中日字形不同，如「直」「骨」）。

use eframe::egui;
use egui::{FontData, FontDefinitions, FontFamily};

use crate::i18n::Lang;

/// (路径, ttc 里的字形下标)
const JA: &[(&str, u32)] = &[
    ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/truetype/fonts-japanese-gothic.ttf", 0),
    ("C:\\Windows\\Fonts\\YuGothM.ttc", 0),
    ("C:\\Windows\\Fonts\\meiryo.ttc", 0),
    ("C:\\Windows\\Fonts\\msgothic.ttc", 0),
];

const ZH: &[(&str, u32)] = &[
    ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 2),
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 2),
    ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc", 2),
    ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", 0),
    ("/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf", 0),
    ("C:\\Windows\\Fonts\\msyh.ttc", 0),
    ("C:\\Windows\\Fonts\\simsun.ttc", 0),
];

fn find(candidates: &[(&str, u32)], cache: &mut Vec<(String, FontData)>) -> Option<FontData> {
    candidates.iter().find_map(|(path, index)| {
        // Noto CJK 的 ttc 同时含日文和中文，只读一次
        let base = match cache.iter().find(|(p, _)| p == path) {
            Some((_, d)) => d.clone(),
            None => {
                let d = FontData::from_owned(std::fs::read(path).ok()?);
                cache.push((path.to_string(), d.clone()));
                d
            }
        };
        let mut d = base;
        d.index = *index;
        Some(d)
    })
}

/// 启动时和切换语言时调用
pub fn install(ctx: &egui::Context, lang: Lang) {
    let mut cache = Vec::new();
    let custom = std::env::var("EPGPLAYER_FONT")
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .map(FontData::from_owned);
    let ja = find(JA, &mut cache);
    let zh = find(ZH, &mut cache);

    let mut order = vec![("custom", custom)];
    if lang == Lang::Zh {
        order.extend([("zh", zh), ("ja", ja)]);
    } else {
        order.extend([("ja", ja), ("zh", zh)]);
    }

    let mut defs = FontDefinitions::default();
    let mut any = false;
    for (name, data) in order {
        let Some(data) = data else { continue };
        any = true;
        defs.font_data.insert(name.to_string(), data.into());
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            defs.families.entry(family).or_default().push(name.to_string());
        }
    }
    if !any {
        eprintln!("No CJK font found; set EPGPLAYER_FONT=/path/to/font");
    }
    ctx.set_fonts(defs);
}
