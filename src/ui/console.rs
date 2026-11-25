use super::RodonApp;
use crate::logging::{self, Level};
use eframe::egui::{self, RichText};

impl RodonApp {
    pub(super) fn page_console(&self, ui: &mut egui::Ui) {
        let lines = logging::recent(500);
        ui.horizontal(|ui| {
            ui.heading("Console");
            if ui.button("Copy").clicked() {
                ui.ctx().copy_text(
                    lines.iter().map(|l| format!("{} [{:?}] {}", l.at.format("%H:%M:%S"), l.level, l.text)).collect::<Vec<_>>().join("\n"),
                );
            }
            if ui.button("Clear view").clicked() {
                logging::clear_view();
            }
        });
        if let Some(error) = &self.last_error {
            ui.horizontal(|ui| {
                if ui.button("Copy last error").clicked() {
                    ui.ctx().copy_text(error.clone());
                }
                if ui.button("Copy session log").clicked()
                    && let Some(path) = logging::session_log_path()
                    && let Ok(text) = std::fs::read_to_string(path)
                {
                    ui.ctx().copy_text(text);
                }
            });
        }
        egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            for l in &lines {
                let color = match l.level {
                    Level::Info => self.palette.text,
                    Level::Warn => self.palette.warn,
                    Level::Error => self.palette.danger,
                };
                ui.label(RichText::new(format!("{}  {}", l.at.format("%H:%M:%S"), l.text)).monospace().color(color));
            }
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
}
