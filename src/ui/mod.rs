//! UI shell: sidebar, page routing, overlays.

pub mod about;
pub mod explore;
pub mod library;
pub mod monitors;
pub mod settings;
pub mod switcher;

use eframe::egui;
use egui::{Align2, Color32, FontId, RichText, Sense, Stroke, Vec2};

use crate::app::{AuraApp, Nav, ToastKind};
use crate::theme;

pub fn main_ui(app: &mut AuraApp, ctx: &egui::Context) {
    title_bar(app, ctx);

    egui::SidePanel::left("aura-sidebar")
        .exact_width(224.0)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(theme::nav())
                .inner_margin(egui::Margin::symmetric(16, 18)),
        )
        .show(ctx, |ui| {
            sidebar(app, ui);
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(theme::bg())
                .inner_margin(egui::Margin::symmetric(26, 22)),
        )
        .show(ctx, |ui| {
            if !app.engine.mpv_available() {
                mpv_banner(app, ui);
            }
            match app.nav {
                Nav::Library => app.library_ui(ui),
                Nav::Explore => app.explore_ui(ui),
                Nav::Monitors => app.monitors_ui(ui),
                Nav::Settings => app.settings_ui(ui),
                Nav::About => app.about_ui(ui),
            }
        });

    toast(app, ctx);
    drop_overlay(ctx);
    window_border(ctx);
    resize_grip(ctx);
}

fn title_bar(app: &mut AuraApp, ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));

    egui::TopBottomPanel::top("aura-titlebar")
        .exact_height(38.0)
        .frame(egui::Frame::NONE.fill(theme::titlebar()))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            let drag = ui.interact(
                rect,
                egui::Id::new("aura-titlebar-drag"),
                Sense::click_and_drag(),
            );
            if drag.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }

            let painter = ui.painter();
            let logo = egui::pos2(rect.left() + 20.0, rect.center().y);
            painter.circle_filled(logo, 9.0, app.accent);
            painter.text(
                logo + Vec2::new(1.0, 0.0),
                Align2::CENTER_CENTER,
                theme::ICON_PLAY,
                FontId::new(9.0, egui::FontFamily::Name("icons".into())),
                theme::contrast_on(app.accent),
            );
            painter.text(
                egui::pos2(rect.left() + 38.0, rect.center().y),
                Align2::LEFT_CENTER,
                app.strings.app_name,
                FontId::proportional(12.5),
                theme::text(),
            );

            let width = 46.0;
            let height = rect.height();
            let close = egui::Rect::from_min_size(
                egui::pos2(rect.right() - width, rect.top()),
                egui::vec2(width, height),
            );
            let maximize = close.translate(egui::vec2(-width, 0.0));
            let minimize = maximize.translate(egui::vec2(-width, 0.0));

            window_button(
                ui,
                minimize,
                "minimize",
                theme::ICON_MINIMIZE,
                false,
                || ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
            );
            window_button(
                ui,
                maximize,
                "maximize",
                if maximized {
                    theme::ICON_RESTORE
                } else {
                    theme::ICON_MAXIMIZE
                },
                false,
                || ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized)),
            );
            window_button(ui, close, "close", theme::ICON_CLOSE, true, || {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close)
            });
        });
}

fn window_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    id: &'static str,
    icon: &str,
    danger: bool,
    action: impl FnOnce(),
) {
    let response = ui.interact(
        rect,
        egui::Id::new(("aura-window-button", id)),
        Sense::click(),
    );
    let hovered = response.hovered();
    let fill = if hovered {
        if danger {
            Color32::from_rgb(196, 43, 28)
        } else {
            theme::card_hover()
        }
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect.shrink(4.0), 6, fill);
    }
    let color = if hovered && danger {
        Color32::WHITE
    } else {
        theme::text()
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::new(11.0, egui::FontFamily::Name("icons".into())),
        color,
    );
    if response.clicked() {
        action();
    }
}

fn window_border(ctx: &egui::Context) {
    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
    let radius = if maximized { 0 } else { 10 };
    let screen = ctx.screen_rect();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("aura-window-border"),
    ))
    .rect_stroke(
        screen.shrink(0.5),
        egui::CornerRadius::same(radius),
        Stroke::new(1.0_f32, theme::border()),
        egui::StrokeKind::Inside,
    );
}

