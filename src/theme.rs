//! Visual theme: palette extraction, fonts and egui styling.
#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

pub const BG: Color32 = Color32::from_rgb(10, 11, 15);
pub const PANEL: Color32 = Color32::from_rgb(15, 17, 23);
pub const CARD: Color32 = Color32::from_rgb(22, 25, 34);
pub const CARD_HOVER: Color32 = Color32::from_rgb(30, 34, 46);
pub const BORDER: Color32 = Color32::from_rgb(38, 43, 56);
pub const TEXT: Color32 = Color32::from_rgb(231, 235, 244);
pub const MUTED: Color32 = Color32::from_rgb(148, 156, 174);
pub const DANGER: Color32 = Color32::from_rgb(235, 94, 94);

pub const ICON_PLAY: &str = "\u{E768}";
pub const ICON_PAUSE: &str = "\u{E769}";
pub const ICON_NEXT: &str = "\u{E893}";
pub const ICON_PREV: &str = "\u{E892}";
pub const ICON_SETTINGS: &str = "\u{E713}";
pub const ICON_STAR: &str = "\u{E734}";
pub const ICON_STAR_FILLED: &str = "\u{E735}";
pub const ICON_PICTURES: &str = "\u{E8B9}";
pub const ICON_GLOBE: &str = "\u{E774}";
pub const ICON_MONITOR: &str = "\u{E7F4}";
pub const ICON_REFRESH: &str = "\u{E72C}";
pub const ICON_ADD: &str = "\u{E710}";
pub const ICON_DELETE: &str = "\u{E74D}";
pub const ICON_FOLDER: &str = "\u{E8B7}";
pub const ICON_SEARCH: &str = "\u{E721}";
pub const ICON_CLOSE: &str = "\u{E711}";
pub const ICON_VOLUME: &str = "\u{E767}";
pub const ICON_MUTE: &str = "\u{E74F}";
pub const ICON_INFO: &str = "\u{E946}";
pub const ICON_DOWNLOAD: &str = "\u{E896}";
pub const ICON_STOP: &str = "\u{E71A}";
pub const ICON_CHEVRON_LEFT: &str = "\u{E76B}";
pub const ICON_CHEVRON_RIGHT: &str = "\u{E76C}";
pub const ICON_OPEN: &str = "\u{E8A7}";
pub const ICON_SPARKLE: &str = "\u{E945}";

pub fn icon(text: &str) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(15.0, FontFamily::Name("icons".into())))
}

pub fn icon_large(text: &str, size: f32) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(size, FontFamily::Name("icons".into())))
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    for (name, path) in [
        ("segoe", r"C:\Windows\Fonts\segoeui.ttf"),
        ("segoe-semibold", r"C:\Windows\Fonts\seguisb.ttf"),
        ("segoe-bold", r"C:\Windows\Fonts\segoeuib.ttf"),
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert(name.to_string(), Arc::new(egui::FontData::from_owned(bytes)));
        }
    }

    if fonts.font_data.contains_key("segoe") {
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "segoe".to_string());
    }

    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segmdl2.ttf") {
        fonts.font_data.insert(
            "mdl2".to_string(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts.families.insert(
            FontFamily::Name("icons".into()),
            vec!["mdl2".to_string()],
        );
    }

    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(26.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.scroll.bar_width = 8.0;
    ctx.set_style(style);
}

pub fn apply(ctx: &egui::Context, accent: Color32) {
    let mut visuals = Visuals::dark();

    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(7, 8, 11);
    visuals.faint_bg_color = CARD;
    visuals.code_bg_color = CARD;
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = accent;
    visuals.warn_fg_color = Color32::from_rgb(240, 190, 90);
    visuals.error_fg_color = DANGER;
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.selection.bg_fill = accent.gamma_multiply(0.38);
    visuals.selection.stroke = Stroke::new(1.0_f32, accent);

    visuals.widgets.noninteractive.bg_fill = CARD;
    visuals.widgets.noninteractive.weak_bg_fill = CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(10);

    visuals.widgets.inactive.bg_fill = CARD;
    visuals.widgets.inactive.weak_bg_fill = CARD;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(10);

    visuals.widgets.hovered.bg_fill = CARD_HOVER;
    visuals.widgets.hovered.weak_bg_fill = CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, accent.gamma_multiply(0.75));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(10);

    visuals.widgets.active.bg_fill = accent.gamma_multiply(0.55);
    visuals.widgets.active.weak_bg_fill = accent.gamma_multiply(0.55);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(10);

    visuals.widgets.open.bg_fill = CARD_HOVER;
    visuals.widgets.open.weak_bg_fill = CARD_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.open.corner_radius = CornerRadius::same(10);

    ctx.set_visuals(visuals);
}

pub fn accent_from_path(path: &Path) -> Option<(Color32, bool)> {
    let image = image::open(path).ok()?;
    Some(accent_from_dynamic(&image))
}

pub fn accent_from_dynamic(image: &image::DynamicImage) -> (Color32, bool) {
    let rgb = image.to_rgb8();
    let sample = image::imageops::resize(&rgb, 96, 96, image::imageops::FilterType::Nearest);

    let mut best_vibrancy = -1.0f32;
    let mut best = (255u8, 132u8, 64u8);
    let mut total_lum = 0.0f64;

    for pixel in sample.pixels() {
        let r = pixel[0] as f32 / 255.0;
        let g = pixel[1] as f32 / 255.0;
        let b = pixel[2] as f32 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let sat = if max <= 0.0 { 0.0 } else { delta / max };
        let value = max;
        if value > 0.15 && value < 0.97 {
            let vibrancy = sat * value;
            if vibrancy > best_vibrancy {
                best_vibrancy = vibrancy;
                best = (pixel[0], pixel[1], pixel[2]);
            }
        }
        total_lum += 0.299 * pixel[0] as f64 + 0.587 * pixel[1] as f64 + 0.114 * pixel[2] as f64;
    }

    let count = (sample.width() * sample.height()) as f64;
    let is_dark = total_lum / count.max(1.0) < 128.0;

    let (mut r, mut g, mut b) = best;
    if best_vibrancy < 0.06 {
        // Neutral slate for monochrome art.
        if is_dark {
            r = 115;
            g = 140;
            b = 166;
        } else {
            r = 89;
            g = 115;
            b = 140;
        }
    }

    (boost(Color32::from_rgb(r, g, b)), is_dark)
}

/// Raises saturation/lightness a little so the accent pops on dark surfaces.
fn boost(color: Color32) -> Color32 {
    let [r, g, b, _] = color.to_array();
    let max = r.max(g).max(b) as f32;
    let min = r.min(g).min(b) as f32;
    if max <= 1.0 {
        return color;
    }
    let mid = (max + min) / 2.0;
    let boost = (200.0 - mid) * 0.35;
    Color32::from_rgb(
        (r as f32 + boost).clamp(0.0, 255.0) as u8,
        (g as f32 + boost).clamp(0.0, 255.0) as u8,
        (b as f32 + boost).clamp(0.0, 255.0) as u8,
    )
}

pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(122, 162, 247);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_extracts_a_strong_color() {
        let mut img = image::RgbImage::new(32, 32);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = if x < 16 {
                image::Rgb([220, 40, 40])
            } else {
                image::Rgb([10, 10, 10])
            };
            let _ = y;
        }
        let (color, _) = accent_from_dynamic(&image::DynamicImage::ImageRgb8(img));
        assert!(color.r() > color.g());
    }
}

