//! The desktop GUI (egui/eframe).

mod background;
mod games;
mod theme_editor;
mod video;
pub static SMOKE_PASSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
mod console;
mod signup;
mod task;
mod theme;
mod tray;
mod unlock;
mod widgets;
mod pages {
    pub mod accounts;
    pub mod add_account;
    pub mod anti_afk;
    pub mod multi;
    pub mod private_servers;
    pub mod rejoin;
    pub mod roblox_settings;
    pub mod servers;
    pub mod settings;
}

use crate::core::Core;
use crate::services::{Services, auto_rejoin::RejoinManager};
use crate::store::model::Account;
use eframe::egui::{self, Color32, RichText};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use task::{Msg, TaskSender, Tasks};
use theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Donations,
    Console,
    Accounts,
    Servers,
    PrivateServers,
    AutoRejoin,
    AntiAfk,
    MultiRoblox,
    Settings,
}

impl Page {
    const NAV: [(Page, &'static str, &'static str); 9] = [
        (Page::Accounts, egui_phosphor::regular::USERS_THREE, "Accounts"),
        (Page::Servers, egui_phosphor::regular::GLOBE_HEMISPHERE_WEST, "Servers"),
        (Page::PrivateServers, egui_phosphor::regular::LOCK_KEY, "Private Servers"),
        (Page::AutoRejoin, egui_phosphor::regular::ARROWS_CLOCKWISE, "Auto-Rejoin"),
        (Page::AntiAfk, egui_phosphor::regular::PERSON_SIMPLE_WALK, "Anti-AFK"),
        (Page::MultiRoblox, egui_phosphor::regular::STACK, "Multi Roblox"),
        (Page::Settings, egui_phosphor::regular::GEAR_SIX, "Settings"),
        (Page::Console, egui_phosphor::regular::TERMINAL, "Console"),
        (Page::Donations, egui_phosphor::regular::HEART, "Donations"),
    ];
}

/// State for the launch bar shared by the Accounts page.
#[derive(Default)]
pub struct LaunchBar {
    pub place_id: String,
    pub private_server: String,
    pub job_id: String,
    pub game_name: String,
    pub join_user: String,
}

/// Progress of a background task (Roblox download, Chromium download), shared with
/// the worker thread.
#[derive(Default, Clone)]
pub struct TaskProgress {
    pub running: bool,
    pub percent: u8,
    pub text: String,
}

/// Inputs and progress for the Roblox / Chromium download controls in Settings.
pub struct DownloadUi {
    pub version: String,
    pub folder: String,
    pub roblox: Arc<std::sync::Mutex<TaskProgress>>,
    pub chromium: Arc<std::sync::Mutex<TaskProgress>>,
}

impl Default for DownloadUi {
    fn default() -> Self {
        let folder = crate::services::downloader::default_versions_dir().map(|p| p.display().to_string()).unwrap_or_default();
        DownloadUi {
            version: "LIVE".into(),
            folder,
            roblox: Arc::new(std::sync::Mutex::new(TaskProgress::default())),
            chromium: Arc::new(std::sync::Mutex::new(TaskProgress::default())),
        }
    }
}

pub struct RodonApp {
    pub core: Arc<Core>,
    pub services: Arc<Services>,
    pub tasks: Tasks,
    pub sender: TaskSender,
    pub palette: Palette,
    pub system_dark: bool,
    pub page: Page,
    smoke_page: Option<usize>,
    startup_size_pending: bool,
    last_error: Option<String>,
    recording_action: bool,
    startup: Option<std::thread::JoinHandle<()>>,
    splash: bool,
    animation_allowed: bool,
    last_page: Page,
    transition: Instant,

    ws_password: String,
    backup_password: String,
    replace_backup: bool,
    games: Option<games::Games>,
    theme_name: String,
    media: background::Media,
    pub download: DownloadUi,
    detached: HashSet<Page>,
    last_size: [f32; 2],
    vault_password: String,
    tray: Option<tray::Tray>,
    quitting: bool,

