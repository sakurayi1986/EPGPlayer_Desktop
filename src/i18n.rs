//! 界面语言：日本語 / 中文 / English
//!
//! 文字就地写三份：`tr("日本語", "中文", "English")`；带参数的用 `trf!`。

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Ja,
    Zh,
    En,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Ja, Lang::Zh, Lang::En];

    /// 各语言用自己的文字写名字
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Ja => "日本語",
            Lang::Zh => "中文",
            Lang::En => "English",
        }
    }

    /// 由 BCP 47 标签（ja-JP、zh-CN、en-US …）判断；不认识的用英文
    pub fn from_locale(tag: &str) -> Lang {
        let tag = tag.to_ascii_lowercase();
        if tag.starts_with("ja") {
            Lang::Ja
        } else if tag.starts_with("zh") {
            Lang::Zh
        } else {
            Lang::En
        }
    }

    pub fn detect() -> Lang {
        sys_locale::get_locale().map(|l| Lang::from_locale(&l)).unwrap_or(Lang::En)
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Zh,
        2 => Lang::En,
        _ => Lang::Ja,
    }
}

pub fn tr(ja: &'static str, zh: &'static str, en: &'static str) -> &'static str {
    match current() {
        Lang::Ja => ja,
        Lang::Zh => zh,
        Lang::En => en,
    }
}

/// `trf!("{} 件", "{} 条", "{} items", n)`：参数只在选中的那一种语言里求值一次
#[macro_export]
macro_rules! trf {
    ($ja:literal, $zh:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::current() {
            $crate::i18n::Lang::Ja => format!($ja $(, $arg)*),
            $crate::i18n::Lang::Zh => format!($zh $(, $arg)*),
            $crate::i18n::Lang::En => format!($en $(, $arg)*),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_tags() {
        assert_eq!(Lang::from_locale("ja-JP"), Lang::Ja);
        assert_eq!(Lang::from_locale("zh-Hans-CN"), Lang::Zh);
        assert_eq!(Lang::from_locale("zh_TW"), Lang::Zh);
        assert_eq!(Lang::from_locale("en-US"), Lang::En);
        assert_eq!(Lang::from_locale("fr-FR"), Lang::En);
    }

    #[test]
    fn switch_and_translate() {
        for (l, want) in [(Lang::Zh, "中"), (Lang::En, "en"), (Lang::Ja, "日")] {
            set(l);
            assert_eq!(current(), l);
            assert_eq!(tr("日", "中", "en"), want);
        }
        set(Lang::En);
        assert_eq!(trf!("{}件", "{}条", "{} items", 3), "3 items");
    }
}
