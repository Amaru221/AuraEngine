//! Online catalog page.

use eframe::egui;
use egui::{Align, Color32, Layout, RichText, Sense, Stroke, StrokeKind, Vec2};

use crate::app::AuraApp;
use crate::online::{MotionBgsCategory, Provider};
use crate::theme;
use crate::ui::library::{cover_uv, truncate, CardInfo};
use crate::ui::page_header;

const CARD_W: f32 = 236.0;
const CARD_H: f32 = 150.0;
const CARD_TEXT_H: f32 = 46.0;
const GAP: f32 = 14.0;

impl AuraApp {
    pub fn explore_ui(&mut self, ui: &mut egui::Ui) {
        page_header(ui, self.strings.explore_title, self.strings.explore_subtitle);
        self.explore_toolbar(ui);
        if self.remote_provider == Provider::MotionBgs {
            ui.add_space(10.0);
            self.motionbgs_categories(ui);
        }
        ui.add_space(14.0);

        if self.remote_provider == Provider::Pexels && self.cfg.pexels_api_key.trim().is_empty() {
            egui::Frame::NONE
                .fill(theme::card())
                .stroke(Stroke::new(1.0_f32, theme::border()))
                .corner_radius(12)
                .inner_margin(egui::Margin::same(18))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(self.strings.pexels_key)
                            .strong()
                            .color(theme::text()),
                    );
                    ui.label(
                        RichText::new(self.strings.pexels_key_hint)
                            .size(12.0)
                            .color(theme::muted()),
                    );
                    ui.add_space(8.0);
                    if ui.button(self.strings.open_settings).clicked() {
                        self.nav = crate::app::Nav::Settings;
                    }
                });
            return;
        }

        if let Some(error) = &self.remote_error {
            egui::Frame::NONE
                .fill(theme::card())
                .stroke(Stroke::new(1.0_f32, theme::danger().gamma_multiply(0.6)))
                .corner_radius(10)
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.label(RichText::new(error).size(12.0).color(theme::text()));
                });
            ui.add_space(10.0);
        }

        if self.remote_loading && self.remote.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(70.0);
                ui.spinner();
                ui.add_space(8.0);
                ui.label(RichText::new(self.strings.loading).color(theme::muted()));
            });
            return;
        }

        if self.remote.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(70.0);
                ui.label(
                    theme::icon_large(theme::ICON_GLOBE, 42.0).color(theme::border()),
                );
                ui.add_space(10.0);
                ui.label(RichText::new(self.strings.no_results).color(theme::muted()));
            });
            return;
        }

        let remote_len = self.remote.len();
        let available = ui.available_width();
        let columns = (((available + GAP) / (CARD_W + GAP)).floor() as usize).max(1);
        let row_height = CARD_H + CARD_TEXT_H + GAP;
        let rows = remote_len.div_ceil(columns);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(ui.available_height() - 52.0)
            .show_rows(ui, row_height, rows, |ui, row_range| {
                ui.spacing_mut().item_spacing = Vec2::new(GAP, GAP);
                for row in row_range {
                    ui.horizontal(|ui| {
                        for column in 0..columns {
                            let index = row * columns + column;
                            if index >= remote_len {
                                break;
                            }
                            // Clone only visible cards.
                            let item = self.remote[index].clone();
                            self.remote_card(ui, &item);
                        }
                    });
                }
            });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if self.remote_provider.supports_pages() {
                if self.remote_loading {
                    ui.spinner();
                    ui.label(RichText::new(self.strings.loading).size(12.0).color(theme::muted()));
                } else if ui.button(self.strings.load_more).clicked() {
                    self.remote_page = self.remote_page.saturating_add(1);
                    self.search_online(false);
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} - {}",
                        self.remote_provider_name(),
                        self.remote.len()
                    ))
                    .size(11.0)
                    .color(theme::muted()),
                );
            });
        });
    }

    fn explore_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for provider in Provider::all() {
                let selected = self.remote_provider == provider;
                let label = match provider {
                    Provider::Wallhaven => self.strings.provider_wallhaven,
                    Provider::Bing => self.strings.provider_bing,
                    Provider::MotionBgs => self.strings.provider_motionbgs,
                    Provider::Pexels => self.strings.provider_pexels,
                };
                let text = RichText::new(label).size(12.5).color(if selected {
                    theme::contrast_on(self.accent)
                } else {
                    theme::muted()
                });
                let button = egui::Button::new(text)
                    .fill(if selected {
                        self.accent
                    } else {
                        theme::card()
                    })
                    .stroke(if selected {
                        Stroke::NONE
                    } else {
                        Stroke::new(1.0_f32, theme::border())
                    })
                    .corner_radius(9)
                    .min_size(Vec2::new(0.0, 30.0));
                if ui.add(button).clicked() && !selected {
                    self.remote_provider = provider;
                    self.search_online(true);
                }
            }

            ui.add_space(8.0);

            if self.remote_provider.supports_search() {
                egui::Frame::NONE
                    .fill(theme::card())
                    .stroke(Stroke::new(1.0_f32, theme::border()))
                    .corner_radius(9)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(theme::icon(theme::ICON_SEARCH).color(theme::muted()));
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut self.remote_query)
                                    .hint_text(self.strings.search_online)
                                    .desired_width(240.0)
                                    .frame(false),
                            );
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                // MotionBGS search is global, not category-scoped.
                                if self.remote_provider == Provider::MotionBgs {
                                    self.motionbgs_category = MotionBgsCategory::All;
                                }
                                self.search_online(true);
                            }
                        });
                    });
                if ui.button(self.strings.search).clicked() {
                    if self.remote_provider == Provider::MotionBgs {
                        self.motionbgs_category = MotionBgsCategory::All;
                    }
                    self.search_online(true);
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(self.strings.refresh).clicked() {
                    self.search_online(true);
                }
            });
        });
    }

    fn motionbgs_categories(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for category in MotionBgsCategory::all() {
                let selected = self.motionbgs_category == category;
                let label = match category {
                    MotionBgsCategory::All => self.strings.motionbgs_all,
                    MotionBgsCategory::Anime => self.strings.motionbgs_anime,
                    MotionBgsCategory::Games => self.strings.motionbgs_games,
                    MotionBgsCategory::Ultra4k => self.strings.motionbgs_4k,
                    MotionBgsCategory::Superhero => self.strings.motionbgs_superhero,
                    MotionBgsCategory::Nature => self.strings.motionbgs_nature,
                };
                let text = RichText::new(label).size(11.5).color(if selected {
                    theme::contrast_on(self.accent)
                } else {
                    theme::muted()
                });
                let button = egui::Button::new(text)
                    .fill(if selected {
                        self.accent
                    } else {
                        theme::card()
                    })
                    .stroke(if selected {
                        Stroke::NONE
                    } else {
                        Stroke::new(1.0_f32, theme::border())
                    })
                    .corner_radius(18)
                    .min_size(Vec2::new(0.0, 28.0));
                if ui.add(button).clicked() && !selected {
                    self.motionbgs_category = category;
                    self.remote_query.clear();
                    self.search_online(true);
                }
            }
        });
    }

    fn remote_provider_name(&self) -> &'static str {
        match self.remote_provider {
            Provider::Wallhaven => self.strings.provider_wallhaven,
            Provider::Bing => self.strings.provider_bing,
            Provider::MotionBgs => self.strings.provider_motionbgs,
            Provider::Pexels => self.strings.provider_pexels,
        }
    }

    fn remote_card(&mut self, ui: &mut egui::Ui, item: &crate::online::RemoteItem) {
        self.request_remote_thumb(item);
        let key = item.thumb_key();
        let downloading = self.downloading.contains(&item.id);

        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(CARD_W, CARD_H + CARD_TEXT_H), Sense::hover());

        let info = CardInfo {
            name: &item.title,
            folder: Some(&item.resolution),
            kind: item.kind,
            kind_label: item.kind.label(),
            thumb_key: &key,
            favorite: false,
            applied: false,
            accent: self.accent,
        };

        // Paint the card manually so we can overlay the download state.
        let response = ui.interact(
            rect,
            egui::Id::new(("aura-remote", &item.id)),
            Sense::click(),
        );
        let painter = ui.painter();
        painter.rect_filled(rect, 12, theme::card());
        let image_rect = egui::Rect::from_min_size(
            rect.min + Vec2::splat(1.0),
            Vec2::new(rect.width() - 2.0, CARD_H - 2.0),
        );
        if let Some(entry) = self.thumbs.get(&key) {
            let uv = cover_uv(entry.aspect, image_rect);
            painter.image(entry.texture.id(), image_rect, uv, Color32::WHITE);
        } else {
            painter.rect_filled(image_rect, 12, Color32::from_rgb(27, 30, 40));
            painter.text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                &item.source,
                egui::FontId::proportional(11.0),
                theme::muted(),
            );
        }

        for step in 0..8 {
            let t = step as f32 / 8.0;
            let band = 34.0;
            let y = image_rect.bottom() - band + band * t;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(image_rect.left(), y),
                    egui::pos2(image_rect.right(), y + band / 8.0 + 1.0),
                ),
                0,
                Color32::from_rgba_unmultiplied(5, 6, 9, (10.0 + 110.0 * t * t) as u8),
            );
        }

        painter.text(
            image_rect.left_top() + Vec2::new(10.0, 18.0),
            egui::Align2::LEFT_CENTER,
            &item.source,
            egui::FontId::proportional(9.5),
            Color32::from_white_alpha(220),
        );

        painter.text(
            egui::pos2(rect.left() + 2.0, image_rect.bottom() + 8.0),
            egui::Align2::LEFT_TOP,
            truncate(&item.title, 28),
            egui::FontId::proportional(12.5),
            theme::text(),
        );
        painter.text(
            egui::pos2(rect.left() + 2.0, image_rect.bottom() + 26.0),
            egui::Align2::LEFT_TOP,
            format!("{}  {}", item.resolution, item.kind.label()),
            egui::FontId::proportional(10.0),
            theme::muted(),
        );

        if response.hovered() {
            painter.rect_stroke(
                rect,
                12,
                Stroke::new(1.0_f32, self.accent),
                StrokeKind::Inside,
            );
        }

        if downloading {
            painter.rect_filled(rect, 12, Color32::from_black_alpha(150));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                self.strings.downloading,
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
        }

        // Do not let a click that closes a context menu reach the card.
        let popup_open = ui.ctx().memory(|memory| memory.any_popup_open());
        if response.clicked() && !downloading && !popup_open {
            self.download_remote(item.clone(), true);
        }

        let page = item.page_url.clone();
        let item_clone = item.clone();
        response.context_menu(|ui| {
            if ui.button(self.strings.download_apply).clicked() {
                self.download_remote(item_clone.clone(), true);
                ui.close_menu();
            }
            if ui.button(self.strings.download_only).clicked() {
                self.download_remote(item_clone.clone(), false);
                ui.close_menu();
            }
            if ui.button(self.strings.reveal).clicked() {
                crate::util::open_url(&page);
                ui.close_menu();
            }
        });

        let _ = info;
    }
}
