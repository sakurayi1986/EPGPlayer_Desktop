//! 节目表（番組表）：横轴频道、纵轴时间的网格。
//!
//! 一天按日本电视习惯从 04:00 开始。频道表头和时间列固定，只绘制可见的节目。

use std::collections::HashMap;

use chrono::{Duration as ChronoDuration, Local, NaiveTime, TimeZone};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::api::{Channel, Client, ReserveLists, Schedule, ScheduleProgram};
use crate::i18n::{tr, Lang};
use crate::trf;

const TIME_W: f32 = 36.0;
const HEADER_H: f32 = 44.0;
const COL_W: f32 = 150.0;
const DAY_START_HOUR: u32 = 4;
const MINUTE: i64 = 60_000;

#[derive(Clone, Copy, PartialEq)]
pub enum Mark {
    Reserved { reserve_id: u64, by_rule: bool },
    Conflict { reserve_id: u64 },
    Skipped,
}

pub enum Action {
    Load { start_at: i64, end_at: i64, channel_type: String },
    Play { channel_id: u64, title: String },
    Reserve { program_id: u64 },
    CancelReserve { reserve_id: u64 },
}

struct Selected {
    program: ScheduleProgram,
    channel: Channel,
}

pub struct Epg {
    pub channel_type: String,
    day_start: i64,
    /// (day_start, channel_type) -> 数据；与当前选择不一致时视为过期
    data: Option<((i64, String), Vec<Schedule>)>,
    marks: HashMap<u64, Mark>,
    loading: Option<(i64, String)>,
    selected: Option<Selected>,
    scroll_to_now: bool,
    px_per_min: f32,
}

impl Default for Epg {
    fn default() -> Self {
        Self {
            channel_type: "GR".into(),
            day_start: today_start(),
            data: None,
            marks: HashMap::new(),
            loading: None,
            selected: None,
            scroll_to_now: true,
            px_per_min: 3.0,
        }
    }
}

fn day_start_of(ms: i64) -> i64 {
    let t = Local.timestamp_millis_opt(ms).single().unwrap_or_else(Local::now);
    let date = if t.time() < NaiveTime::from_hms_opt(DAY_START_HOUR, 0, 0).unwrap() {
        t.date_naive() - ChronoDuration::days(1)
    } else {
        t.date_naive()
    };
    let start = date.and_hms_opt(DAY_START_HOUR, 0, 0).unwrap();
    Local.from_local_datetime(&start).earliest().map(|d| d.timestamp_millis()).unwrap_or(ms)
}

fn today_start() -> i64 {
    day_start_of(Local::now().timestamp_millis())
}

fn shift_days(day_start: i64, days: i64) -> i64 {
    // 加 12 小时再取当天 04:00，避开夏令时一类的偏差
    day_start_of(day_start + days * 24 * 60 * MINUTE + 12 * 60 * MINUTE)
}

fn fmt(ms: i64, f: &str) -> String {
    Local.timestamp_millis_opt(ms).single().map(|t| t.format(f).to_string()).unwrap_or_default()
}

fn weekday(ms: i64) -> &'static str {
    use chrono::Datelike;
    let w = Local.timestamp_millis_opt(ms).single().map(|t| t.weekday().num_days_from_sunday()).unwrap_or(0);
    let names = match crate::i18n::current() {
        Lang::Ja => ["日", "月", "火", "水", "木", "金", "土"],
        Lang::Zh => ["日", "一", "二", "三", "四", "五", "六"],
        Lang::En => ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    };
    names[w as usize]
}

/// ARIB 大分类 → 底色
fn genre_color(genre: Option<u32>, dark: bool) -> Color32 {
    let (r, g, b) = match genre {
        Some(0) => (0xd8, 0xe8, 0xff),  // 新闻
        Some(1) => (0xd9, 0xf2, 0xd0),  // 体育
        Some(2) => (0xff, 0xf1, 0xc9),  // 信息/综合
        Some(3) => (0xff, 0xd9, 0xd9),  // 电视剧
        Some(4) => (0xf6, 0xdc, 0xff),  // 音乐
        Some(5) => (0xff, 0xe4, 0xc8),  // 综艺
        Some(6) => (0xe6, 0xdc, 0xcd),  // 电影
        Some(7) => (0xff, 0xdc, 0xee),  // 动画
        Some(8) => (0xd6, 0xf0, 0xec),  // 纪录片
        Some(9) => (0xec, 0xe4, 0xd4),  // 剧场
        Some(10) => (0xe2, 0xf3, 0xd6), // 兴趣/教育
        Some(11) => (0xe4, 0xe8, 0xf0), // 福祉
        _ => (0xec, 0xec, 0xec),
    };
    if dark {
        // 暗色主题：同色相，压暗
        Color32::from_rgb(r / 3 + 10, g / 3 + 10, b / 3 + 12)
    } else {
        Color32::from_rgb(r, g, b)
    }
}

