//! The advanced Roblox settings editor modal: search and edit every value in
//! `GlobalBasicSettings_13.xml`.

use crate::services::roblox_settings as rs;
use crate::ui::RodonApp;
use eframe::egui::{self, RichText};
use std::path::PathBuf;

pub struct EditorState {
    pub path: PathBuf,
    pub settings: Vec<rs::RobloxSetting>,
    pub search: String,
    /// The setting currently being edited: (name, kind, draft value).
    pub editing: Option<(String, String, String)>,
    pub error: Option<String>,
}

impl EditorState {
    pub fn load(path: PathBuf) -> EditorState {
        let (settings, error) = match rs::staged(&path) {
            Ok(settings) => (settings, None),
            Err(err) => (Vec::new(), Some(err.message)),
        };
        EditorState { path, settings, search: String::new(), editing: None, error }
    }
}

/// What the user asked for this frame, resolved after the modal closure returns.
enum Action {
    Edit(String),
    Cancel,
    Save,
}

impl RodonApp {
    /// Opens the advanced editor for the configured (or default) settings file.
    pub(in crate::ui) fn open_roblox_editor(&mut self) {
        match rs::settings_path(&self.core.settings().roblox) {
            Some(path) if path.exists() => self.roblox_editor = Some(EditorState::load(path)),
            Some(path) => self.show_toast(format!("Roblox settings file not found: {}", path.display()), true),
            None => self.show_toast("Could not locate the Roblox settings file", true),
        }
    }

    pub(in crate::ui) fn show_roblox_editor(&mut self, ctx: &egui::Context) {
        let Some(mut state) = self.roblox_editor.take() else { return };
        let palette = self.palette;
        let mut keep = true;
        let mut action: Option<Action> = None;

        egui::Modal::new(egui::Id::new("roblox-editor")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Advanced Roblox settings").size(17.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(egui_phosphor::regular::X).clicked() {
                        keep = false;
                    }
                });
            });
            ui.label(RichText::new(state.path.display().to_string()).size(10.0).color(palette.faint));
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::singleline(&mut state.search)
                    .hint_text(format!("{}  Search settings", egui_phosphor::regular::MAGNIFYING_GLASS))
                    .desired_width(f32::INFINITY),
            );
            if let Some(error) = &state.error {
                ui.label(RichText::new(error).color(palette.danger));
            }
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                if ui.button("Reload from Roblox").clicked() {
                    match rs::discard_profile(&state.path) {
                        Ok(()) => state = EditorState::load(state.path.clone()),
                        Err(e) => state.error = Some(e.message),
                    }
                }
                if ui.button("Apply profile to Roblox").clicked() {
                    match rs::apply_profile(&state.path, self.core.settings().roblox.advanced_auto_apply) {
                        Ok(()) => self.show_toast("Applied Roblox settings", false),
                        Err(e) => state.error = Some(e.message),
                    }
                }
            });
            let terms: Vec<String> = state.search.to_lowercase().split_whitespace().map(str::to_owned).collect();
            let editing_name = state.editing.as_ref().map(|(n, _, _)| n.clone());

            egui::ScrollArea::vertical().max_height(420.0).auto_shrink([false, false]).show(ui, |ui| {
                // Index into state.settings so we can show the current (unsaved) draft inline.
                for index in 0..state.settings.len() {
                    let (name, kind, value) = {
                        let s = &state.settings[index];
                        (s.name.clone(), s.kind.clone(), s.value.clone())
                    };
                    if !terms.iter().all(|t| name.to_lowercase().contains(t)) {
                        continue;
                    }
                    let editing_this = editing_name.as_deref() == Some(name.as_str());
                    egui::Frame::new().fill(palette.surface).corner_radius(8.0).inner_margin(egui::Margin::symmetric(10, 6)).show(
                        ui,
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&name).size(13.0).color(palette.text));
                                    ui.label(RichText::new(&kind).size(10.0).color(palette.faint));
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if editing_this {
                                        if ui.button(egui_phosphor::regular::CHECK).on_hover_text("Save").clicked() {
                                            action = Some(Action::Save);
                                        }
                                        if ui.button(egui_phosphor::regular::X).on_hover_text("Cancel").clicked() {
                                            action = Some(Action::Cancel);
                                        }
                                        if let Some((_, _, draft)) = state.editing.as_mut() {
                                            ui.add(egui::TextEdit::singleline(draft).desired_width(150.0));
                                        }
                                    } else {
                                        if ui.button(egui_phosphor::regular::PENCIL_SIMPLE).on_hover_text("Edit").clicked() {
                                            action = Some(Action::Edit(name.clone()));
                                        }
                                        ui.label(RichText::new(&value).size(12.0).monospace().color(palette.muted));
                                    }
                                });
                            });
                        },
                    );
                    ui.add_space(4.0);
                }
            });
        });

        match action {
            Some(Action::Edit(name)) => {
                if let Some(s) = state.settings.iter().find(|s| s.name == name) {
                    state.editing = Some((s.name.clone(), s.kind.clone(), s.value.clone()));
                }
            }
            Some(Action::Cancel) => state.editing = None,
            Some(Action::Save) => {
                if let Some((name, kind, draft)) = state.editing.clone() {
                    match rs::stage(&state.path, &name, &kind, &draft) {
                        Ok(()) => {
                            if let Some(item) = state.settings.iter_mut().find(|s| s.name == name) {
                                item.value = draft;
                            }
                            state.editing = None;
                            self.show_toast(format!("{name} saved to the local profile"), false);
                        }
                        Err(err) => self.show_toast(err.message, true),
                    }
                }
            }
            None => {}
        }

        if keep {
            self.roblox_editor = Some(state);
        }
    }
}
