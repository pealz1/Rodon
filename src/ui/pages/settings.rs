//! The Settings page: appearance, launching, accounts, windows, updates, security,
//! data and the local API / MCP server.

use crate::store::settings::{Launcher, Settings, ThemeMode, UpdateChannel, WindowTitleMode};
use crate::store::vault::Protection;
use crate::ui::RodonApp;
use crate::ui::task::Msg;
use crate::ui::{theme, widgets};
use eframe::egui::{self, RichText};
use std::sync::Arc;

impl RodonApp {
    pub(in crate::ui) fn page_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Settings").size(22.0).strong().color(self.palette.text));
        ui.add_space(10.0);
        let mut settings = self.core.settings();
        let mut dirty = false;

        egui::ScrollArea::vertical().id_salt("settings").auto_shrink([false, false]).show(ui, |ui| {
            dirty |= self.appearance(ui, &mut settings);
            dirty |= self.theme_controls(ui, &mut settings);
            ui.add_space(12.0);
            dirty |= self.launching(ui, &mut settings);
            ui.add_space(12.0);
            dirty |= self.accounts_section(ui, &mut settings);
            ui.add_space(12.0);
            dirty |= self.sign_in_section(ui, &mut settings);
            ui.add_space(12.0);
            dirty |= self.windows_section(ui, &mut settings);
            ui.add_space(12.0);
            dirty |= self.roblox_settings_section(ui, &mut settings);
            ui.add_space(12.0);
            self.downloads_section(ui);
            ui.add_space(12.0);
            self.updates_section(ui, &settings);
            ui.add_space(12.0);
            self.security_section(ui);
            ui.add_space(12.0);
            self.data_section(ui);
            ui.add_space(12.0);
            dirty |= self.api_section(ui, &mut settings);
            ui.add_space(12.0);
            self.about_section(ui);
        });

