use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{Local, TimeZone};
use eframe::egui;

use crate::api::{Broadcasting, Channel, Client, RecordedPage, ServerConfig, StreamMode};
use crate::config::{Config, Server};
use crate::i18n::{tr, Lang};
use crate::player::Player;
use crate::trf;

const CHANNEL_TYPES: [&str; 4] = ["GR", "BS", "CS", "SKY"];
const RECORDED_PAGE: u64 = 50;
const BROADCASTING_REFRESH: Duration = Duration::from_secs(60);

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Live,
    Epg,
    Recorded,
    Settings,
}

enum Msg {
    Server(ServerConfig, Vec<Channel>),
    Broadcasting(Vec<Broadcasting>),
    Recorded(RecordedPage),
    Schedule((i64, String), Vec<crate::api::Schedule>, crate::api::ReserveLists),
    Notice(String),
    Error(String),
}

pub struct App {
    config: Config,
    tab: Tab,
    player: Player,
    epg: crate::epg::Epg,
    notice: Option<String>,

    // 当前服务器的数据。epoch 在切换服务器时递增，用来丢弃过时的响应
    epoch: u64,
    tx: Sender<(u64, Msg)>,
    rx: Receiver<(u64, Msg)>,
    client: Option<Client>,
    server_config: Option<ServerConfig>,
    channels: HashMap<u64, Channel>,
    broadcasting: Vec<Broadcasting>,
    broadcasting_at: Option<Instant>,
    channel_type: String,
    recorded: Option<RecordedPage>,
    recorded_offset: u64,
    keyword: String,
    loading: usize,
    error: Option<String>,

