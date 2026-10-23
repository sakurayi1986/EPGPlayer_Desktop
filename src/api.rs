//! EPGStation v2 REST API（只实现客户端用到的部分）

use std::time::Duration;

use anyhow::{bail, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::config::Server;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub half_width_name: Option<String>,
    pub channel_type: String,
    #[serde(default)]
    pub has_logo_data: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    pub start_at: i64,
    pub end_at: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Broadcasting {
    pub channel: Channel,
    #[serde(default)]
    pub programs: Vec<Program>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMode {
    pub name: String,
    #[serde(default)]
    pub is_unconverted: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    // 服务器返回 isEnableTSLiveStream（TS 全大写），camelCase 规则会生成 isEnableTsLiveStream
    #[serde(default, rename = "isEnableTSLiveStream")]
    pub is_enable_ts_live_stream: Option<bool>,
    #[serde(default)]
    pub stream_config: Option<StreamConfig>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct StreamConfig {
    pub live: Option<LiveStreamConfig>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LiveStreamConfig {
    pub ts: Option<LiveTsConfig>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct LiveTsConfig {
    #[serde(default)]
    pub m2ts: Vec<StreamMode>,
}

impl ServerConfig {
    pub fn live_modes(&self) -> Vec<StreamMode> {
        self.stream_config
            .as_ref()
            .and_then(|s| s.live.as_ref())
            .and_then(|l| l.ts.as_ref())
            .map(|t| t.m2ts.clone())
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoFile {
    pub id: u64,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedItem {
    #[serde(default)]
    pub channel_id: Option<u64>,
    pub start_at: i64,
    pub end_at: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_recording: bool,
    #[serde(default)]
    pub thumbnails: Vec<u64>,
    #[serde(default)]
    pub video_files: Vec<VideoFile>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RecordedPage {
    pub records: Vec<RecordedItem>,
    pub total: u64,
}

/// 节目表里的节目（比直播用的 Program 多了类型和详细说明）
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleProgram {
    pub id: u64,
    pub start_at: i64,
    pub end_at: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub extended: Option<String>,
    #[serde(default)]
    pub genre1: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Schedule {
    pub channel: Channel,
    #[serde(default)]
    pub programs: Vec<ScheduleProgram>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReserveListItem {
    pub reserve_id: u64,
    #[serde(default)]
    pub program_id: Option<u64>,
    #[serde(default)]
    pub rule_id: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReserveLists {
    #[serde(default)]
    pub normal: Vec<ReserveListItem>,
    #[serde(default)]
    pub conflicts: Vec<ReserveListItem>,
    #[serde(default)]
    pub skips: Vec<ReserveListItem>,
    #[serde(default)]
    pub overlaps: Vec<ReserveListItem>,
}

#[derive(Clone)]
pub struct Client {
    base: String,
    server: Server,
    agent: ureq::Agent,
}

impl Client {
    pub fn new(server: &Server) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(15)))
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            base: server.url.trim_end_matches('/').to_string(),
            server: server.clone(),
            agent,
        }
    }

    fn auth_header(&self) -> Option<String> {
        if self.server.username.is_empty() {
            return None;
        }
        let raw = format!("{}:{}", self.server.username, self.server.password);
        Some(format!("Basic {}", base64(raw.as_bytes())))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn finish(path: &str, res: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<String> {
        let mut res = res?;
        let status = res.status();
        let body = res.body_mut().read_to_string()?;
        if !status.is_success() {
            bail!("{path}: HTTP {status}: {}", body.chars().take(200).collect::<String>());
        }
        Ok(body)
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let mut req = self.agent.get(self.url(path));
        if let Some(h) = self.auth_header() {
            req = req.header("Authorization", h);
        }
        Ok(serde_json::from_str(&Self::finish(path, req.call())?)?)
    }

    fn post_json(&self, path: &str, body: &serde_json::Value) -> Result<String> {
        let mut req = self.agent.post(self.url(path)).content_type("application/json");
        if let Some(h) = self.auth_header() {
            req = req.header("Authorization", h);
        }
        Self::finish(path, req.send(body.to_string()))
    }

    fn delete(&self, path: &str) -> Result<()> {
        let mut req = self.agent.delete(self.url(path));
        if let Some(h) = self.auth_header() {
            req = req.header("Authorization", h);
        }
        Self::finish(path, req.call()).map(drop)
    }

    pub fn config(&self) -> Result<ServerConfig> {
        self.get("/api/config")
    }

    pub fn version(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct V {
            version: String,
        }
        Ok(self.get::<V>("/api/version")?.version)
    }

    pub fn channels(&self) -> Result<Vec<Channel>> {
        self.get("/api/channels")
    }

    pub fn broadcasting(&self) -> Result<Vec<Broadcasting>> {
        self.get("/api/schedules/broadcasting?isHalfWidth=true")
    }

    pub fn recorded(&self, offset: u64, limit: u64, keyword: &str) -> Result<RecordedPage> {
        let mut q = format!("/api/recorded?isHalfWidth=true&offset={offset}&limit={limit}");
        if !keyword.is_empty() {
            q.push_str("&keyword=");
            q.push_str(&urlencode(keyword));
        }
        self.get(&q)
    }

    /// channel_type 为 GR/BS/CS/SKY 之一
    pub fn schedules(&self, start_at: i64, end_at: i64, channel_type: &str) -> Result<Vec<Schedule>> {
        let flag = |t: &str| t == channel_type;
        self.get(&format!(
            "/api/schedules?startAt={start_at}&endAt={end_at}&isHalfWidth=true&GR={}&BS={}&CS={}&SKY={}&BS4K={}",
            flag("GR"),
            flag("BS"),
            flag("CS"),
            flag("SKY"),
            flag("BS4K")
        ))
    }

    pub fn reserve_lists(&self, start_at: i64, end_at: i64) -> Result<ReserveLists> {
        self.get(&format!("/api/reserves/lists?startAt={start_at}&endAt={end_at}"))
    }

    /// 手动预约（允许末尾缺失，与 EPGStation 网页的默认值相同）
    pub fn add_reserve(&self, program_id: u64) -> Result<()> {
        self.post_json("/api/reserves", &serde_json::json!({ "programId": program_id, "allowEndLack": true }))
            .map(drop)
    }

    pub fn delete_reserve(&self, reserve_id: u64) -> Result<()> {
        self.delete(&format!("/api/reserves/{reserve_id}"))
    }

    // ---- 给 mpv / 图片加载用的 URL ----

    /// 给 mpv 用：Basic 认证放进 userinfo
    fn media_base(&self) -> String {
        if self.server.username.is_empty() {
            return self.base.clone();
        }
        match self.base.split_once("://") {
            Some((scheme, rest)) => format!(
                "{scheme}://{}:{}@{rest}",
                urlencode(&self.server.username),
                urlencode(&self.server.password)
            ),
            None => self.base.clone(),
        }
    }

    pub fn live_url(&self, channel_id: u64, mode: usize) -> String {
        format!("{}/api/streams/live/{channel_id}/m2ts?mode={mode}", self.media_base())
    }

    pub fn video_url(&self, video_file_id: u64) -> String {
        format!("{}/api/videos/{video_file_id}", self.media_base())
    }

    pub fn logo_url(&self, channel_id: u64) -> String {
        format!("{}/api/channels/{channel_id}/logo", self.base)
    }

    pub fn thumbnail_url(&self, thumbnail_id: u64) -> String {
        format!("{}/api/thumbnails/{thumbnail_id}", self.base)
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
    }

    #[test]
    fn urlencode_escapes_reserved_and_utf8() {
        assert_eq!(urlencode("a b/c"), "a%20b%2Fc");
        assert_eq!(urlencode("ニュース"), "%E3%83%8B%E3%83%A5%E3%83%BC%E3%82%B9");
    }

    #[test]
    fn server_config_reads_ts_flag() {
        let c: ServerConfig = serde_json::from_str(r#"{"isEnableTSLiveStream":true}"#).unwrap();
        assert_eq!(c.is_enable_ts_live_stream, Some(true));
        let c: ServerConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c.is_enable_ts_live_stream, None);
    }

    /// 实际从 EPGStation 2.10 取得的响应
    #[test]
    fn parses_real_schedule_and_reserve_responses() {
        let s: Vec<Schedule> = serde_json::from_str(include_str!("../tests/fixtures/schedules.json")).unwrap();
        assert!(!s.is_empty() && !s[0].programs.is_empty());
        assert!(s[0].programs.iter().all(|p| p.end_at > p.start_at));
        let _: ReserveLists = serde_json::from_str(include_str!("../tests/fixtures/reserve_lists.json")).unwrap();
    }

    #[test]
    fn media_url_embeds_basic_auth() {
        let c = Client::new(&Server {
            name: "x".into(),
            url: "http://h:8888/".into(),
            username: "u".into(),
            password: "p@ss".into(),
        });
        assert_eq!(c.live_url(1, 2), "http://u:p%40ss@h:8888/api/streams/live/1/m2ts?mode=2");
        assert_eq!(c.logo_url(1), "http://h:8888/api/channels/1/logo");
    }
}