        if dirty {
            let old = self.core.settings();
            let ws_changed = old.websocket_enabled != settings.websocket_enabled || old.websocket_port != settings.websocket_port;
            if old.installer_fix != settings.installer_fix
                && let Err(e) = crate::services::installers::apply(settings.installer_fix)
            {
                self.show_toast(e.to_string(), true);
                settings.installer_fix = old.installer_fix;
            }
            let new = settings.clone();
            self.services.avatars.set_days(settings.avatar_cache_days);
            let _ = self.core.edit_settings(move |s| *s = new);
            if ws_changed && let Err(e) = self.services.configure_websocket() {
                self.show_toast(format!("WebSocket: {e}"), true);
            }
            self.palette = theme::resolved(&settings, self.system_dark);
            theme::apply(ui.ctx(), &self.palette, settings.ui_scale);
            ui.ctx().all_styles_mut(|style| style.animation_time = if settings.animations && self.animation_allowed { 0.14 } else { 0.0 });
        }
    }

    fn sign_in_section(&self, ui: &mut egui::Ui, s: &mut Settings) -> bool {
        let palette = self.palette;
        let mut dirty = false;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Browser sign-in");
            let browsers = crate::browser::discover::available();
            widgets::setting_row(ui, &palette, "Browser", "Used to open Roblox sign-in and registration", |ui| {
                let selected = if s.browser.is_empty() || s.browser == "auto" { "Automatic".to_owned() } else { s.browser.clone() };
                egui::ComboBox::from_id_salt("browser-engine").selected_text(selected).show_ui(ui, |ui| {
                    dirty |= ui.selectable_value(&mut s.browser, "auto".to_owned(), "Automatic").changed();
                    for b in &browsers {
                        dirty |= ui.selectable_value(&mut s.browser, b.label.clone(), &b.label).changed();
                    }
                });
            });
            if browsers.is_empty() {
                ui.label(
                    RichText::new("The system default browser will open Roblox. Portable Chromium is available below.")
                        .size(11.0)
                        .color(palette.warn),
                );
            } else {
                ui.label(
                    RichText::new(format!("Found: {}", browsers.iter().map(|b| b.label.as_str()).collect::<Vec<_>>().join(", ")))
                        .size(11.0)
                        .color(palette.faint),
                );
            }
        });
        dirty
    }

    fn roblox_settings_section(&mut self, ui: &mut egui::Ui, s: &mut Settings) -> bool {
        let palette = self.palette;
        let mut dirty = false;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Roblox settings (applied before launch)");
            let r = &mut s.roblox;
            if ui.checkbox(&mut r.advanced_auto_apply, "Auto apply saved profile").changed() {
                dirty = true;
                if !r.advanced_auto_apply {
                    crate::services::roblox_settings::unlock_profile();
                } else if let Some(path) = crate::services::roblox_settings::settings_path(r)
                    && let Err(e) = crate::services::roblox_settings::apply_profile(&path, true)
                {
                    self.show_toast(e.message, true);
                }
            }
            widgets::setting_row(ui, &palette, "Framerate cap", "Max FPS the client targets", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut r.framerate_cap_enabled)).changed();
                if r.framerate_cap_enabled {
                    dirty |= ui.add(egui::DragValue::new(&mut r.framerate_cap).range(1..=1000)).changed();
                }
            });
            widgets::setting_row(ui, &palette, "Master volume", "0.0 – 1.0", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut r.master_volume_enabled)).changed();
                if r.master_volume_enabled {
                    dirty |= ui.add(egui::Slider::new(&mut r.master_volume, 0.0..=1.0).step_by(0.05)).changed();
                }
            });
            widgets::setting_row(ui, &palette, "Start quality", "0 = Automatic, 1–10 fixed level", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut r.graphics_quality_enabled)).changed();
                if r.graphics_quality_enabled {
                    dirty |= ui.add(egui::DragValue::new(&mut r.graphics_quality).range(0..=10)).changed();
                }
            });
            widgets::setting_row(ui, &palette, "Settings file", "Leave blank for the default location", |ui| {
                if ui.button(egui_phosphor::regular::FOLDER_OPEN).clicked()
                    && let Some(path) = rfd::FileDialog::new().add_filter("Roblox settings", &["xml"]).pick_file()
                {
                    r.settings_path = path.display().to_string();
                    dirty = true;
                }
                dirty |= ui.add(egui::TextEdit::singleline(&mut r.settings_path).desired_width(180.0)).changed();
            });
            ui.add_space(4.0);
            if widgets::ghost_button(ui, &palette, &format!("{} Advanced editor", egui_phosphor::regular::SLIDERS)).clicked() {
                self.open_roblox_editor();
            }
        });
        dirty
    }

    fn downloads_section(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Downloads");

            // Roblox Player
            widgets::setting_row(ui, &palette, "Roblox Player", "Version hash, or LIVE for the latest", |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.download.version).desired_width(150.0));
            });
            widgets::setting_row(ui, &palette, "Install folder", "Where versions are installed", |ui| {
                if ui.button(egui_phosphor::regular::FOLDER_OPEN).clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder()
                {
                    self.download.folder = path.display().to_string();
                }
                ui.add(egui::TextEdit::singleline(&mut self.download.folder).desired_width(180.0));
            });
            let roblox = self.download.roblox.lock().unwrap_or_else(|p| p.into_inner()).clone();
            ui.horizontal(|ui| {
                if roblox.running {
                    ui.add(egui::ProgressBar::new(roblox.percent as f32 / 100.0).text(roblox.text).desired_width(200.0));
                } else {
                    if widgets::ghost_button(ui, &palette, &format!("{} Download Roblox", egui_phosphor::regular::DOWNLOAD_SIMPLE))
                        .clicked()
                    {
                        self.start_roblox_download();
                    }
                    if !roblox.text.is_empty() {
                        ui.label(RichText::new(&roblox.text).size(11.0).color(palette.faint));
                    }
                }
            });

            ui.add_space(8.0);
            // Portable Chromium
            let chromium = self.download.chromium.lock().unwrap_or_else(|p| p.into_inner()).clone();
            widgets::setting_row(ui, &palette, "Portable Chromium", "For browser sign-in without installed Chrome/Edge", |ui| {
                if chromium.running {
                    ui.add(egui::ProgressBar::new(chromium.percent as f32 / 100.0).text(chromium.text.clone()).desired_width(160.0));
                } else if widgets::ghost_button(ui, &palette, &format!("{} Download", egui_phosphor::regular::DOWNLOAD_SIMPLE)).clicked() {
                    self.start_chromium_download();
                }
            });
            if !chromium.running && !chromium.text.is_empty() {
                ui.label(RichText::new(&chromium.text).size(11.0).color(palette.faint));
            } else if crate::browser::chromium::is_installed() {
                ui.label(RichText::new("Portable Chromium is installed.").size(11.0).color(palette.online));
            }
        });
    }

    fn start_roblox_download(&mut self) {
        let version = self.download.version.trim().to_owned();
        let folder = self.download.folder.trim().to_owned();
        if folder.is_empty() {
            self.show_toast("Choose an install folder first", true);
            return;
        }
        let state = Arc::clone(&self.download.roblox);
        {
            let mut guard = state.lock().unwrap_or_else(|p| p.into_inner());
            guard.running = true;
            guard.percent = 0;
            guard.text = "Starting…".into();
        }
        let repaint = self.sender.clone();
        self.sender.spawn(Arc::clone(&self.core), Arc::clone(&self.services), move |_, _| {
            let progress = |percent: u8, text: &str| {
                let mut guard = state.lock().unwrap_or_else(|p| p.into_inner());
                guard.percent = percent;
                guard.text = text.to_owned();
                repaint.post(crate::ui::task::Msg::Run(Box::new(|_| {})));
            };
            let result = crate::services::downloader::download(&version, std::path::Path::new(&folder), &progress);
            let mut guard = state.lock().unwrap_or_else(|p| p.into_inner());
            guard.running = false;
            match &result {
                Ok(path) => guard.text = format!("Installed to {}", path.display()),
                Err(err) => guard.text = err.message.clone(),
            }
            match result {
                Ok(_) => crate::ui::task::Msg::Toast("Roblox downloaded".into(), false),
                Err(err) => crate::ui::task::Msg::Toast(err.message, true),
            }
        });
    }

    fn start_chromium_download(&mut self) {
        let state = Arc::clone(&self.download.chromium);
        {
            let mut guard = state.lock().unwrap_or_else(|p| p.into_inner());
            guard.running = true;
            guard.percent = 0;
            guard.text = "Starting…".into();
        }
        let repaint = self.sender.clone();
        self.sender.spawn(Arc::clone(&self.core), Arc::clone(&self.services), move |_, _| {
            let progress = |percent: u8, text: &str| {
                let mut guard = state.lock().unwrap_or_else(|p| p.into_inner());
                guard.percent = percent;
                guard.text = text.to_owned();
                repaint.post(crate::ui::task::Msg::Run(Box::new(|_| {})));
            };
            let result = crate::browser::chromium::download(&progress);
            let mut guard = state.lock().unwrap_or_else(|p| p.into_inner());
            guard.running = false;
            guard.text = match &result {
                Ok(_) => "Portable Chromium installed".into(),
                Err(err) => err.message.clone(),
            };
            match result {
                Ok(_) => crate::ui::task::Msg::Toast("Chromium downloaded".into(), false),
                Err(err) => crate::ui::task::Msg::Toast(err.message, true),
            }
        });
    }

    fn appearance(&self, ui: &mut egui::Ui, s: &mut Settings) -> bool {
        let palette = self.palette;
        let mut dirty = false;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Appearance");
            dirty |= ui.checkbox(&mut s.animations, "Smooth animations").changed();
            dirty |= ui.checkbox(&mut s.splash, "Show startup window").changed();
            widgets::setting_row(ui, &palette, "Theme", "", |ui| {
                egui::ComboBox::from_id_salt("theme").selected_text(format!("{:?}", s.theme)).show_ui(ui, |ui| {
                    for mode in [ThemeMode::Dark, ThemeMode::Light, ThemeMode::System] {
                        dirty |= ui.selectable_value(&mut s.theme, mode, format!("{mode:?}")).changed();
                    }
                });
            });
            widgets::setting_row(ui, &palette, "Accent colour", "", |ui| {
                let mut color = theme::parse_hex(&s.accent).unwrap_or(self.palette.accent);
                if ui.color_edit_button_srgba(&mut color).changed() {
                    s.accent = theme::to_hex(color);
                    dirty = true;
                }
            });
            widgets::setting_row(ui, &palette, "Interface scale", "", |ui| {
                dirty |= ui.add(egui::Slider::new(&mut s.ui_scale, 0.8..=1.6).step_by(0.05)).changed();
            });
            widgets::setting_row(ui, &palette, "Compact rows", "Tighter account list", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut s.compact_rows)).changed();
            });
            widgets::setting_row(ui, &palette, "Show avatars", "", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut s.show_avatars)).changed();
            });
        });
        dirty
    }

    fn launching(&self, ui: &mut egui::Ui, s: &mut Settings) -> bool {
        let palette = self.palette;
        let mut dirty = false;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Launching");
            widgets::setting_row(ui, &palette, "Launcher", "", |ui| {
                egui::ComboBox::from_id_salt("launcher").selected_text(s.launcher.label()).show_ui(ui, |ui| {
                    for launcher in Launcher::ALL {
                        dirty |= ui.selectable_value(&mut s.launcher, launcher, launcher.label()).changed();
                    }
                });
            });
            if s.launcher == Launcher::Custom {
                widgets::setting_row(ui, &palette, "Custom launcher", "", |ui| {
                    if ui.button(egui_phosphor::regular::FOLDER_OPEN).clicked()
                        && let Some(path) = rfd::FileDialog::new().add_filter("Executable", &["exe"]).pick_file()
                    {
                        s.custom_launcher_path = path.display().to_string();
                        dirty = true;
                    }
                    ui.add(egui::TextEdit::singleline(&mut s.custom_launcher_path).desired_width(200.0));
                });
            }
            widgets::setting_row(ui, &palette, "Launch delay", "Seconds between accounts in a batch", |ui| {
                dirty |= ui.add(egui::Slider::new(&mut s.launch_delay_secs, 0.0..=10.0).step_by(0.5)).changed();
            });
            widgets::setting_row(ui, &palette, "Confirm before launch", "Ask before launching many accounts", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut s.confirm_before_launch)).changed();
            });
        });
        dirty
    }

    fn accounts_section(&self, ui: &mut egui::Ui, s: &mut Settings) -> bool {
        let palette = self.palette;
        let mut dirty = false;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Accounts");
            dirty |= ui.checkbox(&mut s.multi_select, "Ctrl / Shift multi-select").changed();
            widgets::setting_row(ui, &palette, "Validate cookies on startup", "", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut s.validate_cookies_on_startup)).changed();
            });
            widgets::setting_row(ui, &palette, "Validation delay", "Seconds between cookie checks", |ui| {
                dirty |= ui.add(egui::Slider::new(&mut s.cookie_validation_delay_secs, 0.5..=10.0).step_by(0.5)).changed();
            });
            widgets::setting_row(ui, &palette, "Avatar refresh", "Days before re-downloading (0 = keep)", |ui| {
                dirty |= ui.add(egui::DragValue::new(&mut s.avatar_cache_days).range(0..=90)).changed();
            });
            widgets::setting_row(
                ui,
                &palette,
                "Allow copying secrets",
                "Enables copying or exporting selected passwords and cookies",
                |ui| {
                    dirty |= ui.add(egui::Checkbox::without_text(&mut s.allow_copy_secrets)).changed();
                },
            );
        });
        dirty
    }

    fn windows_section(&self, ui: &mut egui::Ui, s: &mut Settings) -> bool {
        let palette = self.palette;
        let mut dirty = false;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Windows & process");
            dirty |= ui.checkbox(&mut s.window_grid_hotkey_enabled, "Global window grid shortcut").changed();
            if s.window_grid_hotkey_enabled {
                dirty |= ui.add(egui::TextEdit::singleline(&mut s.window_grid_hotkey).hint_text("Ctrl+Shift+A")).changed();
            }
            dirty |= ui.checkbox(&mut s.installer_fix, "Temporarily quarantine Roblox installers").changed();
            dirty |= ui
                .checkbox(&mut s.optimize_ram, "Trim large client working sets")
                .on_hover_text("Trimming can cause stutter or crashes.")
                .changed();
            if s.optimize_ram {
                dirty |= ui.add(egui::DragValue::new(&mut s.ram_limit_mb).range(100..=16384).suffix(" MB")).changed();
            }
            widgets::setting_row(ui, &palette, "Scan interval", "Seconds between client scans", |ui| {
                dirty |= ui.add(egui::DragValue::new(&mut s.instance_scan_secs).range(3..=60)).changed();
            });
            if ui.checkbox(&mut s.always_on_top, "Always on top").changed() {
                dirty = true;
                ui.ctx().send_viewport_cmd_to(
                    egui::ViewportId::ROOT,
                    egui::ViewportCommand::WindowLevel(if s.always_on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    }),
                );
            }
            dirty |= ui.checkbox(&mut s.close_to_tray, "Close to system tray").changed();
            let mut shortcut = crate::win::startup::is_start_menu_shortcut_enabled();
            if ui.checkbox(&mut shortcut, "Start Menu shortcut").changed()
                && let Err(err) = crate::win::startup::set_start_menu_shortcut(shortcut)
            {
                crate::log_warn!("{}", err);
            }
            widgets::setting_row(ui, &palette, "Log files to keep", "0 keeps all files", |ui| {
                dirty |= ui.add(egui::DragValue::new(&mut s.log_retention).range(0..=500)).changed();
            });
            widgets::setting_row(ui, &palette, "Rename Roblox windows", "Title each window by its account", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut s.rename_windows)).changed();
            });
            widgets::setting_row(ui, &palette, "Window title", "", |ui| {
                egui::ComboBox::from_id_salt("title-mode").selected_text(format!("{:?}", s.window_title_mode)).show_ui(ui, |ui| {
                    for mode in [WindowTitleMode::Username, WindowTitleMode::Alias, WindowTitleMode::Note, WindowTitleMode::UsernameAndNote]
                    {
                        dirty |= ui.selectable_value(&mut s.window_title_mode, mode, format!("{mode:?}")).changed();
                    }
                });
            });
            widgets::setting_row(ui, &palette, "Activity monitor", "Show running clients and RAM", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut s.activity_monitor)).changed();
            });
            widgets::setting_row(ui, &palette, "Track server history", "Record joined servers for the Servers page", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut s.track_server_history)).changed();
            });
            widgets::setting_row(ui, &palette, "Start with Windows", "", |ui| {
                let mut on = crate::win::startup::is_startup_enabled();
                if ui.add(egui::Checkbox::without_text(&mut on)).changed() {
                    let _ = crate::win::startup::set_startup(on);
                }
            });
        });
        dirty
    }

    fn updates_section(&mut self, ui: &mut egui::Ui, s: &Settings) {
        let palette = self.palette;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Updates");
            ui.hyperlink_to("Manual download", format!("https://github.com/{}/releases", crate::REPO));
            let mut check = s.check_updates;
            widgets::setting_row(ui, &palette, "Check on startup", "", |ui| {
                if ui.add(egui::Checkbox::without_text(&mut check)).changed() {
                    let _ = self.core.edit_settings(move |s| s.check_updates = check);
                }
            });
            let mut channel = s.update_channel;
            widgets::setting_row(ui, &palette, "Channel", "", |ui| {
                egui::ComboBox::from_id_salt("channel").selected_text(format!("{channel:?}")).show_ui(ui, |ui| {
                    let mut changed = false;
                    changed |= ui.selectable_value(&mut channel, UpdateChannel::Stable, "Stable").changed();
                    changed |= ui.selectable_value(&mut channel, UpdateChannel::Beta, "Beta").changed();
                    if changed {
                        let _ = self.core.edit_settings(move |s| s.update_channel = channel);
                    }
                });
            });
            ui.horizontal(|ui| {
                if let Some(version) = self.services.live.update_available() {
                    ui.label(RichText::new(format!("v{version} available")).color(self.palette.accent));
                    if ui.button("Skip version").clicked() {
                        let _ = self.core.edit_settings(|s| s.skipped_version = version.clone());
                        self.services.live.set_update_available(None);
                    }
                    let notes = self.services.live.update_notes.read().unwrap_or_else(|e| e.into_inner()).clone();
                    ui.collapsing("Release notes", |ui| {
                        ui.label(notes);
                    });
                    if widgets::primary_button(ui, &palette, "Install update").clicked() {
                        self.install_update();
                    }
                } else {
                    if widgets::ghost_button(ui, &palette, "Check now").clicked() {
                        self.check_updates_now();
                    }
                    ui.label(RichText::new(format!("v{} is current", crate::VERSION)).size(11.0).color(self.palette.faint));
                }
            });
        });
    }

    fn security_section(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Vault encryption");
            let current = self.core.protection();
            ui.label(RichText::new(format!("Current: {}", current.label())).size(12.0).color(self.palette.muted));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if current != Protection::Windows && widgets::ghost_button(ui, &palette, "Use Windows account").clicked() {
                    self.switch_protection(Protection::Windows, None);
                }
                if current != Protection::None && widgets::ghost_button(ui, &palette, "Remove encryption").clicked() {
                    self.switch_protection(Protection::None, None);
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let pw = &mut self.vault_password; // reuse the scratch input field
                ui.add(egui::TextEdit::singleline(pw).password(true).hint_text("password").desired_width(160.0));
                if widgets::ghost_button(ui, &palette, "Set password").clicked() {
                    let password = self.vault_password.clone();
                    if password.len() < 6 {
                        self.show_toast("Use at least 6 characters", true);
                    } else {
                        self.switch_protection(Protection::Password, Some(password.clone()));
                        self.vault_password.clear();
                    }
                }
            });
        });
    }

    fn data_section(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Backup & data");
            if ui.button("Erase local Rodon data…").clicked() {
                let ctx = ui.ctx().clone();
                self.confirm = Some(crate::ui::Confirm {
                    title: "Erase this Rodon profile?".into(),
                    body: format!(
                        "Rodon will exit and remove its vault, backup, settings, themes, logs and cached downloads from {}.",
                        self.core.data_dir().display()
                    ),
                    action: Box::new(move |app| {
                        crate::paths::WIPE_ON_EXIT.store(true, std::sync::atomic::Ordering::Release);
                        app.quitting = true;
                        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
                    }),
                });
            }
            ui.add(egui::TextEdit::singleline(&mut self.backup_password).password(true).hint_text("Backup password"));
            ui.checkbox(&mut self.replace_backup, "Replace saved accounts when importing a backup");
            ui.horizontal(|ui| {
                if widgets::ghost_button(ui, &palette, &format!("{} Export backup", egui_phosphor::regular::EXPORT)).clicked() {
                    self.export_backup();
                }
                if widgets::ghost_button(ui, &palette, &format!("{} Import backup", egui_phosphor::regular::DOWNLOAD_SIMPLE)).clicked() {
                    self.import_backup();
                }
                if ui.button(format!("{} Open data folder", egui_phosphor::regular::FOLDER_OPEN)).clicked() {
                    let _ = open_folder(self.core.data_dir());
                }
            });
        });
    }

    fn api_section(&mut self, ui: &mut egui::Ui, s: &mut Settings) -> bool {
        let palette = self.palette;
        let mut dirty = false;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "Local API & AI (MCP)");
            dirty |= ui.checkbox(&mut s.websocket_enabled, "WebSocket command server").changed();
            widgets::setting_row(ui, &palette, "WebSocket port", "Localhost only", |ui| {
                dirty |= ui.add(egui::DragValue::new(&mut s.websocket_port).range(1024..=65535)).changed();
            });
            dirty |= ui.checkbox(&mut s.websocket_auth, "Require WebSocket password").changed();
            if s.websocket_auth {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.ws_password).password(true).hint_text("New WebSocket password"));
                    if ui.button("Save password").clicked() && !self.ws_password.is_empty() {
                        match self.core.set_secret("websocket_password", &self.ws_password) {
                            Ok(()) => self.ws_password.clear(),
                            Err(e) => self.show_toast(e.message, true),
                        }
                    }
                });
            }

            ui.label(RichText::new("A loopback server so tools like Claude and Codex can manage accounts. Token-protected; off-machine requests are refused.").size(11.0).color(self.palette.faint));
            ui.add_space(6.0);
            widgets::setting_row(ui, &palette, "Enable API server", "127.0.0.1 only", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut s.api_enabled)).changed();
            });
            widgets::setting_row(ui, &palette, "Port", "", |ui| {
                dirty |= ui.add(egui::DragValue::new(&mut s.api_port).range(1024..=65535)).changed();
            });
            widgets::setting_row(ui, &palette, "Expose secrets", "Allow the API to return cookies (dangerous)", |ui| {
                dirty |= ui.add(egui::Checkbox::without_text(&mut s.api_expose_secrets)).changed();
            });
            let token = self.core.secret("api_token").unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Token").size(12.0).color(self.palette.muted));
                let shown = if token.is_empty() {
                    "(none — generate one)".to_owned()
                } else {
                    format!("{}…{}", &token[..token.len().min(6)], &token[token.len().saturating_sub(4)..])
                };
                ui.label(RichText::new(shown).monospace().size(11.0).color(self.palette.text));
                if ui.button(egui_phosphor::regular::COPY).on_hover_text("Copy token").clicked() {
                    ui.ctx().copy_text(token.clone());
                    self.show_toast("Token copied", false);
                }
                if ui.button(egui_phosphor::regular::ARROWS_CLOCKWISE).on_hover_text("Regenerate").clicked() {
                    let new = crate::store::crypto::b64(&crate::store::crypto::random_bytes::<24>());
                    let _ = self.core.set_secret("api_token", &new);
                    self.show_toast("New API token generated", false);
                }
            });
            if ui.button(format!("{} API documentation", egui_phosphor::regular::BOOK_OPEN)).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(format!("https://github.com/{}/blob/HEAD/docs/API.md", crate::REPO)));
            }
        });
        dirty
    }

    fn about_section(&self, ui: &mut egui::Ui) {
        let palette = self.palette;
        widgets::card(ui, &palette, |ui| {
            widgets::section(ui, &palette, "About");
            ui.label(RichText::new(format!("{} v{}", crate::APP_NAME, crate::VERSION)).strong().color(self.palette.text));
            ui.label(
                RichText::new(
                    "Rodon is a Rust fork of Evanovar RAM by evanovar. The original Roblox Account Manager was created by ic3w0lf22.",
                )
                .size(11.0)
                .color(self.palette.muted),
            );
            ui.label(RichText::new("Licensed under GPL-3.0.").size(11.0).color(self.palette.faint));
            ui.hyperlink_to("Evanovar RAM", "https://github.com/evanovar/RobloxAccountManager");
            ui.hyperlink_to("Original Roblox Account Manager", "https://github.com/ic3w0lf22/Roblox-Account-Manager");
        });
    }

    // ---- Actions ----

    /// Re-encrypts the vault off the UI thread (a password change runs Argon2).
    fn switch_protection(&mut self, protection: Protection, password: Option<String>) {
        self.show_toast("Changing vault encryption…", false);
        self.sender.spawn(Arc::clone(&self.core), Arc::clone(&self.services), move |core, _| {
            match core.set_protection(protection, password.as_deref()) {
                Ok(()) => Msg::Toast(format!("Vault now: {}", protection.label()), false),
                Err(err) => Msg::Toast(err.message, true),
            }
        });
    }

    fn check_updates_now(&mut self) {
        let beta = self.core.settings().update_channel == UpdateChannel::Beta;
        self.sender.spawn(Arc::clone(&self.core), Arc::clone(&self.services), move |_, services| {
            match crate::services::updater::check(beta) {
                Ok(Some(info)) => {
                    *services.live.update_notes.write().unwrap_or_else(|e| e.into_inner()) = info.notes;
                    services.live.set_update_available(Some(info.version.clone()));
                    Msg::Toast(format!("Update available: v{}", info.version), false)
                }
                Ok(None) => Msg::Toast("You're on the latest version".into(), false),
                Err(err) => Msg::Toast(err.message, true),
            }
        });
    }

    fn install_update(&mut self) {
        let beta = self.core.settings().update_channel == UpdateChannel::Beta;
        self.show_toast("Downloading update…", false);
        self.sender.spawn(Arc::clone(&self.core), Arc::clone(&self.services), move |_, _| match crate::services::updater::check(beta) {
            Ok(Some(info)) => match crate::services::updater::download_and_apply(&info) {
                Ok(()) => Msg::Toast("Update ready — close Rodon to finish installing".into(), false),
                Err(err) => Msg::Toast(err.message, true),
            },
            Ok(None) => Msg::Toast("No update to install".into(), false),
            Err(err) => Msg::Toast(err.message, true),
        });
    }

    fn export_backup(&mut self) {
        let Some(path) =
            rfd::FileDialog::new().set_file_name("accounts.rodonbackup").add_filter("Rodon backup", &["rodonbackup"]).save_file()
        else {
            return;
        };
        // Ask for a password via the scratch field is clumsy; use a fixed prompt modal instead.
        let password = self.backup_password.clone();
        if password.len() < 8 {
            self.show_toast("Type a backup password (8+ chars) in the backup password box first", true);
            return;
        }
        let accounts = self.core.accounts();
        match crate::import::backup::export(&accounts, &path, &password) {
            Ok(count) => self.show_toast(format!("Exported {count} account(s)"), false),
            Err(err) => self.show_toast(err.message, true),
        }
    }

    fn import_backup(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Backup", &["rodonbackup", "novabackup", "json"]).pick_file() else { return };
        let password = self.backup_password.clone();
        match crate::import::backup::import(&path, &password) {
            Ok(batch) => match self.core.import_accounts(&batch, self.replace_backup) {
                Ok(outcome) => {
                    self.reload_accounts();
                    self.show_toast(format!("Imported {} added, {} updated", outcome.added, outcome.updated), false);
                }
                Err(err) => self.show_toast(err.message, true),
            },
            Err(err) => self.show_toast(format!("{} (type the backup password in the backup password box)", err.message), true),
        }
    }
}

#[cfg(windows)]
fn open_folder(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("explorer").arg(path).spawn().map(|_| ())
}
#[cfg(not(windows))]
fn open_folder(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}
