//! The Add / edit dialog: several ways to add accounts, plus note editing.

use crate::import;
use crate::store::model::Account;
use crate::ui::RodonApp;
use crate::ui::task::{Msg, QuickLoginMsg};
use crate::ui::widgets;
use eframe::egui::{self, RichText};
use std::sync::Arc;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Tab {
    Create,
    Cookie,
    UserPass,
    Quick,
    Import,
    Note,
}

pub struct AddDialog {
    pub approval: Arc<()>,
    pub creator: crate::ui::signup::Creator,
    pub tab: Tab,
    pub cookie: String,
    pub user: String,
    pub pass: String,
    pub import_text: String,
    pub busy: bool,
    pub quick_code: Option<String>,
    pub note_target: Option<String>,
    pub note: String,
}

impl Default for AddDialog {
    fn default() -> AddDialog {
        AddDialog {
            tab: Tab::Cookie,
            creator: Default::default(),
            approval: Arc::new(()),
            cookie: String::new(),
            user: String::new(),
            pass: String::new(),
            import_text: String::new(),
            busy: false,
            quick_code: None,
            note_target: None,
            note: String::new(),
        }
    }
}

impl AddDialog {
    pub fn note(account: &Account) -> AddDialog {
        AddDialog { tab: Tab::Note, note_target: Some(account.key()), note: account.note.clone(), ..Default::default() }
    }
}

/// Renders the dialog; returns the dialog to keep it open, or None to close.
pub fn show(app: &mut RodonApp, ctx: &egui::Context, mut dialog: AddDialog) -> Option<AddDialog> {
    let mut keep = true;
    let title = if dialog.tab == Tab::Note { "Edit note" } else { "Add accounts" };
    egui::Modal::new(egui::Id::new("add-account")).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).size(17.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(egui_phosphor::regular::X).clicked() {
                    keep = false;
                }
            });
        });
        ui.add_space(8.0);

        if dialog.tab == Tab::Note {
            keep &= note_body(app, ui, &mut dialog);
        } else {
            tab_bar(app, ui, &mut dialog);
            ui.add_space(10.0);
            keep &= match dialog.tab {
                Tab::Create => crate::ui::signup::show(app, ui, &mut dialog),
                Tab::Cookie => cookie_body(app, ui, &mut dialog),
                Tab::UserPass => user_pass_body(app, ui, &mut dialog),
                Tab::Quick => quick_body(app, ui, &mut dialog),
                Tab::Import => import_body(app, ui, &mut dialog),
                Tab::Note => true,
            };
        }
    });
    keep.then_some(dialog)
}

fn tab_bar(app: &RodonApp, ui: &mut egui::Ui, dialog: &mut AddDialog) {
    ui.horizontal(|ui| {
        for (tab, label) in [
            (Tab::Cookie, "Cookie"),
            (Tab::UserPass, "Browser"),
            (Tab::Create, "Create"),
            (Tab::Quick, "Quick sign-in"),
            (Tab::Import, "Import"),
        ] {
            let selected = dialog.tab == tab;
            let (fg, bg) =
                if selected { (app.palette.accent_text, app.palette.accent) } else { (app.palette.muted, app.palette.surface_alt) };
            if ui.add(egui::Button::new(RichText::new(label).size(12.0).color(fg)).fill(bg).corner_radius(8.0)).clicked() {
                dialog.tab = tab;
            }
        }
    });
}

fn cookie_body(app: &mut RodonApp, ui: &mut egui::Ui, dialog: &mut AddDialog) -> bool {
    ui.label(RichText::new("Paste one or more .ROBLOSECURITY cookies.").size(12.0).color(app.palette.muted));
    ui.add_space(6.0);
    ui.add(
        egui::TextEdit::multiline(&mut dialog.cookie)
            .desired_rows(4)
            .desired_width(f32::INFINITY)
            .hint_text("_|WARNING:-DO-NOT-SHARE-THIS…"),
    );
    ui.add_space(10.0);
    action_row(app, ui, dialog, "Add", |app, dialog| {
        let cookies = import::find_cookies(&dialog.cookie);
        if cookies.is_empty() {
            app.show_toast("No valid cookie found", true);
            return false;
        }
        app.sender.spawn(Arc::clone(&app.core), Arc::clone(&app.services), move |core, _| {
            let mut added = 0;
            let mut last = String::new();
            for cookie in &cookies {
                match core.add_cookie(cookie) {
                    Ok(account) => {
                        added += 1;
                        last = account.username;
                    }
                    Err(err) => last = err.message,
                }
            }
            if added > 0 {
                Msg::Run(Box::new(move |app| {
                    app.reload_accounts();
                    app.show_toast(format!("Added {added} account(s)"), false);
                }))
            } else {
                Msg::Toast(format!("Could not add account: {last}"), true)
            }
        });
        true
    })
}

fn user_pass_body(app: &mut RodonApp, ui: &mut egui::Ui, dialog: &mut AddDialog) -> bool {
    ui.label("Sign in on Roblox, then approve the code shown in Quick sign-in.");
    if widgets::primary_button(ui, &app.palette, "Open Roblox sign-in").clicked() {
        open_roblox(app, ui, "https://www.roblox.com/login");
        dialog.tab = Tab::Quick;
    }
    if ui.button("Create an account on Roblox").clicked() {
        open_roblox(app, ui, "https://www.roblox.com/");
    }
    true
}

