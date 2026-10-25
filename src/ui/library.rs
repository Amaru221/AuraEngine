//! Local library page.

use std::path::Path;

use eframe::egui;
use egui::{Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};

use crate::app::{AuraApp, Filter};
use crate::config::MediaKind;
use crate::scanner::MediaItem;
use crate::theme;
use crate::ui::page_header;

const CARD_W: f32 = 236.0;
const CARD_H: f32 = 150.0;
const CARD_TEXT_H: f32 = 46.0;
const GAP: f32 = 14.0;

impl AuraApp {
    pub fn library_ui(&mut self, ui: &mut egui::Ui) {
        page_header(ui, self.strings.library_title, self.strings.library_subtitle);

        self.library_toolbar(ui);
        ui.add_space(12.0);

        let indices = self.filtered_indices();
        let count = indices.len();

        if self.scanning && count == 0 {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.spinner();
                ui.add_space(8.0);
                ui.label(RichText::new(self.strings.scanning).color(theme::muted()));
            });
            return;
        }

        if count == 0 {
            self.library_empty(ui);
            return;
        }

        let total = self.items.len();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{count} {}", self.strings.items))
                    .size(11.5)
                    .color(theme::muted()),
            );
            if count != total {
                ui.label(
                    RichText::new(format!("/ {total}"))
                        .size(11.5)
                        .color(theme::muted()),
                );
            }
            if self.scanning {
                ui.spinner();
            }
        });
        ui.add_space(8.0);

        let available = ui.available_width();
        let columns = (((available + GAP) / (CARD_W + GAP)).floor() as usize).max(1);
        let row_height = CARD_H + CARD_TEXT_H + GAP;
        let rows = count.div_ceil(columns);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_height, rows, |ui, row_range| {
                ui.spacing_mut().item_spacing = Vec2::new(GAP, GAP);
                for row in row_range {
                    ui.horizontal(|ui| {
                        for column in 0..columns {
                            let index = row * columns + column;
                            let Some(&item_index) = indices.get(index) else {
                                break;
                            };
                            // Clone only the cards that are actually visible.
                            let item = self.items[item_index].clone();
                            self.library_card(ui, &item);
                        }
                    });
                }
            });
    }

    fn library_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            egui::Frame::NONE
                .fill(theme::card())
                .stroke(Stroke::new(1.0_f32, theme::border()))
                .corner_radius(9)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(theme::icon(theme::ICON_SEARCH).color(theme::muted()));
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.query)
                                .hint_text(self.strings.search)
                                .desired_width(220.0)
                                .frame(false),
                        );
                        if response.changed() {
                            self.invalidate_filter();
                        }
                        if !self.query.is_empty()
                            && ui
                                .add(egui::Button::new(theme::icon(theme::ICON_CLOSE)).frame(false))
                                .clicked()
                        {
                            self.query.clear();
                            self.invalidate_filter();
                        }
                    });
                });

            ui.add_space(4.0);
            filter_chip(ui, self, Filter::All, self.strings.filter_all);
            filter_chip(ui, self, Filter::Live, self.strings.filter_live);
            filter_chip(ui, self, Filter::Images, self.strings.filter_images);
            filter_chip(ui, self, Filter::Favorites, self.strings.filter_favorites);

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(self.strings.rescan).clicked() {
                    self.rescan();
                }
                if ui.button(self.strings.add_folder).clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.add_library_dir(folder);
                        self.mark_dirty();
                        self.rescan();
                        self.info(self.strings.toast_folder_added);
                    }
                }
            });
        });
    }

    fn library_empty(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(70.0);
            ui.label(
                theme::icon_large(theme::ICON_PICTURES, 46.0).color(theme::border()),
            );
            ui.add_space(10.0);
            ui.label(RichText::new(self.strings.empty_library).size(18.0).strong());
            ui.add_space(4.0);
            ui.label(
                RichText::new(self.strings.empty_library_hint)
                    .size(12.0)
                    .color(theme::muted()),
            );
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                let total = ui.available_width();
                ui.add_space(total / 2.0 - 150.0);
                if super::primary_button(ui, self.strings.add_folder).clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.add_library_dir(folder);
                        self.mark_dirty();
                        self.rescan();
                    }
                }
                if ui.button(self.strings.nav_explore).clicked() {
                    self.nav = crate::app::Nav::Explore;
                }
            });
            ui.add_space(26.0);
            ui.label(
                RichText::new(self.strings.drop_files_hint)
                    .size(11.5)
                    .color(theme::muted()),
            );
        });
    }

    fn library_card(&mut self, ui: &mut egui::Ui, item: &MediaItem) {
        self.request_thumb(item);
        let thumb_key = Self::thumb_key_for(item);
        let applied = self
            .cfg
            .last_applied
            .as_ref()
            .is_some_and(|p| p == &item.path);

        let (rect, _) = ui.allocate_exact_size(Vec2::new(CARD_W, CARD_H + CARD_TEXT_H), Sense::hover());
        let info = CardInfo {
            name: &item.name,
            folder: if item.folder.is_empty() {
                None
            } else {
                Some(item.folder.as_str())
            },
            kind: item.kind,
            kind_label: crate::i18n::kind_label(item.kind, self.strings),
            thumb_key: &thumb_key,
            favorite: self.favorites.contains(&item.path),
            applied,
            applied_label: self.strings.applied_badge,
            accent: self.accent,
        };
        let response = draw_card(ui, rect, &self.thumbs, &info)
            .on_hover_text(format!("{}\n{}", item.name, crate::util::human_size(item.size)));

        // A click that closes a context menu (or any popup) must not fall
        // through to the card underneath.
        let popup_open = ui.ctx().memory(|memory| memory.any_popup_open());
        if response.clicked() && !popup_open {
            self.apply_media(&item.path, None);
        }
        if response.double_clicked() && !popup_open {
            reveal_path(&item.path);
        }

        response.context_menu(|ui| {
            // Cloned when the menu is actually open, not on every frame.
            let path = item.path.clone();
            if ui.button(self.strings.apply).clicked() {
                self.apply_media(&path, None);
                ui.close_menu();
            }
            let favorite = self.favorites.contains(&path);
            let label = if favorite {
                self.strings.unfavorite
            } else {
                self.strings.favorite
            };
            if ui.button(label).clicked() {
                self.toggle_favorite(&path);
                let toast = if favorite {
                    self.strings.toast_favorite_removed
                } else {
                    self.strings.toast_favorite_added
                };
                self.info(toast);
                ui.close_menu();
            }
            if ui.button(self.strings.reveal).clicked() {
                reveal_path(&path);
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button(self.strings.all_monitors, |ui| {
                let monitors = self.monitors.clone();
                for monitor in &monitors {
                    let label = self.monitor_menu_label(monitor);
                    if ui.button(label).clicked() {
                        self.apply_to_monitor(&path, monitor.key.clone());
                        ui.close_menu();
                    }
                }
            });
        });
    }
}