    pub accounts: Vec<Account>,
    pub revision: u64,
    pub selection: HashSet<String>,
    pub last_clicked: Option<String>,
    pub search: String,
    pub group_filter: Option<String>,

    pub launch: LaunchBar,
    pub rejoin: Arc<RejoinManager>,
    pub anti_afk: Option<crate::services::anti_afk::AntiAfkHandle>,

    pub avatars: HashMap<u64, egui::TextureHandle>,
    pub avatar_pending: HashSet<u64>,
    pub game_names: HashMap<u64, String>,
    pub game_name_pending: HashSet<u64>,

    pub toast: Option<(String, bool, Instant)>,
    pub add_dialog: Option<pages::add_account::AddDialog>,
    pub rejoin_dialog: Option<pages::rejoin::RejoinDialog>,
    pub ps_state: pages::private_servers::PrivateServersState,
    pub roblox_editor: Option<pages::roblox_settings::EditorState>,
    pub confirm: Option<Confirm>,
    pub group_input: String,
    pub renaming_group: Option<(String, String)>,
}

/// A pending confirmation modal.
pub struct Confirm {
    pub title: String,
    pub body: String,
    pub action: Box<dyn FnOnce(&mut RodonApp) + Send>,
}

impl RodonApp {
    fn new(cc: &eframe::CreationContext<'_>, core: Arc<Core>, services: Arc<Services>) -> RodonApp {
        theme::install_fonts(&cc.egui_ctx);
        let system_dark = !matches!(cc.egui_ctx.theme(), egui::Theme::Light);
        let settings = core.settings();
        let palette = theme::resolved(&settings, system_dark);
        theme::apply(&cc.egui_ctx, &palette, settings.ui_scale);

        cc.egui_ctx.all_styles_mut(|style| style.animation_time = if settings.animations && windows_animations() { 0.14 } else { 0.0 });
        let tasks = Tasks::new(cc.egui_ctx.clone());
        let sender = tasks.sender();
        let ctx = cc.egui_ctx.clone();
        services.set_repaint(move || ctx.request_repaint());

        let mut app = RodonApp {
            core: Arc::clone(&core),
            services: services.clone(),
            tasks,
            sender,
            palette,
            system_dark,
            page: Page::Accounts,
            smoke_page: None,
            startup_size_pending: settings.splash,
            last_error: None,
            recording_action: false,
            splash: settings.splash,
            startup: None,
            animation_allowed: windows_animations(),
            last_page: Page::Accounts,
            transition: Instant::now(),

            ws_password: String::new(),
            backup_password: String::new(),
            replace_backup: false,
            games: None,
            theme_name: String::new(),
            media: Default::default(),
            download: DownloadUi::default(),
            detached: HashSet::new(),
            last_size: settings.window_size,
            vault_password: String::new(),
            tray: None,
            quitting: false,
            accounts: Vec::new(),
            revision: 0,
            selection: HashSet::new(),
            last_clicked: None,
            search: String::new(),
            group_filter: None,
            launch: LaunchBar::default(),
            rejoin: services.rejoin.clone(),
            anti_afk: None,
            avatars: HashMap::new(),
            avatar_pending: HashSet::new(),
            game_names: HashMap::new(),
            game_name_pending: HashSet::new(),
            toast: None,
            add_dialog: None,
            rejoin_dialog: None,
            ps_state: Default::default(),
            roblox_editor: None,
            confirm: None,
            group_input: String::new(),
            renaming_group: None,
        };
        app.reload_accounts();
        if core.recovered() {
            app.show_toast("Recovered accounts from the local backup. Recent changes may be missing.", true);
        }
        app
    }

    pub fn reload_accounts(&mut self) {
        self.accounts = self.core.accounts();
        self.revision = self.core.revision();
        let keys: HashSet<String> = self.accounts.iter().map(Account::key).collect();
        self.selection.retain(|k| keys.contains(k));
        self.services.avatars.prune(&self.accounts.iter().map(|a| a.user_id).collect());
    }

