//! The Anti-AFK page: configure the keep-alive action and toggle it on.

use crate::ui::RodonApp;
use crate::ui::widgets;
use eframe::egui::{self, RichText};

const ACTIONS: [&str; 17] =
    ["space", "w", "a", "s", "d", "up", "down", "left", "right", "shift", "lmb", "rmb", "mmb", "mback", "mfwd", "scroll_up", "scroll_down"];

impl RodonApp {
    pub(in crate::ui) fn page_anti_afk(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Anti-AFK").size(22.0).strong().color(self.palette.text));
        ui.label(
            RichText::new("Sends an input to each Roblox window on a timer so sessions aren't kicked for inactivity.")
                .size(12.0)
                .color(self.palette.muted),
        );
        ui.add_space(12.0);

        let palette = self.palette;
        let running = self.anti_afk.is_some();
        let mut settings = self.core.settings();
        let mut changed = false;

        widgets::card(ui, &palette, |ui| {
            widgets::setting_row(ui, &palette, "Enabled", "Starts the keep-alive loop for all running clients", |ui| {
                let mut on = running;
                if ui.add(egui::Checkbox::without_text(&mut on)).changed() {
                    if on {
                        self.anti_afk = Some(crate::services::anti_afk::start(settings.anti_afk.clone()));
                        self.show_toast("Anti-AFK started", false);
                    } else {
                        self.anti_afk = None;
                        self.show_toast("Anti-AFK stopped", false);
                    }
                }
            });

            widgets::setting_row(ui, &palette, "Action", "Key pressed in each window", |ui| {
                egui::ComboBox::from_id_salt("afk-action").selected_text(&settings.anti_afk.action).show_ui(ui, |ui| {
                    for action in ACTIONS {
                        if ui.selectable_label(settings.anti_afk.action == action, action).clicked() {
                            settings.anti_afk.action = action.to_owned();
                            changed = true;
                        }
                    }
                });
            });

            if self.recording_action {
                ui.label("Press a key or mouse button here");
                let recorded = ui.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::Key { key, pressed: true, .. } => Some(key.name().to_lowercase().replace("arrow", "")),
                        egui::Event::PointerButton { button, pressed: true, .. } => Some(
                            match button {
                                egui::PointerButton::Primary => "lmb",
                                egui::PointerButton::Secondary => "rmb",
                                egui::PointerButton::Middle => "mmb",
                                egui::PointerButton::Extra1 => "mback",
                                egui::PointerButton::Extra2 => "mfwd",
                            }
                            .into(),
                        ),
                        egui::Event::MouseWheel { delta, .. } => Some(if delta.y < 0.0 { "scroll_down" } else { "scroll_up" }.into()),
                        _ => None,
                    })
                });
                if let Some(action) = recorded {
                    settings.anti_afk.action = action;
                    self.recording_action = false;
                    changed = true;
                }
            } else if ui.button("Record action").clicked() {
                self.recording_action = true;
            }
            if ui.button("Run maintenance now").clicked() {
                let cfg = settings.anti_afk.clone();
                std::thread::spawn(move || crate::services::anti_afk::maintain(&cfg, &std::sync::atomic::AtomicBool::new(false)));
            }
            changed |= ui.checkbox(&mut settings.anti_afk.show_countdown, "Show countdown").changed();
            changed |= ui.add(egui::Slider::new(&mut settings.anti_afk.countdown_size, 10..=40).text("Countdown text size")).changed();
            let text = if let Some(worker) = &self.anti_afk {
                format!("Maintenance in {} seconds", worker.remaining())
            } else {
                "Maintenance in 60 seconds".into()
            };
            ui.label("Preview countdown").on_hover_ui(|ui| {
                ui.label(RichText::new(&text).size(settings.anti_afk.countdown_size as f32));
            });
            if running && settings.anti_afk.show_countdown {
                ui.label(RichText::new(text).size(settings.anti_afk.countdown_size as f32));
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
            }
            widgets::setting_row(ui, &palette, "Presses", "How many times per cycle", |ui| {
                if ui.add(egui::DragValue::new(&mut settings.anti_afk.press_count).range(1..=20)).changed() {
                    changed = true;
                }
            });

            widgets::setting_row(ui, &palette, "Interval", "Minutes between cycles", |ui| {
                if ui.add(egui::DragValue::new(&mut settings.anti_afk.interval_minutes).range(1..=19)).changed() {
                    changed = true;
                }
            });
        });

        if changed {
            let afk = settings.anti_afk.clone();
            if running {
                self.anti_afk = Some(crate::services::anti_afk::start(afk.clone()));
            }
            let _ = self.core.edit_settings(move |s| s.anti_afk = afk);
        }
    }
}
