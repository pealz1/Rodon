use super::{
    RodonApp,
    pages::add_account::{AddDialog, open_roblox},
};
use eframe::egui;
use rand::{Rng, distr::Alphanumeric};

#[derive(Default)]
pub struct Creator {
    pub prefix: String,
    pub shared: String,
    pub count: u16,
    pub rows: Vec<(String, String)>,
}
pub fn show(app: &mut RodonApp, ui: &mut egui::Ui, d: &mut AddDialog) -> bool {
    let c = &mut d.creator;
    c.count = c.count.max(1);
    ui.label("Prepare account details, then complete each registration on Roblox.");
    ui.add(egui::TextEdit::singleline(&mut c.prefix).hint_text("Username prefix"));
    ui.add(egui::TextEdit::singleline(&mut c.shared).password(true).hint_text("Shared password (optional)"));
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut c.count).range(1..=100).prefix("Accounts "));
        if ui.button("Generate details").clicked() {
            let prefix: String = c.prefix.chars().filter(char::is_ascii_alphanumeric).take(12).collect();
            let generate = |length| rand::rng().sample_iter(&Alphanumeric).take(length).map(char::from).collect::<String>();
            c.rows = (0..c.count.clamp(1, 100))
                .map(|_| (format!("{prefix}{}", generate(8)), if c.shared.is_empty() { generate(22) } else { c.shared.clone() }))
                .collect();
        }
    });
    egui::ScrollArea::vertical().max_height(230.0).show(ui, |ui| {
        for (user, pw) in &c.rows {
            ui.horizontal(|ui| {
                ui.label(user);
                if ui.button("Copy name").clicked() {
                    ui.ctx().copy_text(user.clone());
                }
                if ui.button("Copy password").clicked() {
                    ui.ctx().copy_text(pw.clone());
                }
                if ui.button("Register").clicked() {
                    open_roblox(app, ui, "https://www.roblox.com/");
                }
            });
        }
    });
    true
}
