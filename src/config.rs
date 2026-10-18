use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Server {
    pub name: String,
    /// 例: http://192.168.0.10:8888
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub servers: Vec<Server>,
    #[serde(default)]
    pub selected: usize,
    /// 直播画质名（EPGStation 的 streamConfig.live.ts.m2ts[].name）
    #[serde(default)]
    pub live_mode: Option<String>,
    /// 界面语言。None = 跟随系统
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<crate::i18n::Lang>,
}

/// 首次启动没有服务器，由用户在设置页添加
impl Default for Config {
    fn default() -> Self {
        Self { servers: Vec::new(), selected: 0, live_mode: None, language: None }
    }
}

pub fn path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "epgplayer-desktop")
        .map(|d| d.config_dir().join("config.toml"))
}

impl Server {
    /// 补上 http://、去掉末尾的 /
    pub fn normalized(&self) -> Self {
        let mut s = self.clone();
        s.name = s.name.trim().to_string();
        let url = s.url.trim().trim_end_matches('/');
        s.url = if url.contains("://") { url.to_string() } else { format!("http://{url}") };
        if s.name.is_empty() {
            s.name = s.url.split_once("://").map(|(_, h)| h).unwrap_or(&s.url).to_string();
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_server_url() {
        let s = Server { url: " 192.168.0.10:8888/ ".into(), ..Default::default() }.normalized();
        assert_eq!(s.url, "http://192.168.0.10:8888");
        assert_eq!(s.name, "192.168.0.10:8888");
        let s = Server { name: "家".into(), url: "https://epg.example/".into(), ..Default::default() }.normalized();
        assert_eq!((s.name.as_str(), s.url.as_str()), ("家", "https://epg.example"));
    }
}

impl Config {
    pub fn load() -> Self {
        let Some(p) = path() else { return Self::default() };
        match std::fs::read_to_string(&p) {
            Ok(s) => toml::from_str(&s).unwrap_or_else(|e| {
                eprintln!("{}: {e}", p.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let p = path().context(crate::i18n::tr("設定ディレクトリが見つかりません", "找不到配置目录", "Config directory not found"))?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn server(&self) -> Option<&Server> {
        self.servers.get(self.selected)
    }
}
