//! Settings page.

use eframe::egui;
use egui::{Color32, RichText, Stroke, Vec2};

use crate::app::AuraApp;
use crate::config::{Config, HudStyle, Lang, ParentMode};
use crate::hotkeys;
use crate::platform::win;
use crate::theme;
use crate::ui::{page_header, section_card};

impl AuraApp {
    pub fn settings_ui(&mut self, ui: &mut egui::Ui) {
        page_header(ui, self.strings.settings_title, "");

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width().min(880.0));

                self.settings_general(ui);
                ui.add_space(12.0);
                self.settings_appearance(ui);
                ui.add_space(12.0);
                self.settings_performance(ui);
                ui.add_space(12.0);
                self.settings_audio(ui);
                ui.add_space(12.0);
                self.settings_library(ui);
                ui.add_space(12.0);
                self.settings_engine(ui);
                ui.add_space(12.0);
                self.settings_shortcuts(ui);
                ui.add_space(12.0);
                self.settings_advanced(ui);
                ui.add_space(30.0);
            });
    }

    fn settings_general(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.strings.settings_general, |ui| {
            row(ui, self.strings.language, "", |ui| {
                let current = self.cfg.language;
                let mut lang = current;
                egui::ComboBox::from_id_salt("language")
                    .selected_text(current.native_name())
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        for option in [Lang::En, Lang::Es, Lang::Fr] {
                            ui.selectable_value(&mut lang, option, option.native_name());
                        }
                    });
                if lang != current {
                    self.set_language(lang);
                }
            });

            let mut start = self.cfg.start_with_windows;
            if row_checkbox(ui, self.strings.start_with_windows, "", &mut start) {
                self.cfg.start_with_windows = start;
                if let Err(err) = win::set_autostart(start) {
                    self.error(format!("{err:#}"));
                }
                self.mark_dirty();
            }

            let mut hidden = self.cfg.start_hidden;
            if row_checkbox(ui, self.strings.start_hidden, "", &mut hidden) {
                self.cfg.start_hidden = hidden;
                self.mark_dirty();
            }

            let mut tray = self.cfg.close_to_tray;
            if row_checkbox(ui, self.strings.close_to_tray, "", &mut tray) {
                self.cfg.close_to_tray = tray;
                self.mark_dirty();
            }

            let mut restore = self.cfg.restore_on_exit;
            if row_checkbox(
                ui,
                self.strings.restore_originals,
                self.strings.restore_originals_hint,
                &mut restore,
            ) {
                self.cfg.restore_on_exit = restore;
                self.mark_dirty();
            }
        });
    }

    fn settings_appearance(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.strings.settings_appearance, |ui| {
            let mut accent = self.cfg.accent_from_wallpaper;
            if row_checkbox(
                ui,
                self.strings.accent_from_wallpaper,
                "",
                &mut accent,
            ) {
                self.cfg.accent_from_wallpaper = accent;
                self.mark_dirty();
            }

            let mut windows_accent = self.cfg.apply_windows_accent;
            if row_checkbox(
                ui,
                self.strings.apply_windows_accent,
                self.strings.accent_warning,
                &mut windows_accent,
            ) {
                self.cfg.apply_windows_accent = windows_accent;
                if windows_accent {
                    let [r, g, b, _] = self.accent.to_array();
                    let _ = win::apply_windows_accent(r, g, b);
                }
                self.mark_dirty();
            }

            row(ui, self.strings.hud_style, "", |ui| {
                let mut style = self.cfg.hud_style;
                egui::ComboBox::from_id_salt("hud_style")
                    .selected_text(match style {
                        HudStyle::Grid => self.strings.hud_grid,
                        HudStyle::Cinematic => self.strings.hud_cinematic,
                        HudStyle::Dock => self.strings.hud_dock,
                    })
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut style, HudStyle::Grid, self.strings.hud_grid);
                        ui.selectable_value(
                            &mut style,
                            HudStyle::Cinematic,
                            self.strings.hud_cinematic,
                        );
                        ui.selectable_value(&mut style, HudStyle::Dock, self.strings.hud_dock);
                    });
                if style != self.cfg.hud_style {
                    self.cfg.hud_style = style;
                    self.mark_dirty();
                }
            });

            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 8, self.accent);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("#{:02X}{:02X}{:02X}", self.accent.r(), self.accent.g(), self.accent.b()),
                egui::FontId::proportional(11.0),
                Color32::from_rgb(10, 11, 15),
            );
        });
    }

    fn settings_performance(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.strings.settings_performance, |ui| {
            let mut fullscreen = self.cfg.pause_on_fullscreen;
            if row_checkbox(
                ui,
                self.strings.pause_fullscreen,
                self.strings.pause_fullscreen_hint,
                &mut fullscreen,
            ) {
                self.cfg.pause_on_fullscreen = fullscreen;
                self.mark_dirty();
            }

            let mut occlusion = self.cfg.pause_on_occlusion;
            if row_checkbox(
                ui,
                self.strings.pause_occlusion,
                self.strings.pause_occlusion_hint,
                &mut occlusion,
            ) {
                self.cfg.pause_on_occlusion = occlusion;
                self.mark_dirty();
            }

            let mut battery = self.cfg.pause_on_battery;
            if row_checkbox(
                ui,
                self.strings.pause_battery,
                self.strings.pause_battery_hint,
                &mut battery,
            ) {
                self.cfg.pause_on_battery = battery;
                self.mark_dirty();
            }

            row(ui, self.strings.parent_mode, self.strings.parent_hint, |ui| {
                let mut mode = self.cfg.parent_mode;
                egui::ComboBox::from_id_salt("parent_mode")
                    .selected_text(match mode {
                        ParentMode::Auto => self.strings.parent_auto,
                        ParentMode::WorkerW => self.strings.parent_workerw,
                        ParentMode::Progman => self.strings.parent_progman,
                    })
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut mode, ParentMode::Auto, self.strings.parent_auto);
                        ui.selectable_value(
                            &mut mode,
                            ParentMode::WorkerW,
                            self.strings.parent_workerw,
                        );
                        ui.selectable_value(
                            &mut mode,
                            ParentMode::Progman,
                            self.strings.parent_progman,
                        );
                    });
                if mode != self.cfg.parent_mode {
                    self.cfg.parent_mode = mode;
                    self.mark_dirty();
                }
            });
        });
    }

    fn settings_audio(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.strings.settings_audio, |ui| {
            row(ui, self.strings.volume, "", |ui| {
                let mut volume = self.cfg.volume;
                if ui
                    .add(egui::Slider::new(&mut volume, 0..=100).suffix("%"))
                    .changed()
                {
                    self.cfg.volume = volume;
                    self.engine.set_volume(volume);
                    if volume > 0 && self.cfg.muted {
                        self.cfg.muted = false;
                        self.engine.set_muted(false);
                    }
                    self.mark_dirty();
                }
            });

            let mut muted = self.cfg.muted;
            if row_checkbox(ui, self.strings.mute, "", &mut muted) {
                self.cfg.muted = muted;
                self.engine.set_muted(muted);
                self.mark_dirty();
            }
        });
    }

    fn settings_library(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.strings.settings_library, |ui| {
            let mut remove: Option<std::path::PathBuf> = None;
            for dir in &self.cfg.library_dirs {
                ui.horizontal(|ui| {
                    ui.label(theme::icon(theme::ICON_FOLDER).color(theme::MUTED));
                    let label = RichText::new(dir.display().to_string())
                        .size(11.5)
                        .color(theme::TEXT);
                    if ui.add(egui::Label::new(label).truncate()).clicked() {
                        crate::util::open_path(dir);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(theme::icon(theme::ICON_DELETE)).frame(false))
                            .on_hover_text(self.strings.remove)
                            .clicked()
                        {
                            remove = Some(dir.clone());
                        }
                    });
                });
            }
            if let Some(dir) = remove {
                self.cfg.remove_library_dir(&dir);
                self.mark_dirty();
                self.rescan();
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(self.strings.add_folder).clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.cfg.add_library_dir(folder);
                        self.mark_dirty();
                        self.rescan();
                        self.info(self.strings.toast_folder_added);
                    }
                }
                if ui.button(self.strings.rescan).clicked() {
                    self.rescan();
                }
            });
        });
    }

    fn settings_engine(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.strings.settings_engine, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
                let color = if self.engine.mpv_available() {
                    Color32::from_rgb(96, 210, 150)
                } else {
                    theme::DANGER
                };
                ui.painter().circle_filled(rect.center(), 4.0, color);
                ui.label(
                    RichText::new(if self.engine.mpv_available() {
                        self.strings.engine_ready
                    } else {
                        self.strings.engine_missing
                    })
                    .size(12.0)
                    .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(format!("{}", self.engine.running_count()))
                        .size(11.0)
                        .color(theme::MUTED),
                );
            });

            ui.add_space(6.0);
            ui.label(
                RichText::new(self.strings.mpv_path)
                    .size(11.0)
                    .color(theme::MUTED),
            );
            ui.horizontal(|ui| {
                let path = self
                    .engine
                    .mpv_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "mpv.exe".into());
                ui.add(
                    egui::Label::new(
                        RichText::new(path).size(11.5).color(theme::TEXT),
                    )
                    .truncate(),
                );
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(self.strings.browse).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("mpv", &["exe"])
                        .pick_file()
                    {
                        self.cfg.mpv_path = Some(path);
                        self.engine.refresh_mpv(self.cfg.mpv_path.clone());
                        self.mpv_version = self
                            .engine
                            .mpv_path()
                            .and_then(crate::engine::mpv::version);
                        self.mark_dirty();
                    }
                }
                if ui
                    .add_enabled(!self.installing, egui::Button::new(self.strings.download_mpv))
                    .clicked()
                {
                    self.start_mpv_install();
                }
                if self.installing {
                    ui.add(
                        egui::ProgressBar::new(if self.install_progress.1 > 0 {
                            self.install_progress.0 as f32 / self.install_progress.1 as f32
                        } else {
                            0.0
                        })
                        .desired_width(160.0)
                        .text(self.install_stage.clone()),
                    );
                }
                if ui.button(self.strings.open_logs).clicked() {
                    win::open_logs_folder();
                }
            });

            if let Some(version) = &self.mpv_version {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(version)
                        .size(10.0)
                        .color(theme::MUTED),
                );
            }
        });
    }

    fn settings_shortcuts(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.strings.settings_hotkeys, |ui| {
            for (keys, _description, action) in hotkeys::SHORTCUTS {
                let label = match action {
                    hotkeys::HotkeyAction::Switcher => self.strings.hotkey_switcher,
                    hotkeys::HotkeyAction::Next => self.strings.hotkey_next,
                    hotkeys::HotkeyAction::Prev => self.strings.hotkey_prev,
                    hotkeys::HotkeyAction::TogglePause => self.strings.hotkey_pause,
                    hotkeys::HotkeyAction::Stop => self.strings.hotkey_stop,
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(12.0).color(theme::TEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::Frame::NONE
                            .fill(Color32::from_rgb(12, 14, 19))
                            .stroke(Stroke::new(1.0_f32, theme::BORDER))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(keys)
                                        .size(10.5)
                                        .color(theme::MUTED)
                                        .monospace(),
                                );
                            });
                    });
                });
            }
        });
    }

    fn settings_advanced(&mut self, ui: &mut egui::Ui) {
        section_card(ui, self.strings.settings_advanced, |ui| {
            let strings = self.strings;

            ui.label(
                RichText::new(strings.pexels_key)
                    .size(11.0)
                    .color(theme::MUTED),
            );
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.cfg.pexels_api_key)
                        .password(true)
                        .hint_text("Pexels API key")
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                self.mark_dirty();
            }
            ui.label(
                RichText::new(strings.pexels_key_hint)
                    .size(10.0)
                    .color(theme::MUTED),
            );

            ui.add_space(10.0);
            ui.label(
                RichText::new("Wallhaven API key (optional)")
                    .size(11.0)
                    .color(theme::MUTED),
            );
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.cfg.wallhaven_api_key)
                        .password(true)
                        .hint_text("Wallhaven API key")
                        .desired_width(f32::INFINITY),
                )
                .changed()
            {
                self.mark_dirty();
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            ui.label(
                RichText::new(strings.config_path)
                    .size(11.0)
                    .color(theme::MUTED),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(crate::util::config_path().display().to_string())
                            .size(11.0)
                            .color(theme::TEXT),
                    )
                    .truncate(),
                );
                if ui.button(strings.reveal).clicked() {
                    crate::util::reveal_in_explorer(&crate::util::config_path());
                }
            });

            ui.label(
                RichText::new(strings.data_path)
                    .size(11.0)
                    .color(theme::MUTED),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(crate::util::data_root().display().to_string())
                            .size(11.0)
                            .color(theme::TEXT),
                    )
                    .truncate(),
                );
                if ui.button(strings.reveal).clicked() {
                    crate::util::open_path(&crate::util::data_root());
                }
            });

            ui.add_space(12.0);
            if self.confirm_reset {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(strings.reset_warning)
                            .size(11.0)
                            .color(theme::DANGER),
                    );
                    if ui.button(strings.cancel).clicked() {
                        self.confirm_reset = false;
                    }
                    if ui.button(strings.reset_config).clicked() {
                        self.reset_configuration();
                        self.confirm_reset = false;
                    }
                });
            } else if ui.button(strings.reset_config).clicked() {
                self.confirm_reset = true;
            }
        });
    }

    fn reset_configuration(&mut self) {
        self.cfg = Config::default();
        self.strings = crate::i18n::strings(self.cfg.language);
        self.engine.refresh_mpv(self.cfg.mpv_path.clone());
        self.mpv_version = self
            .engine
            .mpv_path()
            .and_then(crate::engine::mpv::version);
        let _ = win::set_autostart(self.cfg.start_with_windows);
        self.accent = theme::DEFAULT_ACCENT;
        self.mark_dirty();
        self.rescan();
        self.info(self.strings.saved);
    }
}

fn row(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    control: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(12.5).color(theme::TEXT));
            if !subtitle.is_empty() {
                ui.label(RichText::new(subtitle).size(10.0).color(theme::MUTED));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            control(ui);
        });
    });
    ui.add_space(2.0);
}

fn row_checkbox(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    value: &mut bool,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(12.5).color(theme::TEXT));
            if !subtitle.is_empty() {
                ui.label(RichText::new(subtitle).size(10.0).color(theme::MUTED));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = ui.checkbox(value, "").changed();
        });
    });
    ui.add_space(2.0);
    changed
}
