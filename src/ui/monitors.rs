//! Displays page: virtual desktop map and per-monitor controls.

use std::path::Path;

use eframe::egui;
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};

use crate::app::AuraApp;
use crate::config::{MediaKind, Scaling};
use crate::i18n;
use crate::platform::win;
use crate::scanner;
use crate::theme;
use crate::ui::library::cover_uv;
use crate::ui::page_header;

impl AuraApp {
    pub fn monitors_ui(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            self.strings.monitors_title,
            self.strings.monitors_subtitle,
        );

        self.desktop_map(ui);
        ui.add_space(18.0);

        let monitors = self.monitors.clone();
        for monitor in &monitors {
            self.monitor_card(ui, monitor);
            ui.add_space(10.0);
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if let Some(last) = self.cfg.last_applied.clone() {
                if ui
                    .button(format!(
                        "{}: {}",
                        self.strings.apply_here,
                        last.file_name().unwrap_or_default().to_string_lossy()
                    ))
                    .clicked()
                {
                    self.apply_media(&last, None);
                }
            }
            if ui.button(self.strings.stop).clicked() {
                self.stop_all();
            }
        });
    }

    fn desktop_map(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width().max(200.0);
        let height = 240.0;
        let (canvas, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let painter = ui.painter();

        painter.rect_filled(canvas, 14, theme::card());
        painter.rect_stroke(
            canvas,
            14,
            Stroke::new(1.0_f32, theme::border()),
            StrokeKind::Inside,
        );

        let monitors = self.monitors.clone();
        if monitors.is_empty() {
            painter.text(
                canvas.center(),
                Align2::CENTER_CENTER,
                "No displays detected",
                FontId::proportional(13.0),
                theme::muted(),
            );
            return;
        }

        let min_x = monitors.iter().map(|m| m.rect.left).min().unwrap_or(0);
        let min_y = monitors.iter().map(|m| m.rect.top).min().unwrap_or(0);
        let max_x = monitors.iter().map(|m| m.rect.right).max().unwrap_or(1);
        let max_y = monitors.iter().map(|m| m.rect.bottom).max().unwrap_or(1);
        let virtual_w = (max_x - min_x).max(1) as f32;
        let virtual_h = (max_y - min_y).max(1) as f32;

        let padding = 34.0;
        let scale = ((width - padding * 2.0) / virtual_w)
            .min((height - padding * 2.0) / virtual_h)
            .max(0.02);

        for monitor in &monitors {
            let x = canvas.left() + padding + (monitor.rect.left - min_x) as f32 * scale;
            let y = canvas.top() + padding + (monitor.rect.top - min_y) as f32 * scale;
            let w = monitor.rect.width() as f32 * scale;
            let h = monitor.rect.height() as f32 * scale;
            let rect = Rect::from_min_size(egui::pos2(x, y), Vec2::new(w.max(20.0), h.max(14.0)));

            painter.rect_filled(rect, 8, Color32::from_rgb(18, 20, 28));

            // Wallpaper preview.
            let assignment = self.cfg.assignment_for(&monitor.key).cloned();
            let mut drew_preview = false;
            if let Some(assignment) = &assignment {
                if !assignment.thumb_key.is_empty() {
                    self.request_thumb_by_key(
                        assignment.thumb_key.clone(),
                        &assignment.path,
                        assignment.kind,
                    );
                    if let Some(entry) = self.thumbs.get(&assignment.thumb_key) {
                        let uv = cover_uv(entry.aspect, rect);
                        painter.image(entry.texture.id(), rect, uv, Color32::WHITE);
                        drew_preview = true;
                    }
                } else if let Some(item) = scanner::item_for(&assignment.path) {
                    // Legacy configuration without a cached key.
                    self.request_thumb(&item);
                    if let Some(entry) = self.thumbs.get(&item.thumb_key) {
                        let uv = cover_uv(entry.aspect, rect);
                        painter.image(entry.texture.id(), rect, uv, Color32::WHITE);
                        drew_preview = true;
                    }
                }
            }
            if !drew_preview {
                painter.rect_filled(rect, 8, Color32::from_rgb(22, 25, 34));
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    monitor.resolution(),
                    FontId::proportional(10.0),
                    theme::muted(),
                );
            }

            painter.rect_stroke(
                rect,
                8,
                Stroke::new(
                    if monitor.primary { 2.0 } else { 1.0_f32 },
                    if monitor.primary {
                        self.accent
                    } else {
                        theme::border()
                    },
                ),
                StrokeKind::Inside,
            );

            let label = match monitor.number() {
                Some(number) => format!(
                    "{} {}  -  {}",
                    self.strings.display,
                    number,
                    monitor.resolution()
                ),
                None => format!(
                    "{}  {}",
                    crate::ui::library::truncate(&monitor.label(), 18),
                    monitor.resolution()
                ),
            };
            painter.text(
                rect.center_bottom() + Vec2::new(0.0, 6.0),
                Align2::CENTER_TOP,
                label,
                FontId::proportional(10.0),
                theme::text(),
            );

            // Windows display number, matching Settings -> Display -> Identify.
            if let Some(number) = monitor.number() {
                let badge = rect.left_top() + Vec2::new(17.0, 17.0);
                painter.circle_filled(badge, 12.0, self.accent);
                painter.text(
                    badge,
                    Align2::CENTER_CENTER,
                    number.to_string(),
                    FontId::proportional(12.0),
                    theme::contrast_on(self.accent),
                );
            }

            if monitor.primary {
                painter.text(
                    rect.right_top() + Vec2::new(-6.0, 6.0),
                    Align2::RIGHT_TOP,
                    self.strings.primary,
                    FontId::proportional(8.5),
                    self.accent,
                );
            }
        }
    }

    fn monitor_card(&mut self, ui: &mut egui::Ui, monitor: &win::MonitorInfo) {
        let title = self.monitor_title(monitor);
        let name = monitor.label();
        egui::Frame::NONE
            .fill(theme::card())
            .stroke(Stroke::new(1.0_f32, theme::border()))
            .corner_radius(12)
            .inner_margin(egui::Margin::same(16))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
                    ui.painter().rect_filled(rect, 9, Color32::from_rgb(28, 32, 42));
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        theme::ICON_MONITOR,
                        FontId::new(17.0, egui::FontFamily::Name("icons".into())),
                        if monitor.primary {
                            self.accent
                        } else {
                            theme::muted()
                        },
                    );
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&title).strong().size(14.0));
                            if name != title {
                                ui.label(
                                    RichText::new(&name).size(11.5).color(theme::muted()),
                                );
                            }
                            if monitor.primary {
                                super::pill(ui, self.strings.primary, self.accent);
                            }
                        });
                        ui.label(
                            RichText::new(format!(
                                "{}  -  {}x{} @ {}  -  {}",
                                monitor.key,
                                monitor.rect.width(),
                                monitor.rect.height(),
                                monitor.dpi,
                                self.strings.resolution
                            ))
                            .size(10.5)
                            .color(theme::muted()),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(self.strings.stop_here).clicked() {
                            self.engine.stop_monitor(&monitor.key);
                            self.cfg.clear_assignment(&monitor.key);
                            self.mark_dirty();
                        }
                        if ui.button(self.strings.choose_file).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Wallpapers", &["mp4", "webm", "mkv", "mov", "jpg", "jpeg", "png", "webp", "gif"])
                                .pick_file()
                            {
                                self.apply_to_monitor(&path, monitor.key.clone());
                            }
                        }
                        if let Some(last) = self.cfg.last_applied.clone() {
                            if ui.button(self.strings.apply_here).clicked() {
                                self.apply_to_monitor(&last, monitor.key.clone());
                            }
                        }
                    });
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(self.strings.current_wallpaper)
                            .size(11.0)
                            .color(theme::muted()),
                    );
                    let current = self.cfg.assignment_for(&monitor.key).cloned();
                    match current {
                        Some(assignment) => {
                            ui.label(
                                RichText::new(
                                    assignment
                                        .path
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy(),
                                )
                                .size(11.5)
                                .color(theme::text()),
                            );
                            super::pill(
                                ui,
                                if assignment.kind == MediaKind::Video {
                                    "LIVE"
                                } else {
                                    "IMAGE"
                                },
                                self.accent,
                            );
                        }
                        None => {
                            ui.label(RichText::new(self.strings.none).size(11.5).color(theme::muted()));
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let strings = self.strings;
                        let previous = self
                            .monitor_scaling
                            .get(&monitor.key)
                            .copied()
                            .or_else(|| {
                                self.cfg
                                    .assignment_for(&monitor.key)
                                    .map(|a| a.scaling)
                            })
                            .unwrap_or(Scaling::Fill);
                        let mut scaling = previous;

                        egui::ComboBox::from_id_salt(("scaling", &monitor.key))
                            .selected_text(i18n::scaling_name(scaling, strings))
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                for option in Scaling::all() {
                                    ui.selectable_value(
                                        &mut scaling,
                                        option,
                                        i18n::scaling_name(option, strings),
                                    );
                                }
                            });

                        ui.label(
                            RichText::new(strings.scaling)
                                .size(11.0)
                                .color(theme::muted()),
                        );

                        if scaling != previous {
                            self.monitor_scaling.insert(monitor.key.clone(), scaling);
                            if let Some(assignment) =
                                self.cfg.assignments.get_mut(&monitor.key)
                            {
                                assignment.scaling = scaling;
                            }
                            self.engine.set_scaling(&monitor.key, scaling);
                            self.mark_dirty();
                        }
                    });
                });
            });
    }
}

#[allow(dead_code)]
fn apply_to_path(app: &mut AuraApp, path: &Path, key: String) {
    app.apply_to_monitor(path, key);
}