pub struct CardInfo<'a> {
    pub name: &'a str,
    pub folder: Option<&'a str>,
    pub kind: MediaKind,
    pub kind_label: &'a str,
    pub thumb_key: &'a str,
    pub favorite: bool,
    pub applied: bool,
    pub applied_label: &'a str,
    pub accent: Color32,
}

/// Shared card renderer used by the library and the online catalog.
pub fn draw_card(
    ui: &mut egui::Ui,
    rect: Rect,
    thumbs: &std::collections::HashMap<String, crate::app::ThumbEntry>,
    info: &CardInfo<'_>,
) -> egui::Response {
    let response = ui.interact(
        rect,
        egui::Id::new(("aura-card", info.thumb_key)),
        Sense::click(),
    );

    let painter = ui.painter();
    let radius = 12;
    painter.rect_filled(rect, radius, theme::card());

    let image_rect = Rect::from_min_size(
        rect.min + Vec2::splat(1.0),
        Vec2::new(rect.width() - 2.0, CARD_H - 2.0),
    );

    match thumbs.get(info.thumb_key) {
        Some(entry) => {
            let uv = cover_uv(entry.aspect, image_rect);
            painter.image(entry.texture.id(), image_rect, uv, Color32::WHITE);
        }
        None => {
            painter.rect_filled(image_rect, radius, Color32::from_rgb(27, 30, 40));
            painter.text(
                image_rect.center(),
                Align2::CENTER_CENTER,
                info.kind_label,
                FontId::proportional(11.0),
                theme::muted(),
            );
        }
    }

    // Bottom fade for legibility.
    for step in 0..8 {
        let t = step as f32 / 8.0;
        let band_h = 34.0;
        let y = image_rect.bottom() - band_h + band_h * t;
        let alpha = (10.0 + 110.0 * t * t) as u8;
        painter.rect_filled(
            Rect::from_min_max(
                egui::pos2(image_rect.left(), y),
                egui::pos2(image_rect.right(), y + band_h / 8.0 + 1.0),
            ),
            0,
            Color32::from_rgba_unmultiplied(5, 6, 9, alpha),
        );
    }

    // Kind badge.
    let badge_text = info.kind_label;
    let badge_w = 44.0;
    let badge_rect = Rect::from_min_size(
        image_rect.left_top() + Vec2::new(8.0, 8.0),
        Vec2::new(badge_w, 18.0),
    );
    painter.rect_filled(
        badge_rect,
        5,
        if info.kind == MediaKind::Video {
            Color32::from_rgba_unmultiplied(8, 10, 14, 190)
        } else {
            Color32::from_rgba_unmultiplied(8, 10, 14, 150)
        },
    );
    painter.text(
        badge_rect.center(),
        Align2::CENTER_CENTER,
        badge_text,
        FontId::proportional(9.0),
        if info.kind == MediaKind::Video {
            info.accent
        } else {
            Color32::from_rgb(210, 214, 224)
        },
    );

    if info.applied {
        let applied_rect = Rect::from_min_size(
            image_rect.right_top() + Vec2::new(-62.0, 8.0),
            Vec2::new(54.0, 18.0),
        );
        painter.rect_filled(applied_rect, 5, info.accent);
        painter.text(
            applied_rect.center(),
            Align2::CENTER_CENTER,
            info.applied_label,
            FontId::proportional(8.5),
            theme::contrast_on(info.accent),
        );
    } else if info.favorite || response.hovered() {
        let star = if info.favorite {
            theme::ICON_STAR_FILLED
        } else {
            theme::ICON_STAR
        };
        painter.text(
            image_rect.right_top() + Vec2::new(-16.0, 17.0),
            Align2::CENTER_CENTER,
            star,
            FontId::new(14.0, egui::FontFamily::Name("icons".into())),
            if info.favorite {
                Color32::from_rgb(240, 190, 90)
            } else {
                Color32::from_white_alpha(190)
            },
        );
    }

    // Text block.
    let text_top = image_rect.bottom() + 8.0;
    painter.text(
        egui::pos2(rect.left() + 2.0, text_top),
        Align2::LEFT_TOP,
        truncate(info.name, 30),
        FontId::proportional(12.5),
        theme::text(),
    );
    painter.text(
        egui::pos2(rect.left() + 2.0, text_top + 18.0),
        Align2::LEFT_TOP,
        info.folder.unwrap_or(""),
        FontId::proportional(10.0),
        theme::muted(),
    );

    if info.applied || response.hovered() {
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(if info.applied { 1.6 } else { 1.0_f32 }, info.accent),
            StrokeKind::Inside,
        );
    }

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn cover_uv(aspect: f32, rect: Rect) -> Rect {
    let target = if rect.height() > 0.0 {
        rect.width() / rect.height()
    } else {
        1.0
    };
    let mut uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    if aspect > target {
        let width = target / aspect;
        let inset = (1.0 - width) / 2.0;
        uv.min.x = inset;
        uv.max.x = 1.0 - inset;
    } else if aspect > 0.0 {
        let height = aspect / target;
        let inset = (1.0 - height) / 2.0;
        uv.min.y = inset;
        uv.max.y = 1.0 - inset;
    }
    uv
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn filter_chip(ui: &mut egui::Ui, app: &mut AuraApp, filter: Filter, label: &str) {
    let selected = app.filter == filter;
    let text = RichText::new(label)
        .size(12.0)
        .color(if selected {
            theme::contrast_on(app.accent)
        } else {
            theme::muted()
        });
    let button = egui::Button::new(text)
        .fill(if selected {
            app.accent
        } else {
            Color32::TRANSPARENT
        })
        .stroke(if selected {
            Stroke::NONE
        } else {
            Stroke::new(1.0_f32, theme::border())
        })
        .corner_radius(20)
        .min_size(Vec2::new(0.0, 26.0));
    if ui.add(button).clicked() {
        app.filter = filter;
        app.invalidate_filter();
    }
}

pub fn reveal_path(path: &Path) {
    crate::util::reveal_in_explorer(path);
}