fn resize_grip(ctx: &egui::Context) {
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("aura-resize-grip"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.right_bottom() - Vec2::splat(20.0))
        .show(ctx, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(Vec2::splat(18.0), Sense::drag());
            if response.drag_started() {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(
                    egui::ResizeDirection::SouthEast,
                ));
            }
            response.on_hover_cursor(egui::CursorIcon::ResizeNwSe);

            let painter = ui.painter();
            for index in 0..3 {
                let offset = 4.0 + index as f32 * 4.0;
                painter.line_segment(
                    [
                        egui::pos2(rect.right() - offset, rect.bottom() - 2.0),
                        egui::pos2(rect.right() - 2.0, rect.bottom() - offset),
                    ],
                    Stroke::new(1.0_f32, theme::muted().gamma_multiply(0.55)),
                );
            }
        });
}

fn sidebar(app: &mut AuraApp, ui: &mut egui::Ui) {
    // Logo
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::hover());
        let center = rect.center();
        ui.painter().circle_filled(
            center,
            19.0,
            app.accent.gamma_multiply(0.9),
        );
        ui.painter().circle_filled(
            center,
            14.0,
            Color32::from_rgb(18, 20, 28),
        );
        let icon_pos = center + Vec2::new(1.0, 0.0);
        ui.painter().text(
            icon_pos,
            Align2::CENTER_CENTER,
            theme::ICON_PLAY,
            FontId::new(15.0, egui::FontFamily::Name("icons".into())),
            app.accent,
        );
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(app.strings.app_name).size(17.0).strong());
            ui.label(
                RichText::new(app.strings.tagline)
                    .size(10.5)
                    .color(theme::muted()),
            );
        });
    });

    ui.add_space(26.0);

    nav_item(app, ui, Nav::Library, theme::ICON_PICTURES, app.strings.nav_library);
    nav_item(app, ui, Nav::Explore, theme::ICON_GLOBE, app.strings.nav_explore);
    nav_item(app, ui, Nav::Monitors, theme::ICON_MONITOR, app.strings.nav_monitors);
    nav_item(app, ui, Nav::Settings, theme::ICON_SETTINGS, app.strings.nav_settings);
    nav_item(app, ui, Nav::About, theme::ICON_INFO, app.strings.nav_about);

    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.add_space(2.0);
        ui.label(
            RichText::new(format!("v{} - GPL-3.0", env!("CARGO_PKG_VERSION")))
                .size(10.5)
                .color(theme::muted()),
        );
        ui.add_space(10.0);

        // Playback controls
        ui.horizontal(|ui| {
            let size = Vec2::splat(34.0);
            if icon_button(ui, size, theme::ICON_PREV, app.strings.prev).clicked() {
                app.next_wallpaper(-1);
            }
            let paused = app.engine.is_user_paused();
            let icon = if paused { theme::ICON_PLAY } else { theme::ICON_PAUSE };
            let label = if paused { app.strings.resume } else { app.strings.pause };
            if icon_button(ui, size, icon, label).clicked() {
                app.toggle_pause();
            }
            if icon_button(ui, size, theme::ICON_NEXT, app.strings.next).clicked() {
                app.next_wallpaper(1);
            }
            if icon_button(ui, size, theme::ICON_STOP, app.strings.stop).clicked() {
                app.stop_all();
            }
        });

        ui.add_space(10.0);

        // Status pill
        let running = app.engine.running_count();
        let (label, color, reason) = if running > 0 && app.engine.is_paused() {
            (app.strings.paused, Color32::from_rgb(240, 190, 90), Some(app.engine.env().describe()))
        } else if running > 0 {
            (app.strings.playing, Color32::from_rgb(96, 210, 150), None)
        } else {
            (app.strings.idle, theme::muted(), None)
        };
        let pill = egui::Frame::NONE
            .fill(theme::card())
            .corner_radius(9)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                    ui.painter().circle_filled(rect.center(), 4.0, color);
                    ui.label(RichText::new(label).size(12.0).color(theme::text()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{running}"))
                                .size(12.0)
                                .color(theme::muted()),
                        );
                    });
                });
            });
        let _ = pill;
        if let Some(reason) = reason {
            ui.label(
                RichText::new(format!("{}: {reason}", app.strings.pause_reason))
                    .size(10.0)
                    .color(theme::muted()),
            );
        }
    });
}