pub(in crate::ui) fn open_roblox(app: &mut RodonApp, ui: &egui::Ui, url: &'static str) {
    let preference = app.core.settings().browser;
    if preference.is_empty() || preference == "auto" {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    } else if let Some(browser) = crate::browser::discover::available().into_iter().find(|b| b.label == preference) {
        if let Err(err) = std::process::Command::new(browser.exe).arg(url).spawn() {
            app.show_toast(err.to_string(), true);
        }
    } else {
        app.show_toast("The selected browser is unavailable. Choose another in Settings.", true);
    }
}

fn quick_body(app: &mut RodonApp, ui: &mut egui::Ui, dialog: &mut AddDialog) -> bool {
    ui.label(
        RichText::new("On your signed-in Roblox device, open Settings → Quick Login and enter this code.")
            .size(12.0)
            .color(app.palette.muted),
    );
    ui.add_space(10.0);
    if let Some(code) = &dialog.quick_code {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(code).size(30.0).strong().monospace().color(app.palette.accent));
            ui.add_space(4.0);
            ui.label(RichText::new("Waiting for approval…").size(12.0).color(app.palette.muted));
            if ui.button("Open Roblox to approve").clicked() {
                open_roblox(app, ui, "https://www.roblox.com/crossdevicelogin/confirmcode");
            }
            ui.add(egui::Spinner::new());
        });
    } else if dialog.busy {
        ui.vertical_centered(|ui| {
            ui.add(egui::Spinner::new());
        });
    } else if widgets::primary_button(ui, &app.palette, "Start quick sign-in").clicked() {
        dialog.busy = true;
        start_quick_login(app, Arc::downgrade(&dialog.approval));
    }
    ui.add_space(8.0);
    true
}

fn import_body(app: &mut RodonApp, ui: &mut egui::Ui, dialog: &mut AddDialog) -> bool {
    ui.label(
        RichText::new("Paste cookies, user:pass lines, CSV or JSON — or import a file from another manager.")
            .size(12.0)
            .color(app.palette.muted),
    );
    ui.add_space(6.0);
    ui.add(
        egui::TextEdit::multiline(&mut dialog.import_text)
            .desired_rows(4)
            .desired_width(f32::INFINITY)
            .hint_text("paste here, or use the file buttons below"),
    );
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if widgets::ghost_button(ui, &app.palette, &format!("{} Text / CSV / JSON", egui_phosphor::regular::FILE_TEXT)).clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Account files", &["txt", "csv", "json"]).pick_file()
        {
            import_file(app, &path, None);
            return;
        }
        ui.add(egui::TextEdit::singleline(&mut dialog.pass).password(true).desired_width(96.0).hint_text("File password"));
        if widgets::ghost_button(ui, &app.palette, &format!("{} Evanovar / ic3w0lf / backup", egui_phosphor::regular::DATABASE)).clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Manager files", &["json", "rodonbackup", "novabackup"]).pick_file()
        {
            import_manager_file(app, &path, &dialog.pass);
        }
    });
    ui.add_space(10.0);
    action_row(app, ui, dialog, "Import pasted", |app, dialog| {
        let batch = import::parse_text(&dialog.import_text);
        resolve_and_apply(app, batch);
        true
    })
}

fn note_body(app: &mut RodonApp, ui: &mut egui::Ui, dialog: &mut AddDialog) -> bool {
    ui.add(egui::TextEdit::multiline(&mut dialog.note).desired_rows(3).desired_width(f32::INFINITY));
    ui.add_space(10.0);
    let mut keep = true;
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, &app.palette, "Save").clicked() {
            if let Some(target) = &dialog.note_target {
                let keys = if app.selection.contains(target) { app.selected_keys() } else { vec![target.clone()] };
                let result = app.core.edit(|data| {
                    for key in &keys {
                        if let Some(account) = data.find_mut(key) {
                            account.note.clone_from(&dialog.note);
                        }
                    }
                });
                if let Err(err) = result {
                    app.show_toast(err.message, true);
                    return;
                }
                app.reload_accounts();
            }
            keep = false;
        }
        if widgets::ghost_button(ui, &app.palette, "Cancel").clicked() {
            keep = false;
        }
    });
    keep
}

/// A Cancel + primary action row. The action returns whether to close the dialog.
fn action_row(
    app: &mut RodonApp,
    ui: &mut egui::Ui,
    dialog: &mut AddDialog,
    label: &str,
    action: impl FnOnce(&mut RodonApp, &mut AddDialog) -> bool,
) -> bool {
    let mut keep = true;
    ui.horizontal(|ui| {
        if widgets::primary_button(ui, &app.palette, label).clicked() {
            keep = !action(app, dialog);
        }
        if widgets::ghost_button(ui, &app.palette, "Cancel").clicked() {
            keep = false;
        }
    });
    keep
}

