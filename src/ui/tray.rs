use eframe::egui;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem},
};

pub(super) enum Action {
    Show,
    Quit,
}
pub(super) struct Tray {
    _icon: TrayIcon,
    quit_id: tray_icon::menu::MenuId,
}

impl Tray {
    pub(super) fn new(_ctx: &egui::Context) -> Result<Self, Box<dyn std::error::Error>> {
        let menu = Menu::new();
        let show = MenuItem::new("Show Rodon", true, None);
        let quit = MenuItem::new("Exit", true, None);
        menu.append_items(&[&show, &quit])?;
        let image = super::load_icon();
        let icon = Icon::from_rgba(image.rgba, image.width, image.height)?;
        let icon = TrayIconBuilder::new().with_tooltip("Rodon").with_menu(Box::new(menu)).with_icon(icon).build()?;
        Ok(Self { _icon: icon, quit_id: quit.id().clone() })
    }
    pub(super) fn poll(&self) -> Option<Action> {
        if let Ok(e) = MenuEvent::receiver().try_recv() {
            return Some(if e.id == self.quit_id { Action::Quit } else { Action::Show });
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                return Some(Action::Show);
            }
        }
        None
    }
}
