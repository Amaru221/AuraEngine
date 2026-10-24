//! Visual theme: palette, fonts and egui styling.
#![allow(dead_code)]

use std::path::Path;
use std::sync::{Arc, RwLock};

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};

use crate::config::ThemePreset;

/// A complete set of surface/text colors. The accent is kept separate so it
/// can follow the wallpaper or a user-picked color.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub danger: Color32,
    pub light: bool,
}

impl Palette {
    pub const DARK: Self = Self {
        bg: Color32::from_rgb(10, 11, 15),
        panel: Color32::from_rgb(15, 17, 23),
        card: Color32::from_rgb(22, 25, 34),
        card_hover: Color32::from_rgb(30, 34, 46),
        border: Color32::from_rgb(38, 43, 56),
        text: Color32::from_rgb(231, 235, 244),
        muted: Color32::from_rgb(148, 156, 174),
        danger: Color32::from_rgb(235, 94, 94),
        light: false,
    };

    pub const MIDNIGHT: Self = Self {
        bg: Color32::from_rgb(6, 9, 20),
        panel: Color32::from_rgb(10, 14, 28),
        card: Color32::from_rgb(16, 22, 40),
        card_hover: Color32::from_rgb(23, 31, 54),
        border: Color32::from_rgb(34, 45, 74),
        text: Color32::from_rgb(226, 233, 248),
        muted: Color32::from_rgb(137, 150, 178),
        danger: Color32::from_rgb(240, 100, 110),
        light: false,
    };

    pub const LIGHT: Self = Self {
        bg: Color32::from_rgb(244, 246, 250),
        panel: Color32::from_rgb(252, 253, 255),
        card: Color32::from_rgb(255, 255, 255),
        card_hover: Color32::from_rgb(238, 242, 249),
        border: Color32::from_rgb(210, 216, 228),
        text: Color32::from_rgb(28, 32, 42),
        muted: Color32::from_rgb(100, 108, 124),
        danger: Color32::from_rgb(200, 55, 65),
        light: true,
    };

    /// Derives a coherent palette from a custom background color.
    pub fn custom(bg: Color32) -> Self {
        let light = luminance(bg) > 0.55;
        let (panel_amount, card_amount, hover_amount, border_amount) = if light {
            (-0.04, 0.05, 0.09, -0.16)
        } else {
            (0.04, 0.08, 0.14, 0.18)
        };
        Self {
            bg,
            panel: shade(bg, panel_amount),
            card: shade(bg, card_amount),
            card_hover: shade(bg, hover_amount),
            border: shade(bg, border_amount),
            text: if light {
                Color32::from_rgb(26, 30, 40)
            } else {
                Color32::from_rgb(231, 235, 244)
            },
            muted: if light {
                Color32::from_rgb(96, 104, 120)
            } else {
                Color32::from_rgb(148, 156, 174)
            },
            danger: if light {
                Color32::from_rgb(200, 55, 65)
            } else {
                Color32::from_rgb(235, 94, 94)
            },
            light,
        }
    }
}

fn luminance(color: Color32) -> f32 {
    let [r, g, b, _] = color.to_array();
    (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32) / 255.0
}

fn shade(color: Color32, amount: f32) -> Color32 {
    let [r, g, b, _] = color.to_array();
    let adjust = |c: u8| {
        let c = c as f32;
        if amount >= 0.0 {
            (c + (255.0 - c) * amount).clamp(0.0, 255.0) as u8
        } else {
            (c * (1.0 + amount)).clamp(0.0, 255.0) as u8
        }
    };
    Color32::from_rgb(adjust(r), adjust(g), adjust(b))
}

static PALETTE: RwLock<Palette> = RwLock::new(Palette::DARK);

pub fn set_preset(preset: ThemePreset, custom_bg: Option<[u8; 3]>) {
    let palette = match preset {
        ThemePreset::Dark => Palette::DARK,
        ThemePreset::Midnight => Palette::MIDNIGHT,
        ThemePreset::Light => Palette::LIGHT,
        ThemePreset::Custom => {
            let [r, g, b] = custom_bg.unwrap_or([18, 20, 28]);
            Palette::custom(Color32::from_rgb(r, g, b))
        }
    };
    *PALETTE.write().unwrap_or_else(|e| e.into_inner()) = palette;
}

pub fn palette() -> Palette {
    *PALETTE.read().unwrap_or_else(|e| e.into_inner())
}

pub fn bg() -> Color32 {
    palette().bg
}
pub fn panel() -> Color32 {
    palette().panel
}
pub fn card() -> Color32 {
    palette().card
}
pub fn card_hover() -> Color32 {
    palette().card_hover
}
pub fn border() -> Color32 {
    palette().border
}
pub fn text() -> Color32 {
    palette().text
}
pub fn muted() -> Color32 {
    palette().muted
}
pub fn danger() -> Color32 {
    palette().danger
}

/// Black or white, whichever is more legible on `color`.
pub fn contrast_on(color: Color32) -> Color32 {
    if luminance(color) > 0.55 {
        Color32::from_rgb(14, 16, 22)
    } else {
        Color32::WHITE
    }
}

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
pub const ICON_UPDATE: &str = "\u{E895}";

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
    let palette = palette();
    let mut visuals = if palette.light {
        Visuals::light()
    } else {
        Visuals::dark()
    };

    visuals.panel_fill = palette.bg;
    visuals.window_fill = palette.panel;
    visuals.extreme_bg_color = if palette.light {
        palette.card
    } else {
        Color32::from_rgb(7, 8, 11)
    };
    visuals.faint_bg_color = palette.card;
    visuals.code_bg_color = palette.card;
    visuals.override_text_color = Some(palette.text);
    visuals.hyperlink_color = accent;
    visuals.warn_fg_color = Color32::from_rgb(240, 190, 90);
    visuals.error_fg_color = palette.danger;
    visuals.window_stroke = Stroke::new(1.0_f32, palette.border);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.selection.bg_fill = accent.gamma_multiply(0.38);
    visuals.selection.stroke = Stroke::new(1.0_f32, accent);

    visuals.widgets.noninteractive.bg_fill = palette.card;
    visuals.widgets.noninteractive.weak_bg_fill = palette.card;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, palette.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, palette.text);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(10);

    visuals.widgets.inactive.bg_fill = palette.card;
    visuals.widgets.inactive.weak_bg_fill = palette.card;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, palette.text);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(10);

    visuals.widgets.hovered.bg_fill = palette.card_hover;
    visuals.widgets.hovered.weak_bg_fill = palette.card_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, accent.gamma_multiply(0.75));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, palette.text);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(10);

    visuals.widgets.active.bg_fill = accent.gamma_multiply(0.55);
    visuals.widgets.active.weak_bg_fill = accent.gamma_multiply(0.55);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.active.corner_radius = CornerRadius::same(10);

    visuals.widgets.open.bg_fill = palette.card_hover;
    visuals.widgets.open.weak_bg_fill = palette.card_hover;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, palette.border);
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

    #[test]
    fn custom_palettes_pick_contrasting_text() {
        let dark = Palette::custom(Color32::from_rgb(18, 20, 28));
        assert!(!dark.light);
        assert!(dark.text.r() > 200);

        let light = Palette::custom(Color32::from_rgb(240, 242, 248));
        assert!(light.light);
        assert!(light.text.r() < 80);
    }
}