    /// Accounts after the search and group filters are applied.
    pub fn visible_accounts(&self) -> Vec<Account> {
        let terms: Vec<String> = self.search.to_lowercase().split_whitespace().map(str::to_owned).collect();
        self.accounts
            .iter()
            .filter(|a| self.group_filter.as_ref().is_none_or(|g| &a.group == g))
            .filter(|a| {
                if terms.is_empty() {
                    return true;
                }
                let haystack = format!("{} {} {} {} {}", a.username, a.alias, a.note, a.group, a.user_id).to_lowercase();
                terms.iter().all(|t| haystack.contains(t))
            })
            .cloned()
            .collect()
    }

    pub fn selected_keys(&self) -> Vec<String> {
        self.accounts.iter().map(|a| a.key()).filter(|k| self.selection.contains(k)).collect()
    }

    pub fn show_toast(&mut self, message: impl Into<String>, error: bool) {
        let message = message.into();
        if error {
            self.last_error = Some(crate::logging::redact(&message));
            crate::log_error!("{}", message);
        }
        self.toast = Some((message, error, Instant::now()));
    }

    /// Returns a texture for an account avatar, fetching it in the background if missing.
    pub fn avatar_texture(&mut self, ctx: &egui::Context, user_id: u64) -> Option<egui::TextureHandle> {
        if user_id == 0 {
            return None;
        }
        if !self.avatar_pending.contains(&user_id) && self.services.avatars.needs_refresh(user_id) && self.avatar_pending.insert(user_id) {
            let avatars = Arc::clone(&self.services.avatars);
            let sender = self.sender.clone();
            std::thread::Builder::new()
                .name("avatar-fetch".into())
                .spawn(move || {
                    let updated = !avatars.refresh(&[user_id]).is_empty();
                    sender.post(Msg::Run(Box::new(move |app| {
                        if updated {
                            app.avatars.remove(&user_id);
                            app.avatar_pending.remove(&user_id);
                        }
                    })));
                })
                .ok();
        }
        if let Some(handle) = self.avatars.get(&user_id) {
            return Some(handle.clone());
        }
        if let Some(bytes) = self.services.avatars.cached(user_id)
            && let Ok(image) = image::load_from_memory(&bytes)
        {
            let rgba = image.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
            let handle = ctx.load_texture(format!("avatar-{user_id}"), color, egui::TextureOptions::LINEAR);
            self.avatars.insert(user_id, handle.clone());
            return Some(handle);
        }
        None
    }

