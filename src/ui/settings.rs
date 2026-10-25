//! Settings page.

use eframe::egui;
use egui::{Color32, RichText, Stroke, Vec2};

use crate::app::AuraApp;
use crate::config::{Config, HudStyle, Lang, ParentMode, ThemePreset};
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
            row(ui, self.strings.language, self.strings.language_hint, |ui| {
                // Visible segmented picker instead of a collapsed combo box:
                // Spanish, English and French are one click away.
                ui.horizontal(|ui| {
                    // The parent row is right-to-left, so add the chips in
                    // reverse to read "Español | English | Français".
                    for option in [Lang::Fr, Lang::En, Lang::Es] {
                        let selected = self.cfg.language == option;
                        let text = RichText::new(option.native_name())
                            .size(12.0)
                            .color(if selected {
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
                            .min_size(Vec2::new(0.0, 28.0));
                        if ui.add(button).clicked() && !selected {
                            self.set_language(option);
                        }
                    }
                });
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
            row(ui, self.strings.theme_preset, "", |ui| {
                let current = self.cfg.theme_preset;
                let mut preset = current;
                egui::ComboBox::from_id_salt("theme_preset")
                    .selected_text(theme_name(self.strings, current))
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        for option in [
                            ThemePreset::Dark,
                            ThemePreset::Midnight,
                            ThemePreset::Light,
                            ThemePreset::Custom,
                        ] {
                            ui.selectable_value(
                                &mut preset,
                                option,
                                theme_name(self.strings, option),
                            );
                        }
                    });
                if preset != current {
                    self.cfg.theme_preset = preset;
                    theme::set_preset(preset, self.cfg.theme_bg);
                    theme::apply(ui.ctx(), self.accent);
                    self.mark_dirty();
                }
            });

            if self.cfg.theme_preset == ThemePreset::Custom {
                row(ui, self.strings.theme_background, "", |ui| {
                    let mut bg = self.cfg.theme_bg.unwrap_or([18, 20, 28]);
                    if ui.color_edit_button_srgb(&mut bg).changed() {
                        self.cfg.theme_bg = Some(bg);
                        theme::set_preset(ThemePreset::Custom, Some(bg));
                        theme::apply(ui.ctx(), self.accent);
                        self.mark_dirty();
                    }
                    ui.label(
                        RichText::new(format!("#{:02X}{:02X}{:02X}", bg[0], bg[1], bg[2]))
                            .size(10.5)
                            .color(theme::muted()),
                    );
                });
            }

            ui.add_space(4.0);
            ui.label(
                RichText::new(self.strings.opacity_section)
                    .size(12.0)
                    .strong()
                    .color(theme::text()),
            );
            ui.add_space(2.0);

            let mut linked = self.cfg.opacity_linked;
            if row_checkbox(
                ui,
                self.strings.opacity_link,
                self.strings.window_opacity_hint,
                &mut linked,
            ) {
                self.cfg.opacity_linked = linked;
                if linked {
                    self.cfg.nav_opacity = self.cfg.window_opacity;
                    self.cfg.title_opacity = self.cfg.window_opacity;
                }
                self.apply_opacities(ui.ctx());
                self.mark_dirty();
            }

            if self.cfg.opacity_linked {
                row(ui, self.strings.window_opacity, "", |ui| {
                    let mut value = self.cfg.window_opacity;
                    if opacity_slider(ui, &mut value) {
                        self.cfg.window_opacity = value;
                        self.cfg.nav_opacity = value;
                        self.cfg.title_opacity = value;
                        self.apply_opacities(ui.ctx());
                        self.mark_dirty();
                    }
                });
            } else {
                let mut changed = false;
                row(ui, self.strings.opacity_main, "", |ui| {
                    changed |= opacity_slider(ui, &mut self.cfg.window_opacity);
                });
                row(ui, self.strings.opacity_nav, "", |ui| {
                    changed |= opacity_slider(ui, &mut self.cfg.nav_opacity);
                });
                row(ui, self.strings.opacity_title, "", |ui| {
                    changed |= opacity_slider(ui, &mut self.cfg.title_opacity);
                });
                if changed {
                    self.apply_opacities(ui.ctx());
                    self.mark_dirty();
                }
                ui.horizontal(|ui| {
                    if ui.button(self.strings.opacity_match).clicked() {
                        self.cfg.nav_opacity = self.cfg.window_opacity;
                        self.cfg.title_opacity = self.cfg.window_opacity;
                        self.apply_opacities(ui.ctx());
                        self.mark_dirty();
                    }
                });
            }
            ui.add_space(4.0);

            row(
                ui,
                self.strings.theme_accent,
                self.strings.theme_accent_hint,
                |ui| {
                    let mut rgb = [self.accent.r(), self.accent.g(), self.accent.b()];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                        self.cfg.accent_from_wallpaper = false;
                        self.set_accent(color);
                        theme::apply(ui.ctx(), color);
                        self.mark_dirty();
                    }
                    for preset in ACCENT_PRESETS {
                        let selected = self.accent == preset;
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::splat(20.0), egui::Sense::click());
                        let center = rect.center();
                        ui.painter()
                            .circle_filled(center, if selected { 9.0 } else { 7.0 }, preset);
                        if selected {
                            ui.painter().circle_stroke(
                                center,
                                10.5,
                                Stroke::new(1.5_f32, theme::text()),
                            );
                        }
                        if response.clicked() {
                            self.cfg.accent_from_wallpaper = false;
                            self.set_accent(preset);
                            theme::apply(ui.ctx(), preset);
                            self.mark_dirty();
                        }
                    }
                },
            );

            let mut accent = self.cfg.accent_from_wallpaper;
            if row_checkbox(
                ui,
                self.strings.accent_from_wallpaper,
                "",
                &mut accent,
            ) {
                self.cfg.accent_from_wallpaper = accent;
                if accent {
                    self.update_accent_from_last();
                } else {
                    self.set_accent(self.accent);
                }
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

            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), 26.0),
                egui::Sense::hover(),
            );
            ui.painter().rect_filled(rect, 8, self.accent);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!(
                    "#{:02X}{:02X}{:02X}",
                    self.accent.r(),
                    self.accent.g(),
                    self.accent.b()
                ),
                egui::FontId::proportional(11.0),
                theme::contrast_on(self.accent),
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
                    ui.label(theme::icon(theme::ICON_FOLDER).color(theme::muted()));
                    let label = RichText::new(dir.display().to_string())
                        .size(11.5)
                        .color(theme::text());
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
                    theme::danger()
                };
                ui.painter().circle_filled(rect.center(), 4.0, color);
                ui.label(
                    RichText::new(if self.engine.mpv_available() {
                        self.strings.engine_ready
                    } else {
                        self.strings.engine_missing
                    })
                    .size(12.0)
                    .color(theme::text()),
                );
                ui.label(
                    RichText::new(format!("{}", self.engine.running_count()))
                        .size(11.0)
                        .color(theme::muted()),
                );
            });

            ui.add_space(6.0);
            ui.label(
                RichText::new(self.strings.mpv_path)
                    .size(11.0)
                    .color(theme::muted()),
            );
            ui.horizontal(|ui| {
                let path = self
                    .engine
                    .mpv_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "mpv.exe".into());
                ui.add(
                    egui::Label::new(
                        RichText::new(path).size(11.5).color(theme::text()),
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
                        .color(theme::muted()),
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
                    ui.label(RichText::new(label).size(12.0).color(theme::text()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::Frame::NONE
                            .fill(theme::card())
                            .stroke(Stroke::new(1.0_f32, theme::border()))
                            .corner_radius(6)
                            .inner_margin(egui::Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(keys)
                                        .size(10.5)
                                        .color(theme::muted())
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
                    .color(theme::muted()),
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
                    .color(theme::muted()),
            );

            ui.add_space(10.0);
            ui.label(
                RichText::new(strings.wallhaven_key)
                    .size(11.0)
                    .color(theme::muted()),
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
                    .color(theme::muted()),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(crate::util::config_path().display().to_string())
                            .size(11.0)
                            .color(theme::text()),
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
                    .color(theme::muted()),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(crate::util::data_root().display().to_string())
                            .size(11.0)
                            .color(theme::text()),
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
                            .color(theme::danger()),
                    );
                    if ui.button(strings.cancel).clicked() {
                        self.confirm_reset = false;
                    }
                    if ui.button(strings.reset_config).clicked() {
                        self.reset_configuration(ui.ctx());
                        self.confirm_reset = false;
                    }
                });
            } else if ui.button(strings.reset_config).clicked() {
                self.confirm_reset = true;
            }
        });
    }

    fn reset_configuration(&mut self, ctx: &egui::Context) {
        self.cfg = Config::default();
        let language = self.cfg.language;
        self.set_language(language);
        self.favorites = self.cfg.favorites.iter().cloned().collect();
        self.invalidate_filter();
        self.engine.refresh_mpv(self.cfg.mpv_path.clone());
        self.mpv_version = self
            .engine
            .mpv_path()
            .and_then(crate::engine::mpv::version);
        let _ = win::set_autostart(self.cfg.start_with_windows);
        self.accent = theme::DEFAULT_ACCENT;
        self.applied_accent = theme::DEFAULT_ACCENT;
        theme::set_opacities(
            self.cfg.window_opacity,
            self.cfg.nav_opacity,
            self.cfg.title_opacity,
        );
        theme::set_preset(self.cfg.theme_preset, self.cfg.theme_bg);
        theme::apply(ctx, self.accent);
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
            ui.label(RichText::new(title).size(12.5).color(theme::text()));
            if !subtitle.is_empty() {
                ui.label(RichText::new(subtitle).size(10.0).color(theme::muted()));
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
            ui.label(RichText::new(title).size(12.5).color(theme::text()));
            if !subtitle.is_empty() {
                ui.label(RichText::new(subtitle).size(10.0).color(theme::muted()));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            changed = ui.checkbox(value, "").changed();
        });
    });
    ui.add_space(2.0);
    changed
}

const ACCENT_PRESETS: [Color32; 8] = [
    Color32::from_rgb(122, 162, 247),
    Color32::from_rgb(158, 120, 247),
    Color32::from_rgb(247, 122, 162),
    Color32::from_rgb(247, 162, 74),
    Color32::from_rgb(237, 201, 74),
    Color32::from_rgb(94, 214, 154),
    Color32::from_rgb(74, 200, 220),
    Color32::from_rgb(240, 94, 94),
];

fn theme_name(strings: &crate::i18n::Strings, preset: ThemePreset) -> &'static str {
    match preset {
        ThemePreset::Dark => strings.theme_dark,
        ThemePreset::Midnight => strings.theme_midnight,
        ThemePreset::Light => strings.theme_light,
        ThemePreset::Custom => strings.theme_custom,
    }
}

fn opacity_slider(ui: &mut egui::Ui, value: &mut u8) -> bool {
    let mut current = *value as u32;
    let changed = ui
        .add(
            egui::Slider::new(&mut current, 40..=100)
                .suffix("%")
                .integer(),
        )
        .changed();
    if changed {
        *value = current as u8;
    }
    changed
}
