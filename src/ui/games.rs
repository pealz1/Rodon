use super::{RodonApp, task::Msg};
use crate::roblox::games::{GameInfo, icons, search};
use eframe::egui;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub(super) struct Games {
    query: String,
    session: String,
    current: String,
    previous: Vec<String>,
    next: String,
    rows: Vec<GameInfo>,
    textures: HashMap<u64, egui::TextureHandle>,
    busy: bool,
    grid: bool,
    request: u64,
    error: String,
}
impl RodonApp {
    pub(super) fn open_games(&mut self) {
        self.games = Some(Games {
            session: format!(
                "{:08x}-{:04x}-4000-8000-{:012x}",
                rand::random::<u32>(),
                rand::random::<u16>(),
                rand::random::<u64>() & 0xffffffffffff
            ),
            grid: true,
            ..Default::default()
        });
        self.load_games();
    }
    fn load_games(&mut self) {
        let Some(g) = &mut self.games else { return };
        g.busy = true;
        g.error.clear();
        g.rows.clear();
        g.textures.clear();
        g.request = rand::random();
        let (query, id, page, key) = (g.query.clone(), g.session.clone(), g.current.clone(), g.request);
        let post = self.sender.clone();
        self.sender.spawn(Arc::clone(&self.core), Arc::clone(&self.services), move |_, _| match search(&query, &id, &page) {
            Ok((rows, next)) => {
                let ids = rows.iter().map(|r| r.universe_id).collect::<Vec<_>>();
                post.post(Msg::Run(Box::new(move |app| {
                    if let Some(g) = app.games.as_mut().filter(|g| g.request == key) {
                        g.rows = rows;
                        g.next = next;
                        g.busy = false;
                    }
                })));
                for (id, url) in icons(&ids) {
                    if !url.starts_with("https://") || !url[8..].split('/').next().is_some_and(|h| h.ends_with(".rbxcdn.com")) {
                        continue;
                    }
                    let Some(img) = crate::roblox::agent()
                        .get(&url)
                        .call()
                        .ok()
                        .and_then(|mut r| r.body_mut().with_config().limit(2 * 1024 * 1024).read_to_vec().ok())
                        .and_then(|v| image::load_from_memory(&v).ok())
                    else {
                        continue;
                    };
                    let img = img.thumbnail(160, 160).to_rgba8();
                    let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                    let ctx = post.context().clone();
                    post.post(Msg::Run(Box::new(move |app| {
                        if let Some(g) = app.games.as_mut().filter(|g| g.request == key) {
                            g.textures.insert(id, ctx.load_texture(format!("game-{id}"), color, egui::TextureOptions::LINEAR));
                        }
                    })));
                }
                Msg::Run(Box::new(|_| {}))
            }
            Err(err) => Msg::Run(Box::new(move |app| {
                if let Some(g) = app.games.as_mut().filter(|g| g.request == key) {
                    g.error = err.message;
                    g.busy = false;
                }
            })),
        });
    }
    pub(super) fn show_games(&mut self, ctx: &egui::Context) {
        let Some(mut g) = self.games.take() else { return };
        let mut keep = true;
        let mut load = false;
        let mut choice = None;
        egui::Window::new("Games").open(&mut keep).default_size([610.0, 450.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let field = ui.add(egui::TextEdit::singleline(&mut g.query).hint_text("Search games"));
                if ui.button("Search").clicked() || (field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    g.previous.clear();
                    g.current.clear();
                    load = true;
                }
                ui.checkbox(&mut g.grid, "Thumbnail grid");
            });
            if g.busy {
                ui.spinner();
            } else if !g.error.is_empty() {
                ui.label(&g.error);
            } else if g.rows.is_empty() {
                ui.label("No games found.");
            }
            egui::ScrollArea::vertical().max_height(345.0).show(ui, |ui| {
                let cols = if g.grid { (ui.available_width() / 125.0).floor().clamp(1.0, 8.0) as usize } else { 1 };
                egui::Grid::new("game-picker").num_columns(cols).show(ui, |ui| {
                    for (index, row) in g.rows.iter().enumerate() {
                        ui.vertical(|ui| {
                            if g.grid {
                                if let Some(t) = g.textures.get(&row.universe_id)
                                    && ui.add(egui::Button::image(egui::Image::new((t.id(), egui::vec2(98.0, 98.0))))).clicked()
                                {
                                    choice = Some(row.clone());
                                }
                                ui.set_max_width(116.0);
                            }
                            if ui.button(&row.name).clicked() {
                                choice = Some(row.clone());
                            }
                        });
                        if (index + 1) % cols == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(!g.busy && !g.previous.is_empty(), egui::Button::new("Previous")).clicked() {
                    g.current = g.previous.pop().unwrap_or_default();
                    load = true;
                }
                if ui.add_enabled(!g.busy && !g.next.is_empty(), egui::Button::new("Next")).clicked() {
                    g.previous.push(g.current.clone());
                    g.current = g.next.clone();
                    load = true;
                }
            });
        });
        if let Some(game) = choice {
            self.launch.place_id = game.place_id.to_string();
            self.launch.game_name = game.name;
            return;
        }
        if keep {
            self.games = Some(g);
            if load {
                self.load_games();
            }
        }
    }
}