    fn splash_frame(&mut self, ui: &mut egui::Ui) -> bool {
        if !self.splash {
            return false;
        }
        if self.startup.as_ref().is_some_and(|w| w.is_finished()) {
            self.splash = false;
            if let Some(task) = self.startup.take() {
                let _ = task.join();
            }
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(820.0, 540.0)));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(self.last_size.into()));
            return false;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(22.0);
                ui.heading("Rodon");
                ui.label(format!("Version {}", crate::VERSION));
                ui.spinner();
                ui.label("Starting services…");
            });
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        true
    }
    fn drain_messages(&mut self, ctx: &egui::Context) {
        for msg in self.tasks.drain() {
            match msg {
                Msg::Toast(text, error) => self.show_toast(text, error),
                Msg::AccountsChanged => self.reload_accounts(),
                Msg::GameName(place, name) => {
                    self.game_name_pending.remove(&place);
                    if self.launch.place_id.trim() == place.to_string() {
                        self.launch.game_name = name.clone();
                    }
                    self.game_names.insert(place, name);
                }
                Msg::PrivateServers(result) => self.ps_state.on_loaded(result),
                Msg::Run(callback) => callback(self),
            }
        }
        // Background workers also raise toasts through Live.
        if let Some((text, error)) = self.services.live.take_toast() {
            self.show_toast(text, error);
        }
        let _ = ctx;
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.label(RichText::new(egui_phosphor::regular::ROCKET_LAUNCH).size(22.0).color(self.palette.accent));
            ui.label(RichText::new("Rodon").size(18.0).strong().color(self.palette.text));
        });
        ui.add_space(16.0);

        for (page, icon, label) in Page::NAV {
            let selected = self.page == page;
            let (text_color, bg) =
                if selected { (self.palette.accent_text, self.palette.accent) } else { (self.palette.muted, Color32::TRANSPARENT) };
            let label_text = format!("{icon}  {label}");
            let button = egui::Button::new(RichText::new(label_text).color(text_color).size(13.5))
                .fill(bg)
                .corner_radius(9.0)
                .min_size(egui::vec2(ui.available_width() - 16.0, 34.0));
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let response = ui.add(button);
                let id = egui::ViewportId::from_hash_of(page);
                if response.clicked() {
                    if self.detached.contains(&page) {
                        ui.ctx().send_viewport_cmd_to(id, egui::ViewportCommand::Focus);
                    } else {
                        self.page = page;
                    }
                }
                response.context_menu(|ui| {
                    if self.detached.contains(&page) {
                        if ui.button("Show window").clicked() {
                            ui.ctx().send_viewport_cmd_to(id, egui::ViewportCommand::Focus);
                            ui.close();
                        }
                        if ui.button("Reattach page").clicked() {
                            self.detached.remove(&page);
                            self.page = page;
                            ui.close();
                        }
                    } else if ui.button("Open in new window").clicked() {
                        self.detached.insert(page);
                        if self.page == page
                            && let Some((other, _, _)) = Page::NAV.iter().find(|(p, _, _)| !self.detached.contains(p))
                        {
                            self.page = *other;
                        }
                        ui.close();
                    }
                });
            });
            ui.add_space(2.0);
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.add_space(12.0);
            ui.label(RichText::new(format!("v{}", crate::VERSION)).size(10.0).color(self.palette.faint));
            if let Some(version) = self.services.live.update_available()
                && ui
                    .button(
                        RichText::new(format!("{} Update to v{version}", egui_phosphor::regular::DOWNLOAD_SIMPLE))
                            .color(self.palette.accent),
                    )
                    .clicked()
            {
                self.page = Page::Settings;
            }
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(format!("{}  Kill all Roblox", egui_phosphor::regular::X_CIRCLE)).color(self.palette.danger),
                    )
                    .fill(Color32::TRANSPARENT),
                )
                .clicked()
            {
                self.confirm = Some(Confirm {
                    title: "Close all Roblox clients?".into(),
                    body: "Every running Roblox game on this computer will close.".into(),
                    action: Box::new(|app| {
                        // kill_all waits up to a few seconds per process, so run it off the UI thread.
                        app.sender.spawn(Arc::clone(&app.core), Arc::clone(&app.services), |_, _| {
                            let (closed, remaining) = crate::win::process::kill_all();
                            Msg::Toast(format!("Closed {closed} client(s), {remaining} remaining"), remaining > 0)
                        });
                    }),
                });
            }
        });
    }

    fn render_page(&mut self, ui: &mut egui::Ui, page: Page) {
        match page {
            Page::Console => self.page_console(ui),
            Page::Donations => {
                ui.heading("Support evanovar");
                ui.label("Evanovar created the Python account manager that Rodon is based on.");
                ui.hyperlink_to("Open evanovar’s donation store", "https://www.roblox.com/games/718090786/donation#!/store");
                ui.horizontal(|ui| {
                    ui.label("Roblox username: evedkdmdj");
                    if ui.button("Copy username").clicked() {
                        ui.ctx().copy_text("evedkdmdj".into());
                    }
                });
            }
            Page::Accounts => self.page_accounts(ui),
            Page::Servers => self.page_servers(ui),
            Page::PrivateServers => self.page_private_servers(ui),
            Page::AutoRejoin => self.page_auto_rejoin(ui),
            Page::AntiAfk => self.page_anti_afk(ui),
            Page::MultiRoblox => self.page_multi(ui),
            Page::Settings => self.page_settings(ui),
        }
    }

    fn overlay(&mut self, ctx: &egui::Context) {
        if let Some(dialog) = self.add_dialog.take() {
            self.add_dialog = pages::add_account::show(self, ctx, dialog);
        }
        if let Some(dialog) = self.rejoin_dialog.take() {
            self.rejoin_dialog = pages::rejoin::show_dialog(self, ctx, dialog);
        }
        if let Some(confirm) = self.confirm.take() {
            let mut keep = true;
            let mut run = false;
            egui::Modal::new(egui::Id::new("confirm")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.label(RichText::new(&confirm.title).size(16.0).strong());
                ui.add_space(6.0);
                ui.label(&confirm.body);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if widgets::ghost_button(ui, &self.palette, "Cancel").clicked() {
                        keep = false;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(RichText::new("Confirm").color(Color32::WHITE))
                                    .fill(self.palette.danger)
                                    .corner_radius(8.0),
                            )
                            .clicked()
                        {
                            run = true;
                            keep = false;
                        }
                    });
                });
            });
            if run {
                (confirm.action)(self);
            } else if keep {
                self.confirm = Some(confirm);
            }
        }
    }

    fn toast_overlay(&mut self, ctx: &egui::Context) {
        let Some((text, error, shown)) = self.toast.clone() else { return };
        if shown.elapsed().as_secs_f32() > 4.0 {
            self.toast = None;
            return;
        }
        let color = if error { self.palette.danger } else { self.palette.online };
        egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -24.0)).show(ctx, |ui| {
            egui::Frame::new()
                .fill(self.palette.elevated)
                .stroke(egui::Stroke::new(1.0, color))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    let icon = if error { egui_phosphor::regular::WARNING_CIRCLE } else { egui_phosphor::regular::CHECK_CIRCLE };
                    ui.label(RichText::new(format!("{icon}  {text}")).color(self.palette.text));
                });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