fn import_file(app: &mut RodonApp, path: &std::path::Path, _hint: Option<()>) {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let batch = import::parse_text(&text);
            resolve_and_apply(app, batch);
        }
        Err(err) => app.show_toast(format!("Could not read file: {err}"), true),
    }
}

fn import_manager_file(app: &mut RodonApp, path: &std::path::Path, password: &str) {
    let path = path.to_path_buf();
    // Try Evanovar, then ic3w0lf, then backup; password-locked files report what is needed.
    let pw = (!password.is_empty()).then_some(password);
    let header = std::fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    let batch = match header {
        Some(ref data) if data.get("format").is_some() => import::backup::import(&path, password),
        Some(ref data) if data.is_object() => import::evanovar::read_accounts(&path, pw, None),
        _ => import::ic3w0lf::read_accounts(&path, pw),
    };
    match batch {
        Ok(mut batch) => {
            if let Some(dir) = path.parent() {
                import::evanovar::read_side_files(dir, &mut batch);
            }
            resolve_and_apply(app, batch);
        }
        Err(err) => app.show_toast(format!("{}: {}", err.title, err.message), true),
    }
}

/// Resolves usernames/ids for cookie accounts (off-thread) and applies the import.
fn resolve_and_apply(app: &mut RodonApp, batch: import::ImportBatch) {
    if batch.with_cookies() == 0 {
        if batch.needs_sign_in().is_empty() {
            app.show_toast("No importable accounts found", true);
        } else {
            app.show_toast(format!("{} account(s) need individual approval with Quick Sign-in", batch.needs_sign_in().len()), true);
        }
        return;
    }
    app.sender.spawn(Arc::clone(&app.core), Arc::clone(&app.services), move |core, _| {
        // Resolve identity for cookie-only accounts so they get a username and id.
        let mut resolved = batch.clone();
        for account in &mut resolved.accounts {
            if account.has_cookie()
                && account.user_id == 0
                && let Ok(identity) = crate::roblox::account::whoami(&account.cookie)
            {
                account.user_id = identity.user_id;
                if account.username.is_empty() {
                    account.username = identity.username;
                }
            }
        }
        match core.apply_import(&resolved) {
            Ok(outcome) => Msg::Run(Box::new(move |app| {
                app.reload_accounts();
                app.show_toast(
                    format!(
                        "Imported {} added, {} updated, {} skipped",
                        outcome.added,
                        outcome.updated,
                        outcome.skipped + outcome.need_sign_in
                    ),
                    false,
                );
            })),
            Err(err) => Msg::Toast(err.message, true),
        }
    });
    app.show_toast("Importing…", false);
}

fn start_quick_login(app: &mut RodonApp, approval: std::sync::Weak<()>) {
    let sender = app.sender.clone();
    let core = Arc::clone(&app.core);
    std::thread::Builder::new()
        .name("quick-login".into())
        .spawn(move || {
            use crate::roblox::account::{self, QuickStatus};
            let send = |msg: QuickLoginMsg| {
                let expected = approval.clone();
                sender.post(Msg::Run(Box::new(move |app| {
                    if app.add_dialog.as_ref().is_some_and(|d| expected.ptr_eq(&Arc::downgrade(&d.approval))) {
                        on_quick_login(app, msg);
                    }
                })));
            };
            let login = match account::quick_login_start() {
                Ok(login) => login,
                Err(err) => {
                    send(QuickLoginMsg::Failed(err.message));
                    return;
                }
            };
            send(QuickLoginMsg::Code(login.code.clone()));
            loop {
                std::thread::sleep(std::time::Duration::from_secs(4));
                if approval.strong_count() == 0 {
                    return;
                }
                match account::quick_login_poll(&login) {
                    Ok(QuickStatus::Validated(_)) => break,
                    Ok(QuickStatus::Cancelled) => {
                        send(QuickLoginMsg::Failed("Sign-in was cancelled or timed out".into()));
                        return;
                    }
                    Ok(QuickStatus::Pending) => continue,
                    Err(_) => continue,
                }
            }
            if approval.strong_count() == 0 {
                return;
            }
            match account::quick_login_redeem(&login).and_then(|cookie| core.add_cookie(&cookie)) {
                Ok(account) => send(QuickLoginMsg::Added(account.username)),
                Err(err) => send(QuickLoginMsg::Failed(err.message)),
            }
        })
        .ok();
}

/// Handles quick-login progress messages posted from the poller thread.
pub fn on_quick_login(app: &mut RodonApp, msg: QuickLoginMsg) {
    match msg {
        QuickLoginMsg::Code(code) => {
            if let Some(dialog) = app.add_dialog.as_mut() {
                dialog.quick_code = Some(code);
                dialog.busy = false;
            }
        }
        QuickLoginMsg::Added(username) => {
            app.reload_accounts();
            app.show_toast(format!("Signed in as {username}"), false);
            app.add_dialog = None;
        }
        QuickLoginMsg::Failed(message) => {
            app.show_toast(message, true);
            if let Some(dialog) = app.add_dialog.as_mut() {
                dialog.quick_code = None;
                dialog.busy = false;
            }
        }
    }
}
