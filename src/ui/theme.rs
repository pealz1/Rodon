//! Visual theme: a deliberate dark/light palette, spacing and typography, applied
//! to egui. Colours are tokens, never repeated raw.

use crate::store::settings::ThemeMode;
use eframe::egui::{self, Color32, CornerRadius, Stroke};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub pressed: Color32,
    pub hover: Color32,
    pub selected: Color32,
    pub soft: Color32,
    pub bright: Color32,
    pub deep: Color32,
    pub active_accent: Color32,
    pub hover_accent: Color32,
    pub bg: Color32,
    pub surface: Color32,
    pub surface_alt: Color32,
    pub elevated: Color32,
    pub line: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub online: Color32,
    pub in_game: Color32,
    pub danger: Color32,
    pub warn: Color32,
    pub star: Color32,
}

impl Palette {
    pub fn dark(accent: Color32) -> Palette {
        Palette {
            bg: Color32::from_rgb(0x12, 0x14, 0x1a),
            surface: Color32::from_rgb(0x1a, 0x1d, 0x26),
            surface_alt: Color32::from_rgb(0x20, 0x24, 0x2f),
            elevated: Color32::from_rgb(0x26, 0x2b, 0x38),
            line: Color32::from_rgb(0x2e, 0x34, 0x42),
            text: Color32::from_rgb(0xe9, 0xec, 0xf2),
            muted: Color32::from_rgb(0x9a, 0xa3, 0xb5),
            faint: Color32::from_rgb(0x66, 0x6e, 0x80),
            accent,
            pressed: accent.gamma_multiply(0.65),
            hover: accent.gamma_multiply(0.45),
            selected: accent.gamma_multiply(0.35),
            soft: accent.gamma_multiply(0.55),
            bright: accent.gamma_multiply(1.1),
            deep: accent.gamma_multiply(0.2),
            active_accent: accent.gamma_multiply(0.85),
            hover_accent: accent.gamma_multiply(1.1),
            accent_text: Color32::WHITE,
            online: Color32::from_rgb(0x4a, 0xd2, 0x95),
            in_game: Color32::from_rgb(0x5b, 0x8c, 0xff),
            danger: Color32::from_rgb(0xf0, 0x6a, 0x6a),
            warn: Color32::from_rgb(0xe6, 0xb4, 0x50),
            star: Color32::from_rgb(0xf2, 0xc4, 0x5f),
        }
    }

    pub fn light(accent: Color32) -> Palette {
        Palette {
            bg: Color32::from_rgb(0xf4, 0xf5, 0xf8),
            surface: Color32::from_rgb(0xff, 0xff, 0xff),
            surface_alt: Color32::from_rgb(0xed, 0xef, 0xf4),
            elevated: Color32::from_rgb(0xff, 0xff, 0xff),
            line: Color32::from_rgb(0xd9, 0xdd, 0xe6),
            text: Color32::from_rgb(0x1a, 0x1e, 0x28),
            muted: Color32::from_rgb(0x5a, 0x62, 0x74),
            faint: Color32::from_rgb(0x8a, 0x92, 0xa4),
            accent,
            pressed: accent.gamma_multiply(0.65),
            hover: accent.gamma_multiply(0.45),
            selected: accent.gamma_multiply(0.35),
            soft: accent.gamma_multiply(0.55),
            bright: accent.gamma_multiply(1.1),
            deep: accent.gamma_multiply(0.2),
            active_accent: accent.gamma_multiply(0.85),
            hover_accent: accent.gamma_multiply(1.1),
            accent_text: Color32::WHITE,
            online: Color32::from_rgb(0x1f, 0xa8, 0x6b),
            in_game: Color32::from_rgb(0x3a, 0x6f, 0xe6),
            danger: Color32::from_rgb(0xd1, 0x43, 0x43),
            warn: Color32::from_rgb(0xb8, 0x82, 0x1a),
            star: Color32::from_rgb(0xd9, 0x9e, 0x1f),
        }
    }
}

pub fn parse_hex(hex: &str) -> Option<Color32> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

pub fn to_hex(color: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}

/// Resolves the active palette from settings and the OS dark/light preference.
pub fn palette(mode: ThemeMode, accent_hex: &str, system_dark: bool) -> Palette {
    let accent = parse_hex(accent_hex).unwrap_or(Color32::from_rgb(0x5b, 0x8c, 0xff));
    let dark = match mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => system_dark,
    };
    if dark { Palette::dark(accent) } else { Palette::light(accent) }
}