impl Epg {
    pub fn set_data(&mut self, key: (i64, String), schedules: Vec<Schedule>, reserves: ReserveLists) {
        if self.loading.as_ref() == Some(&key) {
            self.loading = None;
        }
        let mut marks = HashMap::new();
        for r in &reserves.skips {
            if let Some(p) = r.program_id {
                marks.insert(p, Mark::Skipped);
            }
        }
        for r in reserves.normal.iter().chain(&reserves.overlaps) {
            if let Some(p) = r.program_id {
                marks.insert(p, Mark::Reserved { reserve_id: r.reserve_id, by_rule: r.rule_id.is_some() });
            }
        }
        for r in &reserves.conflicts {
            if let Some(p) = r.program_id {
                marks.insert(p, Mark::Conflict { reserve_id: r.reserve_id });
            }
        }
        self.marks = marks;
        self.data = Some((key, schedules));
    }

    pub fn load_failed(&mut self) {
        self.loading = None;
    }

    /// 切换服务器后清空
    pub fn reset(&mut self) {
        self.data = None;
        self.marks.clear();
        self.loading = None;
        self.selected = None;
    }

    /// 预约/取消后重新读取当前这一页
    pub fn invalidate(&mut self) {
        self.data = None;
    }

    fn key(&self) -> (i64, String) {
        (self.day_start, self.channel_type.clone())
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, client: &Client, channel_types: &[&str], actions: &mut Vec<Action>) {
        // ---- 工具栏 ----
        ui.horizontal_wrapped(|ui| {
            for t in channel_types {
                ui.selectable_value(&mut self.channel_type, t.to_string(), *t);
            }
            ui.separator();
            if ui.button("◀").on_hover_text(tr("前日", "前一天", "Previous day")).clicked() {
                self.day_start = shift_days(self.day_start, -1);
            }
            ui.strong(format!("{} ({})", fmt(self.day_start, "%m/%d"), weekday(self.day_start)));
            if ui.button("▶").on_hover_text(tr("翌日", "后一天", "Next day")).clicked() {
                self.day_start = shift_days(self.day_start, 1);
            }
            if ui.button(tr("現在", "现在", "Now")).clicked() {
                self.day_start = today_start();
                self.scroll_to_now = true;
            }
            ui.separator();
            ui.label(tr("拡大", "缩放", "Zoom"));
            ui.add(egui::Slider::new(&mut self.px_per_min, 1.5..=6.0).show_value(false));
            if self.loading.is_some() {
                ui.spinner();
            }
        });
        ui.separator();

        let key = self.key();
        let fresh = self.data.as_ref().is_some_and(|(k, _)| *k == key);
        if !fresh && self.loading.as_ref() != Some(&key) {
            self.loading = Some(key.clone());
            actions.push(Action::Load {
                start_at: self.day_start,
                end_at: self.day_start + 24 * 60 * MINUTE,
                channel_type: self.channel_type.clone(),
            });
        }
        let Some((data_key, schedules)) = &self.data else {
            return;
        };
        if *data_key != key {
            // 新数据到来前继续显示旧数据会对不上日期/类型，干脆留空
            return;
        }

        let now = Local::now().timestamp_millis();
        let day_start = self.day_start;
        let day_end = day_start + 24 * 60 * MINUTE;
        let ppm = self.px_per_min;
        let content = Vec2::new(TIME_W + schedules.len() as f32 * COL_W, HEADER_H + 24.0 * 60.0 * ppm);

        let mut area = egui::ScrollArea::both().auto_shrink(false);
        if self.scroll_to_now {
            self.scroll_to_now = false;
            if (day_start..day_end).contains(&now) {
                let y = ((now - day_start) / MINUTE) as f32 * ppm - 60.0 * ppm;
                area = area.vertical_scroll_offset(y.max(0.0));
            }
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(60));

        let mut clicked: Option<(usize, usize)> = None;
        area.show_viewport(ui, |ui, _viewport| {
            let (origin_rect, resp) = ui.allocate_exact_size(content, Sense::click());
            let origin = origin_rect.min;
            let clip = ui.clip_rect();
            let painter = ui.painter_at(clip);
            let visuals = ui.visuals().clone();
            let dark = visuals.dark_mode;
            let text_color = visuals.text_color();
            let weak = visuals.weak_text_color();
            let grid_stroke = Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color);
            let body_top = origin.y + HEADER_H;
            let y_of = |ms: i64| body_top + ((ms - day_start) as f32 / MINUTE as f32) * ppm;

            // ---- 节目 ----
            let hover = resp.hover_pos();
            let mut hovered: Option<(usize, usize)> = None;
            for (ci, sch) in schedules.iter().enumerate() {
                let x0 = origin.x + TIME_W + ci as f32 * COL_W;
                if x0 > clip.right() || x0 + COL_W < clip.left() {
                    continue;
                }
                for (pi, p) in sch.programs.iter().enumerate() {
                    let (s, e) = (p.start_at.max(day_start), p.end_at.min(day_end));
                    if e <= s {
                        continue;
                    }
                    let rect = Rect::from_min_max(Pos2::new(x0, y_of(s)), Pos2::new(x0 + COL_W, y_of(e)));
                    if !rect.intersects(clip) {
                        continue;
                    }
                    let mark = self.marks.get(&p.id).copied();
                    let is_hover = hover.is_some_and(|h| rect.contains(h) && h.y > clip.top() + HEADER_H && h.x > clip.left() + TIME_W);
                    if is_hover {
                        hovered = Some((ci, pi));
                    }
                    let mut fill = genre_color(p.genre1, dark);
                    if p.end_at <= now {
                        fill = fill.gamma_multiply(0.55);
                    }
                    painter.rect_filled(rect, 0.0, fill);
                    painter.rect_stroke(rect, 0.0, grid_stroke, StrokeKind::Inside);
                    let border = match mark {
                        Some(Mark::Reserved { by_rule: false, .. }) => Some(Color32::from_rgb(0xe0, 0x40, 0x40)),
                        Some(Mark::Reserved { by_rule: true, .. }) => Some(Color32::from_rgb(0xe0, 0x8a, 0x20)),
                        Some(Mark::Conflict { .. }) => Some(Color32::from_rgb(0xb0, 0x00, 0xb0)),
                        Some(Mark::Skipped) | None => None,
                    };
                    if let Some(c) = border {
                        painter.rect_stroke(rect.shrink(1.0), 0.0, Stroke::new(2.0, c), StrokeKind::Inside);
                    }
                    if is_hover {
                        painter.rect_stroke(rect, 0.0, visuals.selection.stroke, StrokeKind::Inside);
                    }

                    let inner = rect.shrink2(Vec2::new(4.0, 2.0));
                    if inner.height() < 8.0 {
                        continue;
                    }
                    let badge = match mark {
                        Some(Mark::Reserved { by_rule: false, .. }) => "● ",
                        Some(Mark::Reserved { by_rule: true, .. }) => "◆ ",
                        Some(Mark::Conflict { .. }) => "⚠ ",
                        _ => "",
                    };
                    let mut text = format!("{} {badge}{}", fmt(p.start_at, "%M"), p.name);
                    if inner.height() > 60.0 {
                        if let Some(d) = p.description.as_deref().filter(|d| !d.is_empty()) {
                            text.push('\n');
                            text.push_str(d);
                        }
                    }
                    let galley = painter.layout(text, FontId::proportional(12.0), text_color, inner.width());
                    painter.with_clip_rect(inner.intersect(clip)).galley(inner.min, galley, text_color);
                }
            }

            // 当前时间线
            if (day_start..day_end).contains(&now) {
                let y = y_of(now);
                painter.hline(origin.x + TIME_W..=origin.x + content.x, y, Stroke::new(2.0, Color32::RED));
            }

            // ---- 固定的时间列 ----
            let time_col = Rect::from_min_max(Pos2::new(clip.left(), body_top.max(clip.top())), Pos2::new(clip.left() + TIME_W, clip.bottom()));
            painter.rect_filled(time_col, 0.0, visuals.panel_fill);
            for h in 0..24 {
                let ms = day_start + h * 60 * MINUTE;
                let y = y_of(ms);
                let hour_rect = Rect::from_min_max(Pos2::new(clip.left(), y), Pos2::new(clip.left() + TIME_W, y + 60.0 * ppm));
                if !hour_rect.intersects(clip) {
                    continue;
                }
                painter.rect_stroke(hour_rect, 0.0, grid_stroke, StrokeKind::Inside);
                painter.text(
                    Pos2::new(hour_rect.center().x, y.max(clip.top() + HEADER_H) + 4.0),
                    Align2::CENTER_TOP,
                    fmt(ms, "%-H"),
                    FontId::proportional(14.0),
                    text_color,
                );
            }

            // ---- 固定的频道表头 ----
            let header = Rect::from_min_max(clip.min, Pos2::new(clip.right(), clip.top() + HEADER_H));
            painter.rect_filled(header, 0.0, visuals.panel_fill);
            for (ci, sch) in schedules.iter().enumerate() {
                let x0 = origin.x + TIME_W + ci as f32 * COL_W;
                let cell = Rect::from_min_size(Pos2::new(x0, clip.top()), Vec2::new(COL_W, HEADER_H));
                if !cell.intersects(clip) || cell.right() < clip.left() + TIME_W {
                    continue;
                }
                let hp = painter.with_clip_rect(cell.intersect(Rect::from_min_max(Pos2::new(clip.left() + TIME_W, clip.top()), clip.max)));
                hp.rect_stroke(cell, 0.0, grid_stroke, StrokeKind::Inside);
                let mut text_x = cell.left() + 4.0;
                if sch.channel.has_logo_data {
                    let logo = Rect::from_min_size(Pos2::new(cell.left() + 4.0, cell.center().y - 9.0), Vec2::new(32.0, 18.0));
                    if logo.left() >= clip.left() + TIME_W {
                        egui::Image::new(client.logo_url(sch.channel.id)).paint_at(ui, logo);
                    }
                    text_x = logo.right() + 4.0;
                }
                let name = sch.channel.half_width_name.as_deref().unwrap_or(&sch.channel.name);
                let galley = hp.layout(name.to_string(), FontId::proportional(12.0), text_color, cell.right() - text_x - 2.0);
                let y = cell.center().y - galley.size().y / 2.0;
                hp.galley(Pos2::new(text_x, y), galley, text_color);
            }
            let corner = Rect::from_min_size(clip.min, Vec2::new(TIME_W, HEADER_H));
            painter.rect_filled(corner, 0.0, visuals.panel_fill);
            painter.text(corner.center(), Align2::CENTER_CENTER, weekday(day_start), FontId::proportional(13.0), weak);

            // ---- 悬停与点击 ----
            if let Some((ci, pi)) = hovered {
                let p = &schedules[ci].programs[pi];
                let resp = resp.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                resp.on_hover_ui_at_pointer(|ui| {
                    ui.set_max_width(320.0);
                    ui.strong(&p.name);
                    ui.weak(format!("{} – {}", fmt(p.start_at, "%H:%M"), fmt(p.end_at, "%H:%M")));
                    if let Some(d) = p.description.as_deref().filter(|d| !d.is_empty()) {
                        ui.label(d);
                    }
                });
            }
            if resp.clicked() {
                clicked = hovered;
            }
        });

