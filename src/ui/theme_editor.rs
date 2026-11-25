use super::{RodonApp, theme};
use crate::{
    store::settings::Settings,
    themes::{self, PRESETS},
};
use eframe::egui;

impl RodonApp {
    pub(super) fn theme_controls(&mut self, ui: &mut egui::Ui, s: &mut Settings) -> bool {
        let before = s.appearance.clone();
        let mut changed_library = false;
        let current = s.appearance.as_ref().map(|t| t.name.as_str()).unwrap_or("System palette");
        egui::ComboBox::from_label("Color preset").selected_text(current).show_ui(ui, |ui| {
            if ui.selectable_label(s.appearance.is_none(), "System palette").clicked() {
                s.appearance = None;
            }
            for option in PRESETS.iter().chain(&s.saved_themes) {
                if ui.selectable_label(s.appearance.as_ref().is_some_and(|t| t.id == option.id), &option.name).clicked() {
                    let old = s.appearance.as_ref().map(|t| t.background.clone());
                    let mut selected = option.clone();
                    if option.id.starts_with("builtin:")
                        && let Some(bg) = old
                    {
                        selected.background = bg;
                    }
                    s.appearance = Some(selected);
                }
            }
        });
        if ui.button("Import .ramtheme…").clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("RAM theme", &["ramtheme"]).pick_file()
        {
            match themes::import(&path, self.core.data_dir()) {
                Ok(t) => {
                    s.saved_themes.push(t.clone());
                    s.appearance = Some(t);
                    changed_library = true;
                }
                Err(e) => self.show_toast(e.to_string(), true),
            }
        }
        let mut delete = false;
        if let Some(t) = &mut s.appearance {
            ui.collapsing("Customize colors and background", |ui| {
                let mut edit = None;
                egui::Grid::new("theme-roles").show(ui, |ui| {
                    for (role, value) in &t.colors {
                        ui.label(role);
                        let mut color = theme::parse_hex(value).unwrap_or(egui::Color32::GRAY);
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            edit = Some((role.clone(), Some(theme::to_hex(color))));
                        }
                        if ui.small_button("Reset").clicked() {
                            edit = Some((role.clone(), None));
                        }
                        ui.end_row();
                    }
                });
                if let Some((role, color)) = edit {
                    t.edit_color(&role, color.as_deref());
                }
                ui.horizontal(|ui| {
                    ui.label("Background tint");
                    let mut color = theme::parse_hex(&t.background.tint).unwrap_or(egui::Color32::BLACK);
                    if ui.color_edit_button_srgba(&mut color).changed() {
                        t.background.tint = theme::to_hex(color);
                    }
                });
                ui.checkbox(&mut t.background.enabled, "Show background");
                if ui.button("Choose image, GIF or video…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter(
                            "Background",
                            &["png", "jpg", "jpeg", "gif", "bmp", "webp", "mp4", "mov", "wmv", "m4v", "avi", "mkv", "webm"],
                        )
                        .pick_file()
                {
                    match themes::copy_media(&path, self.core.data_dir()) {
                        Ok(p) => {
                            t.background.path = p.display().to_string();
                            t.background.enabled = true;
                        }
                        Err(e) => self.show_toast(e.to_string(), true),
                    }
                }
                ui.add(egui::Slider::new(&mut t.background.blur, 0..=100).text("Blur"));
                ui.add(egui::Slider::new(&mut t.background.dim, 0..=100).text("Dim"));
                ui.add(egui::TextEdit::singleline(&mut self.theme_name).hint_text("Theme name"));
                if let Some(saved) = s.saved_themes.iter_mut().find(|saved| saved.id == t.id) {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Save changes").clicked() {
                            *saved = t.clone();
                            changed_library = true;
                        }
                        let name = self.theme_name.trim();
                        if ui.add_enabled(!name.is_empty() && name.chars().count() <= 80, egui::Button::new("Rename")).clicked() {
                            t.name = name.to_owned();
                            saved.name = t.name.clone();
                            self.theme_name.clear();
                            changed_library = true;
                        }
                        if self.theme_delete.as_ref() == Some(&t.id) {
                            if ui.button("Confirm delete").clicked() {
                                delete = true;
                            }
                            if ui.button("Cancel").clicked() {
                                self.theme_delete = None;
                            }
                        } else if ui.button("Delete theme").clicked() {
                            self.theme_delete = Some(t.id.clone());
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button("Save named copy").clicked() {
                        if self.theme_name.trim().is_empty() || self.theme_name.chars().count() > 80 {
                            self.show_toast("Enter a name of 1 to 80 characters", true);
                        } else {
                            t.fork(&self.theme_name);
                            s.saved_themes.push(t.clone());
                            self.theme_name.clear();
                            changed_library = true;
                        }
                    }
                    if ui.button("Revert changes").clicked()
                        && let Some(saved) = s.saved_themes.iter().chain(PRESETS.iter()).find(|saved| saved.id == t.id)
                    {
                        *t = saved.clone();
                    }
                    if ui.button("Export .ramtheme…").clicked()
                        && let Some(path) = rfd::FileDialog::new().set_file_name("theme.ramtheme").save_file()
                        && let Err(e) = themes::export(t, &path)
                    {
                        self.show_toast(e.to_string(), true);
                    }
                });
            });
        }
        if delete && let Some(id) = self.theme_delete.take() {
            s.saved_themes.retain(|t| t.id != id);
            s.appearance = None;
            changed_library = true;
        }
        changed_library || s.appearance != before
    }
}