impl eframe::App for RodonApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.services.heartbeat.store(chrono::Utc::now().timestamp(), std::sync::atomic::Ordering::Release);
        if self.splash_frame(ui) {
            return;
        }
        if let Some(index) = self.smoke_page {
            if let Some((page, _, _)) = Page::NAV.get(index) {
                self.page = *page;
            } else if index == Page::NAV.len() {
                self.detached.insert(Page::Console);
            } else {
                SMOKE_PASSED.store(true, std::sync::atomic::Ordering::Release);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            self.smoke_page = Some(index + 1);
            ctx.request_repaint();
        }
        self.drain_messages(&ctx);
        if self.add_dialog.is_none()
            && let Some(cookie) = self.services.pending_import.lock().unwrap_or_else(|p| p.into_inner()).take()
        {
            self.add_dialog = Some(pages::add_account::AddDialog { cookie, ..Default::default() });
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        let settings = self.core.settings();
        if let Some(size) = ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()))
            && size.x >= 820.0
            && size.y >= 540.0
        {
            self.last_size = [size.x, size.y];
            self.startup_size_pending = false;
        }
        if settings.close_to_tray {
            if self.tray.is_none() {
                self.tray = tray::Tray::new(&ctx).ok();
            }
            if let Some(tray) = &self.tray {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                match tray.poll() {
                    Some(tray::Action::Show) => {
                        self.services.window_visible.store(true, std::sync::atomic::Ordering::Release);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    Some(tray::Action::Quit) => {
                        self.quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    None => {}
                }
                if !self.quitting && ctx.input(|i| i.viewport().close_requested()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.services.window_visible.store(false, std::sync::atomic::Ordering::Release);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
            }
        } else {
            self.tray = None;
        }
        if self.core.revision() != self.revision {
            self.reload_accounts();
        }

        egui::Panel::left("nav")
            .exact_size(190.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(self.palette.surface))
            .show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(self.palette.bg).inner_margin(egui::Margin::same(16))).show(ui, |ui| {
            self.media.paint(ui, settings.appearance.as_ref().map(|t| &t.background));
            if !self.detached.contains(&self.page) {
                if self.last_page != self.page {
                    self.transition = Instant::now();
                    self.last_page = self.page;
                }
                if settings.animations && self.animation_allowed {
                    let alpha = (self.transition.elapsed().as_secs_f32() / 0.15).min(1.0);
                    ui.set_opacity(alpha);
                    if alpha < 1.0 {
                        ui.ctx().request_repaint();
                    }
                }
                self.render_page(ui, self.page);
            }
        });

        for (page, _, label) in Page::NAV {
            if !self.detached.contains(&page) {
                continue;
            }
            let viewport = egui::ViewportId::from_hash_of(page);
            ctx.show_viewport_immediate(
                viewport,
                egui::ViewportBuilder::default().with_title(format!("Rodon · {label}")).with_inner_size([950.0, 640.0]),
                |child, _| {
                    if child.input(|i| i.viewport().close_requested()) {
                        self.detached.remove(&page);
                        self.page = page;
                    } else {
                        egui::CentralPanel::default().show(child, |ui| {
                            self.media.paint(ui, settings.appearance.as_ref().map(|t| &t.background));
                            self.render_page(ui, page);
                        });
                        if self.confirm.is_some()
                            || self.add_dialog.is_some()
                            || self.rejoin_dialog.is_some()
                            || self.roblox_editor.is_some()
                        {
                            child.ctx().send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
                        }
                    }
                },
            );
        }

        self.overlay(&ctx);
        self.show_roblox_editor(&ctx);
        self.show_games(&ctx);
        self.toast_overlay(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.smoke_page.is_none() && !self.startup_size_pending {
            let size = self.last_size;
            let _ = self.core.edit_settings(|s| s.window_size = size);
        }
        if let Some(worker) = self.startup.take() {
            let _ = worker.join();
        }
        self.rejoin.stop_all();
        self.services.stop_all();
        crate::services::instances::restore_hidden();
        crate::services::roblox_settings::unlock_profile();
    }
}

/// Launches the desktop app. Blocks until the window closes.
pub fn run(core: Arc<Core>, services: Arc<Services>, smoke: bool) -> eframe::Result<()> {
    let settings = core.settings();
    let icon = load_icon();
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(if settings.splash { [380.0, 180.0] } else { settings.window_size })
        .with_window_level(if settings.always_on_top { egui::WindowLevel::AlwaysOnTop } else { egui::WindowLevel::Normal })
        .with_min_inner_size(if settings.splash { [320.0, 160.0] } else { [820.0, 540.0] })
        .with_title(format!("{} v{}", crate::APP_NAME, crate::VERSION))
        .with_icon(icon);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "Rodon",
        options,
        Box::new(move |cc| {
            let mut app = RodonApp::new(cc, Arc::clone(&core), Arc::clone(&services));
            if smoke {
                app.smoke_page = Some(0);
                return Ok(Box::new(app));
            }
            let worker = std::thread::spawn(move || services.start_background());
            app.startup = Some(worker);
            Ok(Box::new(app))
        }),
    )
}