    // 设置页的编辑副本
    edit_servers: Vec<Server>,
    settings_note: Option<String>,
    // 连接测试：(测试时的服务器内容, 结果)。内容改了就不再显示旧结果
    test_tx: Sender<(Server, Result<String, String>)>,
    test_rx: Receiver<(Server, Result<String, String>)>,
    tests: Vec<(Server, Option<Result<String, String>>)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = Config::load();
        let lang = config.language.unwrap_or_else(Lang::detect);
        crate::i18n::set(lang);
        crate::fonts::install(&cc.egui_ctx, lang);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let (test_tx, test_rx) = mpsc::channel();
        let tab = if config.servers.is_empty() { Tab::Settings } else { Tab::Live };
        let mut app = Self {
            edit_servers: config.servers.clone(),
            config,
            tab,
            player: Player::default(),
            epg: crate::epg::Epg::default(),
            notice: None,
            epoch: 0,
            tx,
            rx,
            client: None,
            server_config: None,
            channels: HashMap::new(),
            broadcasting: Vec::new(),
            broadcasting_at: None,
            channel_type: "GR".into(),
            recorded: None,
            recorded_offset: 0,
            keyword: String::new(),
            loading: 0,
            error: None,
            settings_note: None,
            test_tx,
            test_rx,
            tests: Vec::new(),
        };
        app.connect(&cc.egui_ctx);
        app
    }

    // ---- 后台请求 ----

    fn spawn(&mut self, ctx: &egui::Context, job: impl FnOnce(&Client) -> anyhow::Result<Msg> + Send + 'static) {
        let Some(client) = self.client.clone() else { return };
        let tx = self.tx.clone();
        let epoch = self.epoch;
        let ctx = ctx.clone();
        self.loading += 1;
        thread::spawn(move || {
            let msg = job(&client).unwrap_or_else(|e| Msg::Error(format!("{e:#}")));
            tx.send((epoch, msg)).ok();
            ctx.request_repaint();
        });
    }

    fn connect(&mut self, ctx: &egui::Context) {
        self.epoch += 1;
        self.loading = 0;
        self.server_config = None;
        self.channels.clear();
        self.broadcasting.clear();
        self.broadcasting_at = None;
        self.recorded = None;
        self.recorded_offset = 0;
        self.error = None;
        self.epg.reset();
        self.client = self.config.server().map(Client::new);
        self.spawn(ctx, |c| Ok(Msg::Server(c.config()?, c.channels()?)));
        self.refresh_broadcasting(ctx);
        self.refresh_recorded(ctx);
    }

    fn refresh_broadcasting(&mut self, ctx: &egui::Context) {
        self.broadcasting_at = Some(Instant::now());
        self.spawn(ctx, |c| Ok(Msg::Broadcasting(c.broadcasting()?)));
    }

    fn refresh_recorded(&mut self, ctx: &egui::Context) {
        let (offset, keyword) = (self.recorded_offset, self.keyword.clone());
        self.spawn(ctx, move |c| Ok(Msg::Recorded(c.recorded(offset, RECORDED_PAGE, &keyword)?)));
    }

    fn test_server(&mut self, ctx: &egui::Context, server: Server) {
        self.tests.retain(|(s, _)| s != &server);
        self.tests.push((server.clone(), None));
        let tx = self.test_tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let r = Client::new(&server).version().map_err(|e| format!("{e:#}"));
            tx.send((server, r)).ok();
            ctx.request_repaint();
        });
    }

    fn receive(&mut self) {
        while let Ok((server, r)) = self.test_rx.try_recv() {
            if let Some(t) = self.tests.iter_mut().find(|(s, _)| s == &server) {
                t.1 = Some(r);
            }
        }
        while let Ok((epoch, msg)) = self.rx.try_recv() {
            if epoch != self.epoch {
                continue;
            }
            self.loading = self.loading.saturating_sub(1);
            match msg {
                Msg::Server(cfg, channels) => {
                    self.channels = channels.into_iter().map(|c| (c.id, c)).collect();
                    if !self.channels.values().any(|c| c.channel_type == self.channel_type) {
                        if let Some(t) = CHANNEL_TYPES
                            .iter()
                            .find(|t| self.channels.values().any(|c| c.channel_type == **t))
                        {
                            self.channel_type = (*t).into();
                        }
                    }
                    if let Some(t) = CHANNEL_TYPES
                        .iter()
                        .find(|t| self.channels.values().any(|c| c.channel_type == **t))
                        .filter(|_| !self.channels.values().any(|c| c.channel_type == self.epg.channel_type))
                    {
                        self.epg.channel_type = (*t).into();
                    }
                    self.server_config = Some(cfg);
                }
                Msg::Broadcasting(b) => self.broadcasting = b,
                Msg::Recorded(r) => self.recorded = Some(r),
                Msg::Schedule(key, schedules, reserves) => self.epg.set_data(key, schedules, reserves),
                Msg::Notice(n) => {
                    self.notice = Some(n);
                    self.epg.invalidate();
                }
                Msg::Error(e) => {
                    self.epg.load_failed();
                    self.error = Some(e);
                }
            }
        }
    }

    // ---- 播放 ----

    fn live_modes(&self) -> Vec<StreamMode> {
        self.server_config.as_ref().map(|c| c.live_modes()).unwrap_or_default()
    }

    /// 选中的直播画质下标。没选过时默认「無変換」
    fn live_mode_index(&self) -> usize {
        let modes = self.live_modes();
        self.config
            .live_mode
            .as_ref()
            .and_then(|n| modes.iter().position(|m| &m.name == n))
            .or_else(|| modes.iter().position(|m| m.is_unconverted))
            .unwrap_or(0)
    }

    fn play(&mut self, ctx: &egui::Context, url: String, title: String) {
        let ctx = ctx.clone();
        self.player.play(url, title, move || ctx.request_repaint());
    }

    // ---- 界面 ----

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            let names: Vec<String> = self.config.servers.iter().map(|s| s.name.clone()).collect();
            let mut selected = self.config.selected;
            egui::ComboBox::from_id_salt("server")
                .selected_text(names.get(selected).cloned().unwrap_or_else(|| tr("（サーバーなし）", "（无服务器）", "(no server)").into()))
                .show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut selected, i, n);
                    }
                });
            if selected != self.config.selected {
                self.config.selected = selected;
                self.config.save().ok();
                self.connect(ui.ctx());
            }

            ui.separator();
            ui.selectable_value(&mut self.tab, Tab::Live, tr("📺 ライブ", "📺 直播", "📺 Live"));
            ui.selectable_value(&mut self.tab, Tab::Epg, tr("📅 番組表", "📅 节目表", "📅 Guide"));
            ui.selectable_value(&mut self.tab, Tab::Recorded, tr("📼 録画", "📼 录像", "📼 Recordings"));
            ui.selectable_value(&mut self.tab, Tab::Settings, tr("⚙ 設定", "⚙ 设置", "⚙ Settings"));

            ui.separator();
            if ui.button("⟳").on_hover_text(tr("再読み込み", "重新加载", "Reload")).clicked() {
                self.connect(ui.ctx());
            }
            if self.loading > 0 {
                ui.spinner();
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let st = self.player.status();
            match (&st.error, st.running, &st.title) {
                (Some(e), _, _) => {
                    ui.colored_label(ui.visuals().error_fg_color, trf!("プレーヤー: {}", "播放器: {}", "Player: {}", e));
                }
                (None, true, Some(t)) => {
                    ui.label(format!("▶ {t}"));
                }
                (None, true, None) => {
                    ui.label(tr("▶ プレーヤーを起動中…", "▶ 正在启动播放器…", "▶ Starting player…"));
                }
                _ => {
                    ui.weak(tr("プレーヤー停止中", "播放器未运行", "Player not running"));
                }
            }
            if let Some(e) = &self.error {
                ui.separator();
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            if let Some(n) = &self.notice {
                ui.separator();
                ui.label(n);
            }
        });
    }

    fn live_tab(&mut self, ui: &mut egui::Ui) {
        if self.client.is_none() {
            ui.label(tr(
                "サーバーがありません。「⚙ 設定」で追加してください。",
                "还没有服务器，请先到「⚙ 设置」添加。",
                "No server yet. Add one in ⚙ Settings.",
            ));
            return;
        }
        if self.broadcasting_at.is_some_and(|t| t.elapsed() > BROADCASTING_REFRESH) {
            self.refresh_broadcasting(ui.ctx());
        }
        ui.ctx().request_repaint_after(Duration::from_secs(30));

        ui.horizontal(|ui| {
            for t in CHANNEL_TYPES {
                let count = self.broadcasting.iter().filter(|b| b.channel.channel_type == t).count();
                if count > 0 {
                    ui.selectable_value(&mut self.channel_type, t.to_string(), format!("{t} ({count})"));
                }
            }
            ui.separator();
            let modes = self.live_modes();
            if !modes.is_empty() {
                let mut idx = self.live_mode_index();
                egui::ComboBox::from_id_salt("live_mode")
                    .selected_text(trf!("画質: {}", "画质: {}", "Quality: {}", modes[idx].name))
                    .show_ui(ui, |ui| {
                        for (i, m) in modes.iter().enumerate() {
                            ui.selectable_value(&mut idx, i, &m.name);
                        }
                    });
                if Some(&modes[idx].name) != self.config.live_mode.as_ref() {
                    self.config.live_mode = Some(modes[idx].name.clone());
                    self.config.save().ok();
                }
            }
            if self.server_config.as_ref().is_some_and(|c| c.is_enable_ts_live_stream == Some(false)) {
                ui.colored_label(ui.visuals().warn_fg_color, tr("サーバーで TS ライブ配信が無効です", "服务器未启用 TS 直播", "TS live streaming is disabled on the server"));
            }
        });
        ui.separator();

        let now = Local::now().timestamp_millis();
        let mode = self.live_mode_index();
        let mut clicked = None;
        let client = self.client.clone();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for b in self.broadcasting.iter().filter(|b| b.channel.channel_type == self.channel_type) {
                let p = b.programs.first();
                let resp = ui
                    .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
                        ui.horizontal(|ui| {
                            let size = egui::vec2(64.0, 36.0);
                            match (&client, b.channel.has_logo_data) {
                                (Some(c), true) => {
                                    ui.add(egui::Image::new(c.logo_url(b.channel.id)).fit_to_exact_size(size));
                                }
                                _ => {
                                    ui.allocate_space(size);
                                }
                            }
                            ui.vertical(|ui| {
                                ui.strong(&b.channel.name);
                                if let Some(p) = p {
                                    ui.label(&p.name);
                                    ui.horizontal(|ui| {
                                        ui.weak(format!("{} – {}", hm(p.start_at), hm(p.end_at)));
                                        let total = (p.end_at - p.start_at).max(1) as f32;
                                        let frac = ((now - p.start_at) as f32 / total).clamp(0.0, 1.0);
                                        ui.add(egui::ProgressBar::new(frac).desired_width(160.0).desired_height(6.0));
                                    });
                                } else {
                                    ui.weak(tr("（番組情報なし）", "（无节目信息）", "(no program info)"));
                                }
                            });
                        });
                    })
                    .response;
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                let resp = match p.and_then(|p| p.description.as_deref()) {
                    Some(d) if !d.is_empty() => resp.on_hover_text(d),
                    _ => resp,
                };
                if resp.hovered() {
                    ui.painter().rect_stroke(
                        resp.rect,
                        4.0,
                        ui.visuals().selection.stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                if resp.clicked() {
                    let title = match p {
                        Some(p) => format!("{} - {}", b.channel.name, p.name),
                        None => b.channel.name.clone(),
                    };
                    clicked = Some((b.channel.id, title));
                }
                ui.separator();
            }
        });
        if let (Some((id, title)), Some(c)) = (clicked, &self.client) {
            let url = c.live_url(id, mode);
            self.play(ui.ctx(), url, title);
        }
    }

    fn epg_tab(&mut self, ui: &mut egui::Ui) {
        let Some(client) = self.client.clone() else {
            ui.label(tr(
                "サーバーがありません。「⚙ 設定」で追加してください。",
                "还没有服务器，请先到「⚙ 设置」添加。",
                "No server yet. Add one in ⚙ Settings.",
            ));
            return;
        };
        let types: Vec<&str> = CHANNEL_TYPES
            .iter()
            .copied()
            .filter(|t| self.channels.values().any(|c| c.channel_type == *t))
            .collect();
        let mut actions = Vec::new();
        self.epg.ui(ui, &client, &types, &mut actions);
        let mode = self.live_mode_index();
        for a in actions {
            match a {
                crate::epg::Action::Load { start_at, end_at, channel_type } => {
                    self.spawn(ui.ctx(), move |c| {
                        let schedules = c.schedules(start_at, end_at, &channel_type)?;
                        let reserves = c.reserve_lists(start_at, end_at)?;
                        Ok(Msg::Schedule((start_at, channel_type), schedules, reserves))
                    });
                }
                crate::epg::Action::Play { channel_id, title } => {
                    let url = client.live_url(channel_id, mode);
                    self.play(ui.ctx(), url, title);
                }
                crate::epg::Action::Reserve { program_id } => {
                    self.spawn(ui.ctx(), move |c| {
                        c.add_reserve(program_id)?;
                        Ok(Msg::Notice(tr("予約しました", "已预约", "Reserved").into()))
                    });
                }
                crate::epg::Action::CancelReserve { reserve_id } => {
                    self.spawn(ui.ctx(), move |c| {
                        c.delete_reserve(reserve_id)?;
                        Ok(Msg::Notice(tr("予約を取り消しました", "已取消预约", "Reservation cancelled").into()))
                    });
                }
            }
        }
    }

    fn recorded_tab(&mut self, ui: &mut egui::Ui) {
        if self.client.is_none() {
            ui.label(tr(
                "サーバーがありません。「⚙ 設定」で追加してください。",
                "还没有服务器，请先到「⚙ 设置」添加。",
                "No server yet. Add one in ⚙ Settings.",
            ));
            return;
        }
        let mut reload = false;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.keyword).hint_text(tr("タイトル検索", "搜索标题", "Search title")).desired_width(240.0));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.recorded_offset = 0;
                reload = true;
            }
            if ui.button("🔍").clicked() {
                self.recorded_offset = 0;
                reload = true;
            }
            if let Some(r) = &self.recorded {
                ui.separator();
                let page = self.recorded_offset / RECORDED_PAGE + 1;
                let pages = r.total.div_ceil(RECORDED_PAGE).max(1);
                if ui.add_enabled(self.recorded_offset > 0, egui::Button::new("◀")).clicked() {
                    self.recorded_offset -= RECORDED_PAGE;
                    reload = true;
                }
                ui.label(trf!("{} / {}（全 {} 件）", "{} / {}（共 {} 条）", "{} / {} ({} total)", page, pages, r.total));
                if ui.add_enabled(page < pages, egui::Button::new("▶")).clicked() {
                    self.recorded_offset += RECORDED_PAGE;
                    reload = true;
                }
            }
        });
        if reload {
            self.refresh_recorded(ui.ctx());
        }
        ui.separator();

        let Some(page) = &self.recorded else { return };
        let mut play = None;
        let client = self.client.clone();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for r in &page.records {
                ui.horizontal(|ui| {
                    let size = egui::vec2(160.0, 90.0);
                    match (&client, r.thumbnails.first()) {
                        (Some(c), Some(t)) => {
                            ui.add(egui::Image::new(c.thumbnail_url(*t)).fit_to_exact_size(size).corner_radius(4.0));
                        }
                        _ => {
                            ui.allocate_space(size);
                        }
                    }
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            if r.is_recording {
                                ui.colored_label(egui::Color32::RED, tr("● 録画中", "● 录制中", "● Recording"));
                            }
                            ui.strong(&r.name);
                        });
                        let ch = r.channel_id.and_then(|id| self.channels.get(&id));
                        let ch = ch.map(|c| c.half_width_name.as_deref().unwrap_or(&c.name)).unwrap_or("");
                        let (start, end, min) = (ymd_hm(r.start_at), hm(r.end_at), (r.end_at - r.start_at) / 60000);
                        ui.weak(trf!("{}  {} – {}（{} 分）", "{}  {} – {}（{} 分钟）", "{}  {} – {} ({} min)", ch, start, end, min));
                        if let Some(d) = &r.description {
                            ui.add(egui::Label::new(egui::RichText::new(d).small()).truncate());
                        }
                        ui.horizontal(|ui| {
                            for v in &r.video_files {
                                let label = format!("▶ {}（{}）", v.name, human_size(v.size));
                                if ui.button(label).on_hover_text(&v.kind).clicked() {
                                    play = Some((v.id, r.name.clone()));
                                }
                            }
                        });
                    });
                });
                ui.separator();
            }
        });
        if let (Some((id, title)), Some(c)) = (play, &self.client) {
            let url = c.video_url(id);
            self.play(ui.ctx(), url, title);
        }
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // 不管当前是哪种语言都看得懂
            ui.strong("言語 / 语言 / Language");
            let mut choice = self.config.language;
            let label = |l: Option<Lang>| match l {
                Some(l) => l.native_name().to_string(),
                None => format!("{}（{}）", tr("自動", "自动", "Auto"), Lang::detect().native_name()),
            };
            egui::ComboBox::from_id_salt("language").selected_text(label(choice)).show_ui(ui, |ui| {
                ui.selectable_value(&mut choice, None, label(None));
                for l in Lang::ALL {
                    ui.selectable_value(&mut choice, Some(l), l.native_name());
                }
            });
            if choice != self.config.language {
                self.config.language = choice;
                let lang = choice.unwrap_or_else(Lang::detect);
                crate::i18n::set(lang);
                crate::fonts::install(ui.ctx(), lang);
                self.settings_note = Some(match self.config.save() {
                    Ok(()) => tr("保存しました", "已保存", "Saved").into(),
                    Err(e) => trf!("保存に失敗しました: {:#}", "保存失败: {:#}", "Save failed: {:#}", e),
                });
            }
        });
        ui.add_space(8.0);
        ui.separator();

        ui.heading(tr("EPGStation サーバー", "EPGStation 服务器", "EPGStation servers"));
        ui.add_space(4.0);
        if self.edit_servers.is_empty() {
            ui.label(tr(
                "サーバーがまだありません。「➕ 追加」を押して EPGStation のアドレス（例: 192.168.0.10:8888）を入力し、接続テストしてから保存してください。",
                "还没有服务器。点「➕ 添加」，填入 EPGStation 的地址（例如 192.168.0.10:8888），测试连接后保存。",
                "No servers yet. Click “➕ Add”, enter the EPGStation address (e.g. 192.168.0.10:8888), test the connection, then save.",
            ));
            ui.add_space(4.0);
        }

        let mut remove = None;
        let mut test = None;
        egui::Grid::new("servers").num_columns(6).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.strong(tr("名前", "名称", "Name"));
            ui.strong(tr("アドレス", "地址", "Address"));
            ui.strong(tr("ユーザー名", "用户名", "Username"));
            ui.strong(tr("パスワード", "密码", "Password"));
            ui.end_row();
            for (i, s) in self.edit_servers.iter_mut().enumerate() {
                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(140.0).hint_text(tr("省略可", "可省略", "optional")));
                ui.add(egui::TextEdit::singleline(&mut s.url).desired_width(240.0).hint_text("192.168.0.10:8888"));
                ui.add(egui::TextEdit::singleline(&mut s.username).desired_width(100.0).hint_text(tr("認証なしは空欄", "无认证时留空", "blank if no auth")));
                ui.add(egui::TextEdit::singleline(&mut s.password).password(true).desired_width(100.0));
                ui.horizontal(|ui| {
                    if ui.button(tr("接続テスト", "测试连接", "Test")).clicked() {
                        test = Some(s.normalized());
                    }
                    if ui.button("🗑").on_hover_text(tr("削除", "删除", "Delete")).clicked() {
                        remove = Some(i);
                    }
                });
                let n = s.normalized();
                match self.tests.iter().find(|(t, _)| t == &n).map(|(_, r)| r) {
                    Some(None) => {
                        ui.spinner();
                    }
                    Some(Some(Ok(v))) => {
                        ui.colored_label(egui::Color32::from_rgb(0x2e, 0xa0, 0x43), format!("✔ EPGStation {v}"));
                    }
                    Some(Some(Err(e))) => {
                        ui.colored_label(ui.visuals().error_fg_color, tr("✖ 接続失敗", "✖ 连接失败", "✖ Failed")).on_hover_text(e);
                    }
                    None => {
                        ui.label("");
                    }
                }
                ui.end_row();
            }
        });
        if let Some(s) = test {
            self.test_server(ui.ctx(), s);
        }
        if let Some(i) = remove {
            self.edit_servers.remove(i);
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(tr("➕ 追加", "➕ 添加", "➕ Add")).clicked() {
                self.edit_servers.push(Server::default());
            }
            let normalized: Vec<Server> = self
                .edit_servers
                .iter()
                .filter(|s| !s.url.trim().is_empty())
                .map(Server::normalized)
                .collect();
            let changed = normalized != self.config.servers || normalized.len() != self.edit_servers.len();
            if ui.add_enabled(changed, egui::Button::new(tr("💾 保存", "💾 保存", "💾 Save"))).clicked() {
                let previous = self.config.server().cloned();
                self.config.servers = normalized.clone();
                self.edit_servers = normalized;
                // 尽量保持原来选中的服务器
                self.config.selected = previous
                    .and_then(|p| self.config.servers.iter().position(|s| s.url == p.url))
                    .unwrap_or(0);
                self.settings_note = Some(match self.config.save() {
                    Ok(()) => tr("保存しました", "已保存", "Saved").into(),
                    Err(e) => trf!("保存に失敗しました: {:#}", "保存失败: {:#}", "Save failed: {:#}", e),
                });
                self.connect(ui.ctx());
            }
            if ui.add_enabled(changed, egui::Button::new(tr("元に戻す", "撤销", "Revert"))).clicked() {
                self.edit_servers = self.config.servers.clone();
            }
            if let Some(n) = &self.settings_note {
                ui.label(n);
            }
        });

        ui.add_space(12.0);
        if let Some(p) = crate::config::path() {
            ui.weak(trf!("設定ファイル: {}", "配置文件: {}", "Config file: {}", p.display()));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive();
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Live => self.live_tab(ui),
            Tab::Epg => self.epg_tab(ui),
            Tab::Recorded => self.recorded_tab(ui),
            Tab::Settings => self.settings_tab(ui),
        });
    }
}

fn local(ms: i64) -> chrono::DateTime<Local> {
    Local.timestamp_millis_opt(ms).single().unwrap_or_default()
}

fn hm(ms: i64) -> String {
    local(ms).format("%H:%M").to_string()
}

fn ymd_hm(ms: i64) -> String {
    local(ms).format("%Y/%m/%d %H:%M").to_string()
}

fn human_size(b: u64) -> String {
    const G: f64 = 1024.0 * 1024.0 * 1024.0;
    if b as f64 >= G {
        format!("{:.1} GB", b as f64 / G)
    } else {
        format!("{:.0} MB", b as f64 / 1024.0 / 1024.0)
    }
}