fn nav_item(app: &mut AuraApp, ui: &mut egui::Ui, nav: Nav, icon: &str, label: &str) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 42.0), Sense::click());
    let selected = app.nav == nav;
    if selected {
        ui.painter()
            .rect_filled(rect, 10, app.accent.gamma_multiply(0.16));
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
            2,
            app.accent,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 10, theme::card_hover());
    }

    let color = if selected { theme::text() } else { theme::muted() };
    let icon_color = if selected { app.accent } else { theme::muted() };
    ui.painter().text(
        rect.left_center() + Vec2::new(16.0, 0.0),
        Align2::LEFT_CENTER,
        icon,
        FontId::new(16.0, egui::FontFamily::Name("icons".into())),
        icon_color,
    );
    ui.painter().text(
        rect.left_center() + Vec2::new(44.0, 0.0),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.5),
        color,
    );

    if response.clicked() {
        app.nav = nav;
    }
    if response.hovered() {
        response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
}

pub fn icon_button(
    ui: &mut egui::Ui,
    size: Vec2,
    icon: &str,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let fill = if response.hovered() {
        theme::card_hover()
    } else {
        theme::card()
    };
    ui.painter().rect_filled(rect, 9, fill);
    let color = theme::text();
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::new(15.0, egui::FontFamily::Name("icons".into())),
        color,
    );
    response.on_hover_text(tooltip)
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::NONE
        .fill(color.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.55)))
        .corner_radius(20)
        .inner_margin(egui::Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.0).color(color));
        });
}

pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let accent = ui.visuals().hyperlink_color;
    let text_color = theme::contrast_on(accent);
    ui.add(
        egui::Button::new(RichText::new(text).color(text_color).strong())
            .fill(accent)
            .stroke(Stroke::NONE)
            .corner_radius(9)
            .min_size(Vec2::new(0.0, 32.0)),
    )
}

fn mpv_banner(app: &mut AuraApp, ui: &mut egui::Ui) {
    egui::Frame::NONE
        .fill(theme::card())
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(214, 170, 70)))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::icon(theme::ICON_INFO).color(Color32::from_rgb(240, 190, 90)));
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(app.strings.mpv_not_found)
                            .strong()
                            .color(theme::text()),
                    );
                    ui.label(
                        RichText::new(app.strings.mpv_not_found_hint)
                            .size(11.5)
                            .color(theme::muted()),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if app.installing {
                        ui.add(
                            egui::ProgressBar::new(if app.install_progress.1 > 0 {
                                app.install_progress.0 as f32 / app.install_progress.1 as f32
                            } else {
                                0.0
                            })
                            .desired_width(120.0)
                            .text(app.strings.downloading_mpv),
                        );
                    } else if primary_button(ui, app.strings.download_mpv).clicked() {
                        app.start_mpv_install();
                    }
                    if ui.button(app.strings.browse).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("mpv", &["exe"])
                            .pick_file()
                        {
                            app.cfg.mpv_path = Some(path);
                            app.engine.refresh_mpv(app.cfg.mpv_path.clone());
                            app.mpv_version = app
                                .engine
                                .mpv_path()
                                .and_then(crate::engine::mpv::version);
                            app.mark_dirty();
                        }
                    }
                });
            });
        });
    ui.add_space(12.0);
}

fn toast(app: &mut AuraApp, ctx: &egui::Context) {
    let Some(toast) = &app.toast else {
        return;
    };
    let color = match toast.kind {
        ToastKind::Info => app.accent,
        ToastKind::Error => theme::danger(),
    };
    let text = toast.text.clone();
    egui::Area::new(egui::Id::new("aura-toast"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -26.0])
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme::card())
                .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.7)))
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(16, 10))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 4],
                    blur: 18,
                    spread: 0,
                    color: Color32::from_black_alpha(120),
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, color);
                        ui.label(RichText::new(text).size(13.0).color(theme::text()));
                    });
                });
        });
}

fn drop_overlay(ctx: &egui::Context) {
    let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
    if !hovering {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("aura-drop"),
    ));
    let screen = ctx.screen_rect();
    painter.rect_filled(screen, 0, Color32::from_black_alpha(170));
    painter.rect_stroke(
        screen.shrink(40.0),
        16,
        Stroke::new(2.0_f32, theme::DEFAULT_ACCENT),
        egui::StrokeKind::Inside,
    );
    painter.text(
        screen.center(),
        Align2::CENTER_CENTER,
        "Drop to apply",
        FontId::proportional(24.0),
        Color32::WHITE,
    );
}

pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(title).size(24.0).strong().color(theme::text()));
            ui.label(RichText::new(subtitle).size(12.0).color(theme::muted()));
        });
    });
    ui.add_space(16.0);
}

pub fn section_card<R>(
    ui: &mut egui::Ui,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Frame::NONE
        .fill(theme::card())
        .stroke(Stroke::new(1.0_f32, theme::border()))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(13.0).strong().color(theme::text()));
            ui.add_space(10.0);
            add_contents(ui)
        })
        .inner
}