fn windows_animations() -> bool {
    let mut enabled = 1i32;
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW(
            windows_sys::Win32::UI::WindowsAndMessaging::SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut enabled as *mut i32).cast(),
            0,
        );
    }
    enabled != 0
}

fn load_icon() -> egui::IconData {
    // Fall back to an empty icon if the bundled asset is missing.
    let bytes = include_bytes!("../../assets/icon.ico");
    match image::load_from_memory(bytes) {
        Ok(image) => {
            let rgba = image.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            egui::IconData { rgba: rgba.into_raw(), width: w, height: h }
        }
        Err(_) => egui::IconData { rgba: vec![0; 4], width: 1, height: 1 },
    }
}

/// Shows the vault unlock window; returns the opened core or None if the user quit.
pub fn unlock_prompt(data_dir: &std::path::Path) -> Option<Arc<Core>> {
    unlock::prompt(data_dir)
}

/// Shows a blocking native error dialog for a fatal startup problem.
#[cfg(windows)]
pub fn fatal_dialog(message: &str) {
    use std::os::windows::ffi::OsStrExt;
    let text: Vec<u16> = std::ffi::OsStr::new(message).encode_wide().chain(std::iter::once(0)).collect();
    let title: Vec<u16> = std::ffi::OsStr::new("Rodon").encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: both strings are null-terminated; MessageBoxW only reads them.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
    }
}

#[cfg(not(windows))]
pub fn fatal_dialog(message: &str) {
    eprintln!("{message}");
}
