//! About page.

use eframe::egui;
use egui::{Color32, RichText, Sense, Stroke, Vec2};

use crate::app::AuraApp;
use crate::theme;

impl AuraApp {
    pub fn about_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(30.0);

                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(84.0), Sense::hover());
                    let center = rect.center();
                    ui.painter().circle_filled(center, 42.0, self.accent.gamma_multiply(0.16));
                    ui.painter().circle_filled(center, 34.0, theme::card());
                    ui.painter().text(
                        center + Vec2::new(1.0, 0.0),
                        egui::Align2::CENTER_CENTER,
                        theme::ICON_PLAY,
                        egui::FontId::new(30.0, egui::FontFamily::Name("icons".into())),
                        self.accent,
                    );

                    ui.add_space(14.0);
                    ui.label(
                        RichText::new(self.strings.app_name)
                            .size(30.0)
                            .strong()
                            .color(theme::text()),
                    );
                    ui.label(
                        RichText::new(self.strings.tagline)
                            .size(12.5)
                            .color(theme::muted()),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        super::pill(
                            ui,
                            &format!("v{}", env!("CARGO_PKG_VERSION")),
                            self.accent,
                        );
                        if self.update_available.is_some() {
                            ui.label(
                                theme::icon(theme::ICON_UPDATE)
                                    .size(16.0)
                                    .color(self.accent),
                            );
                        }
                    });

                    ui.add_space(22.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(620.0, 0.0),
                        egui::Layout::top_down(egui::Align::Center),
                        |ui| {
                            ui.label(
                                RichText::new(self.strings.about_body)
                                    .size(12.5)
                                    .color(theme::text()),
                            );
                        },
                    );

                    ui.add_space(24.0);

                    if let Some(update) = self.update_available.clone() {
                        egui::Frame::NONE
                            .fill(theme::card())
                            .stroke(Stroke::new(1.0_f32, self.accent.gamma_multiply(0.65)))
                            .corner_radius(12)
                            .inner_margin(egui::Margin::same(16))
                            .show(ui, |ui| {
                                ui.set_width(560.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        theme::icon(theme::ICON_UPDATE)
                                            .size(18.0)
                                            .color(self.accent),
                                    );
                                    ui.label(
                                        RichText::new(format!(
                                            "{}: v{}",
                                            self.strings.update_available, update.version
                                        ))
                                        .size(14.0)
                                        .strong()
                                        .color(self.accent),
                                    );
                                });
                                if !update.notes.is_empty() {
                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new(self.strings.update_notes)
                                            .size(10.0)
                                            .color(theme::muted()),
                                    );
                                    let notes = update
                                        .notes
                                        .lines()
                                        .take(8)
                                        .collect::<Vec<_>>()
                                        .join("\n");
                                    ui.label(
                                        RichText::new(notes)
                                            .size(10.5)
                                            .color(theme::muted()),
                                    );
                                }
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    if self.update_downloading {
                                        let (done, total) = self.update_progress;
                                        let fraction = if total > 0 {
                                            done as f32 / total as f32
                                        } else {
                                            0.0
                                        };
                                        ui.add(
                                            egui::ProgressBar::new(fraction)
                                                .desired_width(220.0)
                                                .show_percentage(),
                                        );
                                        ui.label(
                                            RichText::new(self.strings.update_downloading)
                                                .size(11.0)
                                                .color(theme::muted()),
                                        );
                                    } else if self.update_ready.is_some() {
                                        if super::primary_button(
                                            ui,
                                            self.strings.update_install,
                                        )
                                        .clicked()
                                        {
                                            self.install_update();
                                        }
                                    } else if update.setup_url.is_some() {
                                        if super::primary_button(
                                            ui,
                                            self.strings.update_download,
                                        )
                                        .clicked()
                                        {
                                            self.start_update_download(ui.ctx().clone());
                                        }
                                    }
                                    if ui.button(self.strings.update_view_release).clicked() {
                                        crate::util::open_url(&update.page_url);
                                    }
                                });
                            });
                    } else {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if self.update_checking {
                                ui.spinner();
                                ui.label(
                                    RichText::new(self.strings.update_checking)
                                        .size(11.5)
                                        .color(theme::muted()),
                                );
                            } else {
                                if ui.button(self.strings.update_check).clicked() {
                                    self.start_update_check(ui.ctx().clone(), true);
                                }
                                if self.update_checked && self.update_error.is_none() {
                                    ui.label(
                                        RichText::new(self.strings.update_up_to_date)
                                            .size(11.0)
                                            .color(theme::muted()),
                                    );
                                }
                            }
                        });
                    }

ui.add_space(16.0);


                    egui::Frame::NONE
                        .fill(theme::card())
                        .stroke(Stroke::new(1.0_f32, theme::border()))
                        .corner_radius(12)
                        .inner_margin(egui::Margin::same(18))
                        .show(ui, |ui| {
                            ui.set_width(560.0);
                            info_row(ui, self.strings.version, env!("CARGO_PKG_VERSION"));
                            info_row(ui, "Platform", &format!("Windows {}", std::env::consts::ARCH));
                            info_row(
                                ui,
                                self.strings.status_monitors,
                                &format!("{}", self.monitors.len()),
                            );
                            info_row(
                                ui,
                                "mpv",
                                self.mpv_version
                                    .as_deref()
                                    .unwrap_or(self.strings.none),
                            );
                            info_row(ui, self.strings.license, "GPL-3.0-or-later");
                        });

                    ui.add_space(16.0);

                    ui.add_space(18.0);
                    ui.label(
                        RichText::new(self.strings.credits)
                            .size(11.0)
                            .color(theme::muted()),
                    );

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if ui.button(self.strings.source_code).clicked() {
                            crate::util::open_url("https://github.com/Amaru221/AuraEngine");
                        }
                        if ui.button(self.strings.report_issue).clicked() {
                            crate::util::open_url("https://github.com/Amaru221/AuraEngine/issues");
                        }
                        if ui.button(self.strings.third_party_licenses).clicked() {
                            let path = std::env::current_exe()
                                .ok()
                                .and_then(|exe| {
                                    exe.parent()
                                        .map(|dir| dir.join("THIRD_PARTY_LICENSES.txt"))
                                })
                                .filter(|path| path.exists());
                            if let Some(path) = path {
                                crate::util::open_path(&path);
                            } else {
                                crate::util::open_url(&format!(
                                    "https://github.com/Amaru221/AuraEngine/blob/v{}/THIRD_PARTY_LICENSES.txt",
                                    env!("CARGO_PKG_VERSION")
                                ));
                            }
                        }
                        if ui.button(self.strings.open_logs).clicked() {
                            crate::platform::win::open_logs_folder();
                        }
                    });

                    ui.add_space(30.0);
                });
            });
    }
}

fn info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.5).color(theme::muted()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(11.5).color(theme::text()));
        });
    });
    ui.add_space(4.0);
}

#[allow(dead_code)]
const ACCENT: Color32 = theme::DEFAULT_ACCENT;