pub fn resolved(settings: &crate::store::settings::Settings, system_dark: bool) -> Palette {
    let mut p = palette(settings.theme, &settings.accent, system_dark);
    if let Some(t) = &settings.appearance {
        if let Some(c) = t.colors.get("BG").and_then(|s| parse_hex(s)) {
            p.bg = c;
        }
        if let Some(c) = t.colors.get("PANEL").and_then(|s| parse_hex(s)) {
            p.surface = c;
        }
        if let Some(c) = t.colors.get("INPUT").and_then(|s| parse_hex(s)) {
            p.surface_alt = c;
        }
        if let Some(c) = t.colors.get("TEXT").and_then(|s| parse_hex(s)) {
            p.text = c;
        }
        if let Some(c) = t.colors.get("MUTED").and_then(|s| parse_hex(s)) {
            p.muted = c;
        }
        if let Some(c) = t.colors.get("LINE").and_then(|s| parse_hex(s)) {
            p.line = c;
        }
        if let Some(c) = t.colors.get("NOTE").and_then(|s| parse_hex(s)) {
            p.star = c;
        }
        if let Some(c) = t.colors.get("RAISED").and_then(|s| parse_hex(s)) {
            p.elevated = c;
        }
        if let Some(c) = t.colors.get("ACCENT").and_then(|s| parse_hex(s)) {
            p.accent = c;
        }
        if let Some(c) = t.colors.get("ON_ACCENT").and_then(|s| parse_hex(s)) {
            p.accent_text = c;
        }
        if let Some(c) = t.colors.get("ACCENT_TEXT").and_then(|s| parse_hex(s)) {
            p.in_game = c;
        }
        if let Some(c) = t.colors.get("PRESSED").and_then(|s| parse_hex(s)) {
            p.pressed = c;
        }
        if let Some(c) = t.colors.get("HOVER").and_then(|s| parse_hex(s)) {
            p.hover = c;
        }
        if let Some(c) = t.colors.get("SELECT").and_then(|s| parse_hex(s)) {
            p.selected = c;
        }
        if let Some(c) = t.colors.get("ACCENT_SOFT").and_then(|s| parse_hex(s)) {
            p.soft = c;
        }
        if let Some(c) = t.colors.get("ACCENT_BRIGHT").and_then(|s| parse_hex(s)) {
            p.bright = c;
        }
        if let Some(c) = t.colors.get("ACCENT_DEEP").and_then(|s| parse_hex(s)) {
            p.deep = c;
        }
        if let Some(c) = t.colors.get("ACCENT_PRESSED").and_then(|s| parse_hex(s)) {
            p.active_accent = c;
        }
        if let Some(c) = t.colors.get("ACCENT_HOVER").and_then(|s| parse_hex(s)) {
            p.hover_accent = c;
        }
    }
    p
}

/// Applies a palette and the shared spacing/rounding to an egui context.
pub fn apply(ctx: &egui::Context, palette: &Palette, scale: f32) {
    ctx.all_styles_mut(|style| style_palette(style, palette));
    ctx.set_zoom_factor(scale.clamp(0.75, 2.0));
}

fn style_palette(style: &mut egui::Style, palette: &Palette) {
    let visuals = &mut style.visuals;
    visuals.dark_mode = palette.bg.r() < 128;
    visuals.override_text_color = Some(palette.text);
    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.surface;
    visuals.window_stroke = Stroke::new(1.0, palette.line);
    visuals.extreme_bg_color = palette.surface_alt;
    visuals.faint_bg_color = palette.surface_alt;
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.selection.bg_fill = palette.selected;
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.hyperlink_color = palette.bright;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = palette.surface;
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.muted);
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.line);
    for w in [&mut widgets.inactive, &mut widgets.hovered, &mut widgets.active, &mut widgets.open] {
        w.corner_radius = CornerRadius::same(8);
        w.fg_stroke = Stroke::new(1.0, palette.text);
    }
    widgets.inactive.bg_fill = palette.surface_alt;
    widgets.inactive.weak_bg_fill = palette.surface_alt;
    widgets.hovered.bg_fill = palette.hover;
    widgets.hovered.weak_bg_fill = palette.hover;
    widgets.hovered.bg_stroke = Stroke::new(1.0, palette.hover_accent);
    widgets.active.bg_fill = palette.pressed;
    widgets.active.weak_bg_fill = palette.accent.gamma_multiply(0.5);

    let spacing = &mut style.spacing;
    spacing.item_spacing = egui::vec2(8.0, 8.0);
    spacing.button_padding = egui::vec2(10.0, 6.0);
    spacing.menu_margin = egui::Margin::same(6);
    spacing.interact_size.y = 28.0;
    spacing.scroll.bar_width = 9.0;
}

/// Installs the default fonts plus Phosphor icons.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c = parse_hex("#5B8CFF").unwrap();
        assert_eq!(to_hex(c), "#5B8CFF");
        assert!(parse_hex("nope").is_none());
    }

    #[test]
    fn system_mode_follows_os() {
        assert!(palette(ThemeMode::System, "#5B8CFF", true).bg.r() < 128);
        assert!(palette(ThemeMode::System, "#5B8CFF", false).bg.r() > 128);
    }
}
