#![cfg_attr(windows, windows_subsystem = "windows")]

mod api;
mod app;
mod config;
mod epg;
mod fonts;
mod i18n;
mod player;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("EPGPlayer For Desktop")
            .with_app_id("epgplayer-desktop")
            .with_inner_size([960.0, 720.0])
            .with_min_inner_size([480.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native("EPGPlayer For Desktop", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