        if let Some((ci, pi)) = clicked {
            let sch = &schedules[ci];
            self.selected = Some(Selected { program: sch.programs[pi].clone(), channel: sch.channel.clone() });
        }
        self.detail_window(ui.ctx(), now, actions);
    }

    fn detail_window(&mut self, ctx: &egui::Context, now: i64, actions: &mut Vec<Action>) {
        let Some(sel) = &self.selected else { return };
        let mut open = true;
        let mut close = false;
        let p = &sel.program;
        let ch = &sel.channel;
        let mark = self.marks.get(&p.id).copied();
        egui::Window::new(tr("番組詳細", "节目详情", "Program details"))
            .id(egui::Id::unique("epg_detail"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.heading(&p.name);
                let name = ch.half_width_name.as_deref().unwrap_or(&ch.name);
                let (day, start, end) = (fmt(p.start_at, "%m/%d"), fmt(p.start_at, "%H:%M"), fmt(p.end_at, "%H:%M"));
                let min = (p.end_at - p.start_at) / MINUTE;
                ui.label(trf!(
                    "{}  {} {} – {}（{} 分）",
                    "{}  {} {} – {}（{} 分钟）",
                    "{}  {} {} – {} ({} min)",
                    name, day, start, end, min
                ));
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    if let Some(d) = p.description.as_deref().filter(|d| !d.is_empty()) {
                        ui.label(d);
                    }
                    if let Some(x) = p.extended.as_deref().filter(|x| !x.is_empty()) {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(x).small());
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let on_air = (p.start_at..p.end_at).contains(&now);
                    if on_air && ui.button(tr("▶ ライブ視聴", "▶ 观看直播", "▶ Watch live")).clicked() {
                        actions.push(Action::Play {
                            channel_id: ch.id,
                            title: format!("{} - {}", ch.name, p.name),
                        });
                    }
                    let future = p.end_at > now;
                    match mark {
                        None if future => {
                            if ui.button(tr("⏺ 録画予約", "⏺ 预约录制", "⏺ Record")).clicked() {
                                actions.push(Action::Reserve { program_id: p.id });
                                close = true;
                            }
                        }
                        Some(Mark::Reserved { reserve_id, by_rule: false }) | Some(Mark::Conflict { reserve_id }) => {
                            if matches!(mark, Some(Mark::Conflict { .. })) {
                                ui.colored_label(Color32::from_rgb(0xb0, 0x00, 0xb0), tr("⚠ 予約が競合しています", "⚠ 预约冲突", "⚠ Reservation conflict"));
                            } else {
                                ui.colored_label(Color32::from_rgb(0xe0, 0x40, 0x40), tr("● 予約済み", "● 已预约", "● Reserved"));
                            }
                            if future && ui.button(tr("予約を取り消す", "取消预约", "Cancel reservation")).clicked() {
                                actions.push(Action::CancelReserve { reserve_id });
                                close = true;
                            }
                        }
                        Some(Mark::Reserved { by_rule: true, .. }) => {
                            ui.colored_label(Color32::from_rgb(0xe0, 0x8a, 0x20), tr(
                                "◆ ルール予約（EPGStation の Web 画面で変更してください）",
                                "◆ 规则预约（请在 EPGStation 网页修改）",
                                "◆ Rule-based reservation (edit it in the EPGStation web UI)",
                            ));
                        }
                        Some(Mark::Skipped) => {
                            ui.weak(tr("ルール予約から除外されています", "已从规则预约中排除", "Excluded from the rule"));
                        }
                        None => {}
                    }
                });
            });
        if !open || close {
            self.selected = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_ms(y: i32, m: u32, d: u32, h: u32, min: u32) -> i64 {
        Local.with_ymd_and_hms(y, m, d, h, min, 0).single().unwrap().timestamp_millis()
    }

    #[test]
    fn day_starts_at_4am() {
        assert_eq!(day_start_of(local_ms(2026, 10, 18, 10, 0)), local_ms(2026, 10, 18, 4, 0));
        assert_eq!(day_start_of(local_ms(2026, 10, 18, 4, 0)), local_ms(2026, 10, 18, 4, 0));
        // 凌晨 2 点属于前一天的节目表
        assert_eq!(day_start_of(local_ms(2026, 10, 18, 2, 0)), local_ms(2026, 10, 17, 4, 0));
    }

    #[test]
    fn shift_days_keeps_4am() {
        let d = local_ms(2026, 12, 31, 4, 0);
        assert_eq!(shift_days(d, 1), local_ms(2027, 1, 1, 4, 0));
        assert_eq!(shift_days(d, -1), local_ms(2026, 12, 30, 4, 0));
    }

    #[test]
    fn reserve_marks() {
        let mut e = Epg::default();
        let lists: ReserveLists = serde_json::from_str(
            r#"{"normal":[{"reserveId":1,"programId":10},{"reserveId":2,"programId":20,"ruleId":5}],
                "conflicts":[{"reserveId":3,"programId":30}],"skips":[{"reserveId":4,"programId":40,"ruleId":5}],"overlaps":[]}"#,
        )
        .unwrap();
        e.set_data((0, "GR".into()), vec![], lists);
        assert!(e.marks[&10] == Mark::Reserved { reserve_id: 1, by_rule: false });
        assert!(e.marks[&20] == Mark::Reserved { reserve_id: 2, by_rule: true });
        assert!(e.marks[&30] == Mark::Conflict { reserve_id: 3 });
        assert!(e.marks[&40] == Mark::Skipped);
    }
}
