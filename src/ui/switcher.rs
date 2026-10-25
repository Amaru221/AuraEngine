//! Quick switcher HUD, rendered as a borderless always-on-top viewport.

use std::collections::HashMap;

use eframe::egui;
use egui::epaint::{Mesh, Vertex};
use egui::{
    Align2, Color32, FontId, Key, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, TextureId, Vec2,
};

use crate::app::{AuraApp, HUD_COLS, HUD_PAGE_SIZE, HUD_ROWS};
use crate::config::{HudStyle, MediaKind};
use crate::platform::win;
use crate::scanner::MediaItem;
use crate::theme;
use crate::ui::library::cover_uv;

type ThumbMap = HashMap<String, (TextureId, f32)>;

const SQRT3: f32 = 1.732_050_8;

impl AuraApp {
    pub fn hud_viewport(&mut self, ctx: &egui::Context) {
        // Snapshot taken when the HUD was opened; avoids cloning the library
        // and hashing every item on each frame.
        let items = std::mem::take(&mut self.hud_items);
        if items.is_empty() {
            log::warn!("quick switcher: items snapshot is empty, closing");
            self.hud_open = false;
            self.hidden_lost_foreground = false;
            return;
        }
        let len = items.len();
        let mut index = self.hud_index.min(len - 1);
        let mut page_anim = self.hud_page_anim;
        let mut carousel = self.hud_carousel;

        let style = self.cfg.hud_style;

        // Pre-fetch thumbnails for the visible window.
        let (lo, hi) = if style == HudStyle::Grid {
            let page = index / HUD_PAGE_SIZE;
            (
                page.saturating_sub(1) * HUD_PAGE_SIZE,
                ((page + 2) * HUD_PAGE_SIZE).min(len),
            )
        } else {
            (index.saturating_sub(24), (index + 24).min(len))
        };
        for item in &items[lo..hi] {
            self.request_thumb(item);
        }
        let visible_keys: std::collections::HashSet<&str> = items[lo..hi]
            .iter()
            .map(|item| item.thumb_key.as_str())
            .collect();
        let thumbs: ThumbMap = self
            .thumbs
            .iter()
            .filter(|(key, _)| visible_keys.contains(key.as_str()))
            .map(|(key, entry)| (key.clone(), (entry.texture.id(), entry.aspect)))
            .collect();

        let monitor = win::monitor_for_cursor().or_else(|| self.monitors.first().cloned());
        let (position, size) = match monitor {
            Some(monitor) => {
                let scale = (monitor.dpi as f32 / 96.0).max(0.5);
                (
                    egui::pos2(
                        monitor.rect.left as f32 / scale,
                        monitor.rect.top as f32 / scale,
                    ),
                    egui::vec2(
                        monitor.rect.width() as f32 / scale,
                        monitor.rect.height() as f32 / scale,
                    ),
                )
            }
            None => (egui::pos2(0.0, 0.0), egui::vec2(1280.0, 720.0)),
        };

        let builder = egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_always_on_top()
            .with_active(true)
            .with_transparent(false)
            .with_resizable(false)
            .with_position(position)
            .with_inner_size(size)
            .with_title("Aura Quick Switcher");

        let accent = self.accent;
        let strings = self.strings;
        let mut apply_index: Option<usize> = None;
        let mut close = false;
        let mut toggle_pause = false;
        let mut favorite = false;

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("aura-quick-switcher"),
            builder,
            |ctx, _class| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        let screen = ui.max_rect();
                        let painter = ui.painter().clone();

                        // Dark backdrop with a soft accent ambience.
                        painter.rect_filled(screen, 0, Color32::from_rgb(6, 7, 11));
                        painter.circle_filled(
                            egui::pos2(
                                screen.left() + screen.width() * 0.16,
                                screen.top() + screen.height() * 0.12,
                            ),
                            screen.width() * 0.34,
                            accent.gamma_multiply(0.030),
                        );
                        painter.circle_filled(
                            egui::pos2(
                                screen.right() - screen.width() * 0.12,
                                screen.bottom() - screen.height() * 0.08,
                            ),
                            screen.width() * 0.28,
                            accent.gamma_multiply(0.022),
                        );

                        // Close button.
                        let close_rect = Rect::from_center_size(
                            egui::pos2(screen.right() - 34.0, screen.top() + 30.0),
                            Vec2::splat(30.0),
                        );
                        painter.rect_filled(
                            close_rect,
                            8,
                            Color32::from_rgba_unmultiplied(30, 34, 44, 200),
                        );
                        painter.text(
                            close_rect.center(),
                            Align2::CENTER_CENTER,
                            theme::ICON_CLOSE,
                            FontId::new(13.0, egui::FontFamily::Name("icons".into())),
                            theme::text(),
                        );
                        if ui
                            .interact(close_rect, egui::Id::new("hud-close"), Sense::click())
                            .clicked()
                        {
                            close = true;
                        }

                        let mut delta = 0isize;
                        let mut page_delta = 0isize;
                        let mut go_home = false;
                        let mut go_end = false;
                        let mut scroll = 0.0f32;
                        let mut clicked: Option<usize> = None;

                        ui.input(|input| {
                            if input.key_pressed(Key::Escape) {
                                close = true;
                            }
                            if input.key_pressed(Key::Enter) {
                                apply_index = Some(index);
                            }
                            if input.key_pressed(Key::Space) {
                                toggle_pause = true;
                            }
                            if input.key_pressed(Key::F) {
                                favorite = true;
                            }
                            if input.key_pressed(Key::ArrowRight) || input.key_pressed(Key::D) {
                                delta += 1;
                            }
                            if input.key_pressed(Key::ArrowLeft) || input.key_pressed(Key::A) {
                                delta -= 1;
                            }
                            let vertical = if style == HudStyle::Grid {
                                HUD_COLS as isize
                            } else {
                                1
                            };
                            if input.key_pressed(Key::ArrowDown) || input.key_pressed(Key::S) {
                                delta += vertical;
                            }
                            if input.key_pressed(Key::ArrowUp) || input.key_pressed(Key::W) {
                                delta -= vertical;
                            }
                            if input.key_pressed(Key::PageDown) {
                                page_delta += 1;
                            }
                            if input.key_pressed(Key::PageUp) {
                                page_delta -= 1;
                            }
                            if input.key_pressed(Key::Home) {
                                go_home = true;
                            }
                            if input.key_pressed(Key::End) {
                                go_end = true;
                            }
                            scroll += input.raw_scroll_delta.y;
                        });

                        let pages = len.div_ceil(HUD_PAGE_SIZE).max(1);
                        if page_delta != 0 {
                            if style == HudStyle::Grid {
                                let current_page = index / HUD_PAGE_SIZE;
                                let new_page = (current_page as isize + page_delta)
                                    .clamp(0, pages as isize - 1)
                                    as usize;
                                index = (new_page * HUD_PAGE_SIZE).min(len - 1);
                            } else {
                                // Cinematic/Dock use a circular carousel: page by
                                // the number of cards visible at once.
                                index = (index as isize + page_delta * HUD_COLS as isize)
                                    .rem_euclid(len as isize)
                                    as usize;
                            }
                        }
                        if scroll.abs() > 1.0 {
                            if style == HudStyle::Grid {
                                let current_page = index / HUD_PAGE_SIZE;
                                let step = if scroll > 0.0 { -1 } else { 1 };
                                let new_page = (current_page as isize + step)
                                    .clamp(0, pages as isize - 1)
                                    as usize;
                                index = (new_page * HUD_PAGE_SIZE).min(len - 1);
                            } else {
                                let step = if scroll > 0.0 { -1 } else { 1 };
                                index =
                                    (index as isize + step).rem_euclid(len as isize) as usize;
                            }
                        }
                        if delta != 0 {
                            index = (index as isize + delta).rem_euclid(len as isize) as usize;
                        }
                        if go_home {
                            index = 0;
                        }
                        if go_end {
                            index = len - 1;
                        }

                        // Title block.
                        painter.text(
                            egui::pos2(screen.center().x, screen.top() + 44.0),
                            Align2::CENTER_CENTER,
                            strings.quick_switcher,
                            FontId::proportional(10.0),
                            accent,
                        );
                        let current = &items[index];
                        painter.text(
                            egui::pos2(screen.center().x, screen.top() + 76.0),
                            Align2::CENTER_CENTER,
                            crate::ui::library::truncate(&current.name, 52),
                            FontId::proportional(26.0),
                            theme::text(),
                        );
                        painter.text(
                            egui::pos2(screen.center().x, screen.top() + 103.0),
                            Align2::CENTER_CENTER,
                            format!(
                                "{}  -  {}  -  {} / {}",
                                crate::i18n::kind_label(current.kind, strings),
                                current.folder,
                                index + 1,
                                len
                            ),
                            FontId::proportional(10.5),
                            theme::muted(),
                        );
                        if style == HudStyle::Grid {
                            painter.text(
                                egui::pos2(screen.center().x, screen.top() + 124.0),
                                Align2::CENTER_CENTER,
                                format!("{} / {}", index / HUD_PAGE_SIZE + 1, pages),
                                FontId::proportional(9.5),
                                accent.gamma_multiply(0.75),
                            );
                        }

                        match style {
                            HudStyle::Grid => {
                                draw_honeycomb(
                                    ui,
                                    &items,
                                    &thumbs,
                                    &mut index,
                                    &mut page_anim,
                                    &mut clicked,
                                    accent,
                                    screen,
                                );
                            }
                            HudStyle::Cinematic => {
                                draw_cinematic(
                                    ui, &items, &thumbs, &mut index, &mut carousel, &mut clicked,
                                    accent, screen,
                                );
                            }
                            HudStyle::Dock => {
                                draw_dock(
                                    ui, &items, &thumbs, &mut index, &mut carousel, &mut clicked,
                                    accent, screen,
                                );
                            }
                        }

                        if let Some(index) = clicked {
                            apply_index = Some(index);
                        }

                        // Hint bar.
                        let hint_rect = Rect::from_center_size(
                            egui::pos2(screen.center().x, screen.bottom() - 34.0),
                            egui::vec2(1000.0, 30.0),
                        );
                        painter.rect_filled(
                            hint_rect,
                            8,
                            Color32::from_rgba_unmultiplied(20, 23, 31, 220),
                        );
                        painter.text(
                            hint_rect.center(),
                            Align2::CENTER_CENTER,
                            strings.hud_hint,
                            FontId::proportional(9.5),
                            theme::muted(),
                        );
                    });
            },
        );

        let apply_path = apply_index.map(|i| items[i.min(len - 1)].path.clone());
        let favorite_path = if favorite {
            Some(items[index.min(len - 1)].path.clone())
        } else {
            None
        };

        self.hud_items = items;
        self.hud_index = index;
        self.hud_page_anim = page_anim;
        self.hud_carousel = carousel;

        if let Some(path) = apply_path {
            self.apply_media(&path, None);
            self.hud_open = false;
            self.hidden_lost_foreground = false;
        }
        if close {
            self.hud_open = false;
            self.hidden_lost_foreground = false;
        }
        if toggle_pause {
            self.toggle_pause();
        }
        if let Some(path) = favorite_path {
            let was_favorite = self.favorites.contains(&path);
            self.toggle_favorite(&path);
            let toast = if was_favorite {
                self.strings.toast_favorite_removed
            } else {
                self.strings.toast_favorite_added
            };
            self.info(toast);
        }
    }
}

/// Honeycomb layout: 6 columns x 4 rows per page, with a horizontal page slide.
#[allow(clippy::too_many_arguments)]
fn draw_honeycomb(
    ui: &mut egui::Ui,
    items: &[MediaItem],
    thumbs: &ThumbMap,
    index: &mut usize,
    anim: &mut f32,
    clicked: &mut Option<usize>,
    accent: Color32,
    screen: Rect,
) {
    let len = items.len();
    let pages = len.div_ceil(HUD_PAGE_SIZE).max(1);
    let current_page = (*index / HUD_PAGE_SIZE) as isize;
    let target = current_page as f32;
    *anim += (target - *anim) * 0.22;
    if (*anim - target).abs() < 0.001 {
        *anim = target;
    }

    let cols = HUD_COLS as f32;
    let rows = HUD_ROWS as f32;
    let available_w = (screen.width() - 320.0).max(320.0);
    let available_h = (screen.height() - 300.0).max(240.0);
    // Layout radius (hexagons touch); they are drawn slightly smaller to leave
    // a clean gap between panels.
    let radius = (available_w / (cols * SQRT3 + SQRT3 * 0.5))
        .min(available_h / (2.0 + (rows - 1.0) * 1.5))
        .clamp(38.0, 170.0);
    let draw_radius = radius * 0.90;

    let hex_w = SQRT3 * radius;
    let grid_w = cols * hex_w + hex_w * 0.5;
    let grid_h = 2.0 * radius + (rows - 1.0) * 1.5 * radius;
    let center_y = screen.center().y + 12.0;

    let page_x = |page: isize| {
        screen.center().x - grid_w / 2.0
            + (page as f32 - *anim) * (grid_w + 150.0)
    };

    // Hit-test the current page before painting.
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let mut hover_index: Option<usize> = None;
    if let Some(pointer) = pointer {
        'outer: for row in 0..HUD_ROWS {
            for col in 0..HUD_COLS {
                let item_index = current_page as usize * HUD_PAGE_SIZE + row * HUD_COLS + col;
                if item_index >= len {
                    continue;
                }
                let origin = egui::pos2(page_x(current_page), center_y - grid_h / 2.0);
                let center = hex_center(origin, row, col, radius);
                if point_in_hex(pointer, center, draw_radius) {
                    hover_index = Some(item_index);
                    break 'outer;
                }
            }
        }
    }
    if let Some(hit) = hover_index {
        *index = hit;
    }

    // Neighbour pages first, current page on top.
    for page_offset in [-1isize, 1, 0] {
        let page = current_page + page_offset;
        if page < 0 || page >= pages as isize {
            continue;
        }
        let origin = egui::pos2(page_x(page), center_y - grid_h / 2.0);
        for row in 0..HUD_ROWS {
            for col in 0..HUD_COLS {
                let item_index = page as usize * HUD_PAGE_SIZE + row * HUD_COLS + col;
                if item_index >= len {
                    continue;
                }
                let item = &items[item_index];
                let center = hex_center(origin, row, col, radius);
                let selected = item_index == *index;
                let hovered = hover_index == Some(item_index);
                paint_hex(
                    ui,
                    center,
                    draw_radius,
                    thumbs.get(&item.thumb_key),
                    selected,
                    hovered,
                    accent,
                    item.kind == MediaKind::Video,
                );
            }
        }
    }

    // Grid click (one interaction avoids overlapping hex bounding boxes).
    let grid_rect = Rect::from_center_size(
        egui::pos2(screen.center().x, center_y),
        egui::vec2(grid_w + 24.0, grid_h + 24.0),
    );
    let response = ui.interact(
        grid_rect,
        egui::Id::new("hud-honeycomb-grid"),
        Sense::click(),
    );
    if response.clicked() {
        if let Some(hit) = hover_index {
            *clicked = Some(hit);
        }
    }

    // Side arrows for page navigation.
    let arrow_size = egui::vec2(54.0, 112.0);
    let arrow_y = center_y;
    for (side, target_page) in [(-1isize, current_page - 1), (1, current_page + 1)] {
        let x = if side < 0 {
            screen.left() + 52.0
        } else {
            screen.right() - 52.0
        };
        let rect = Rect::from_center_size(egui::pos2(x, arrow_y), arrow_size);
        let enabled = target_page >= 0 && target_page < pages as isize;
        let hovered = ui
            .input(|input| input.pointer.hover_pos())
            .map(|p| rect.contains(p))
            .unwrap_or(false);
        let painter = ui.painter();
        painter.rect_filled(
            rect,
            14,
            if hovered && enabled {
                accent.gamma_multiply(0.16)
            } else {
                Color32::from_rgba_unmultiplied(24, 27, 36, 200)
            },
        );
        painter.rect_stroke(
            rect,
            14,
            Stroke::new(
                1.0_f32,
                if hovered && enabled {
                    accent.gamma_multiply(0.6)
                } else {
                    theme::border()
                },
            ),
            StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            if side < 0 {
                theme::ICON_CHEVRON_LEFT
            } else {
                theme::ICON_CHEVRON_RIGHT
            },
            FontId::new(20.0, egui::FontFamily::Name("icons".into())),
            if enabled {
                if hovered {
                    accent
                } else {
                    theme::text()
                }
            } else {
                theme::border()
            },
        );
        if enabled
            && ui
                .interact(
                    rect,
                    egui::Id::new(("hud-page-arrow", side)),
                    Sense::click(),
                )
                .clicked()
        {
            *index = (target_page as usize * HUD_PAGE_SIZE).min(len - 1);
        }
    }

    // Page dots.
    if pages > 1 {
        let dot_y = screen.bottom() - 66.0;
        let spacing = 18.0;
        let total = (pages as f32 - 1.0) * spacing;
        for page in 0..pages {
            let x = screen.center().x - total / 2.0 + page as f32 * spacing;
            let active = page as isize == current_page;
            let rect = Rect::from_center_size(egui::pos2(x, dot_y), Vec2::splat(16.0));
            let hovered = ui
                .input(|input| input.pointer.hover_pos())
                .map(|p| rect.contains(p))
                .unwrap_or(false);
            ui.painter().circle_filled(
                egui::pos2(x, dot_y),
                if active {
                    4.5
                } else if hovered {
                    4.0
                } else {
                    3.0
                },
                if active {
                    accent
                } else if hovered {
                    theme::text()
                } else {
                    theme::muted().gamma_multiply(0.6)
                },
            );
            if ui
                .interact(rect, egui::Id::new(("hud-page-dot", page)), Sense::click())
                .clicked()
            {
                *index = (page * HUD_PAGE_SIZE).min(len - 1);
            }
        }
    }
}

fn paint_hex(
    ui: &egui::Ui,
    center: Pos2,
    radius: f32,
    thumb: Option<&(TextureId, f32)>,
    selected: bool,
    hovered: bool,
    accent: Color32,
    is_video: bool,
) {
    let painter = ui.painter();
    let draw_radius = radius * if selected { 1.05 } else if hovered { 1.025 } else { 1.0 };
    let points = hex_points(center, draw_radius);

    // Drop shadow.
    let shadow = hex_points(center + Vec2::new(0.0, 7.0), draw_radius);
    painter.add(Shape::convex_polygon(
        shadow.to_vec(),
        Color32::from_black_alpha(110),
        Stroke::NONE,
    ));

    if selected {
        for (extra, alpha, width) in [(13.0, 0.10, 10.0_f32), (6.0, 0.22, 5.0_f32)] {
            painter.add(Shape::closed_line(
                hex_points(center, draw_radius + extra).to_vec(),
                Stroke::new(width, accent.gamma_multiply(alpha)),
            ));
        }
        painter.add(Shape::convex_polygon(
            points.to_vec(),
            accent.gamma_multiply(0.08),
            Stroke::NONE,
        ));
    }

    if let Some((texture, aspect)) = thumb {
        let bbox = Rect::from_center_size(
            center,
            egui::vec2(SQRT3 * draw_radius, 2.0 * draw_radius),
        );
        let uv = cover_uv(*aspect, bbox);
        let mut mesh = Mesh::with_texture(*texture);
        mesh.vertices = points
            .iter()
            .map(|point| {
                let u = uv.min.x + (point.x - bbox.left()) / bbox.width() * uv.width();
                let v = uv.min.y + (point.y - bbox.top()) / bbox.height() * uv.height();
                Vertex {
                    pos: *point,
                    uv: egui::pos2(u, v),
                    color: Color32::WHITE,
                }
            })
            .collect();
        mesh.indices = vec![0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5];
        painter.add(Shape::mesh(mesh));
    } else {
        painter.add(Shape::convex_polygon(
            points.to_vec(),
            theme::card(),
            Stroke::NONE,
        ));
    }

    // Subtle inner shade for legibility.
    painter.add(Shape::closed_line(
        points.to_vec(),
        Stroke::new(
            if selected {
                2.5_f32
            } else if hovered {
                1.8_f32
            } else {
                1.0_f32
            },
            if selected {
                accent
            } else if hovered {
                accent.gamma_multiply(0.7)
            } else {
                theme::border()
            },
        ),
    ));

    if is_video {
        let badge = center + Vec2::new(0.0, draw_radius * 0.58);
        painter.circle_filled(badge, 13.0, Color32::from_black_alpha(140));
        painter.text(
            badge,
            Align2::CENTER_CENTER,
            theme::ICON_PLAY,
            FontId::new(11.0, egui::FontFamily::Name("icons".into())),
            Color32::from_white_alpha(230),
        );
    }
}

fn hex_points(center: Pos2, radius: f32) -> [Pos2; 6] {
    std::array::from_fn(|i| {
        let angle = -std::f32::consts::FRAC_PI_2
            + i as f32 * std::f32::consts::FRAC_PI_3;
        center + Vec2::new(angle.cos(), angle.sin()) * radius
    })
}

fn hex_center(origin: Pos2, row: usize, col: usize, radius: f32) -> Pos2 {
    let width = SQRT3 * radius;
    egui::pos2(
        origin.x
            + col as f32 * width
            + if row % 2 == 1 { width * 0.5 } else { 0.0 }
            + width * 0.5,
        origin.y + row as f32 * 1.5 * radius + radius,
    )
}

fn point_in_hex(point: Pos2, center: Pos2, radius: f32) -> bool {
    let x = (point.x - center.x) / radius;
    let y = (point.y - center.y) / radius;
    let q = (SQRT3 / 3.0) * x - (1.0 / 3.0) * y;
    let r = (2.0 / 3.0) * y;
    q.abs().max(r.abs()).max((q + r).abs()) <= 1.0
}

/// Frame-rate independent exponential smoothing.
fn smooth_towards(current: &mut f32, target: f32, dt: f32, speed: f32) {
    let factor = 1.0 - (-speed * dt).exp();
    *current += (target - *current) * factor;
    if (target - *current).abs() < 0.001 {
        *current = target;
    }
}

/// Shortest signed distance from `center` to `value` on a circular list.
fn wrap_offset(value: f32, center: f32, len: f32) -> f32 {
    if len <= 0.0 {
        return 0.0;
    }
    (value - center + len * 0.5).rem_euclid(len) - len * 0.5
}

#[allow(clippy::too_many_arguments)]
fn draw_cinematic(
    ui: &mut egui::Ui,
    items: &[MediaItem],
    thumbs: &ThumbMap,
    index: &mut usize,
    anim: &mut f32,
    clicked: &mut Option<usize>,
    accent: Color32,
    screen: Rect,
) {
    let len = items.len();
    if len == 0 {
        return;
    }
    let len_f = len as f32;
    let dt = ui.input(|input| input.stable_dt).clamp(0.001, 0.1);
    smooth_towards(anim, *index as f32, dt, 16.0);
    *anim = (*anim).rem_euclid(len_f);

    let painter = ui.painter().clone();
    let center = egui::pos2(screen.center().x, screen.center().y + 26.0);

    // Farthest cards first so the focused one is painted on top.
    let mut visible: Vec<(usize, f32)> = (0..len)
        .map(|i| (i, wrap_offset(i as f32, *anim, len_f)))
        .filter(|(_, rel)| rel.abs() <= 2.6)
        .collect();
    visible.sort_by(|a, b| {
        b.1.abs()
            .partial_cmp(&a.1.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (item_index, rel) in visible {
        let item = &items[item_index];
        let key = AuraApp::thumb_key_for(item);
        let selected = item_index == *index;
        let distance = rel.abs();

        // Continuous coverflow: size and lift interpolate while the carousel
        // slides, which keeps the transition smooth at any frame rate.
        let width = if distance <= 1.0 {
            310.0 + (220.0 - 310.0) * distance
        } else {
            220.0 + (175.0 - 220.0) * (distance - 1.0).min(1.0)
        };
        let height = width * 9.0 / 16.0;
        let lift = 10.0 * distance.min(1.0) + 8.0 * (distance - 1.0).clamp(0.0, 1.0);
        let x = center.x + rel * 250.0;
        let rect =
            Rect::from_center_size(egui::pos2(x, center.y + lift), Vec2::new(width, height));
        let skew = 10.0 * rel.signum() * distance.min(1.0);
        let fade = (1.0 - (distance / 2.6) * 0.55).clamp(0.35, 1.0);

        if selected {
            let glow = 0.02 + (1.0 - distance.min(1.0)) * 0.07;
            for step in (1..=3).rev() {
                painter.rect_filled(
                    rect.expand(step as f32 * 7.0),
                    18,
                    accent.gamma_multiply(glow),
                );
            }
        }

        paint_skewed(
            ui,
            rect,
            thumbs.get(&key),
            skew,
            selected,
            accent,
            (255.0 * fade) as u8,
        );

        let response = ui.interact(
            rect.expand(4.0),
            egui::Id::new(("hud-cine", item_index)),
            Sense::click(),
        );
        if response.clicked() {
            *clicked = Some(item_index);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_dock(
    ui: &mut egui::Ui,
    items: &[MediaItem],
    thumbs: &ThumbMap,
    index: &mut usize,
    anim: &mut f32,
    clicked: &mut Option<usize>,
    accent: Color32,
    screen: Rect,
) {
    let len = items.len();
    if len == 0 {
        return;
    }
    let len_f = len as f32;
    let dt = ui.input(|input| input.stable_dt).clamp(0.001, 0.1);
    smooth_towards(anim, *index as f32, dt, 15.0);
    *anim = (*anim).rem_euclid(len_f);

    let painter = ui.painter().clone();
    let base_y: f32 = screen.bottom() - 185.0;
    let spacing: f32 = 170.0;

    let dock_width = (spacing * 8.0 + 186.0).min(screen.width() - 120.0);
    let dock_rect = Rect::from_center_size(
        egui::pos2(screen.center().x, base_y + 12.0),
        Vec2::new(dock_width, 210.0),
    );
    painter.rect_filled(
        dock_rect,
        22,
        Color32::from_rgba_unmultiplied(18, 21, 29, 240),
    );
    painter.rect_stroke(
        dock_rect,
        22,
        Stroke::new(1.0_f32, theme::border()),
        StrokeKind::Inside,
    );

    let mut visible: Vec<(usize, f32)> = (0..len)
        .map(|i| (i, wrap_offset(i as f32, *anim, len_f)))
        .filter(|(_, rel)| rel.abs() <= 4.3)
        .collect();
    visible.sort_by(|a, b| {
        b.1.abs()
            .partial_cmp(&a.1.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (item_index, rel) in visible {
        let item = &items[item_index];
        let key = AuraApp::thumb_key_for(item);
        let selected = item_index == *index;
        let distance = rel.abs();

        let width = if distance <= 1.0 {
            184.0 + (146.0 - 184.0) * distance
        } else if distance <= 2.0 {
            146.0 + (122.0 - 146.0) * (distance - 1.0)
        } else if distance <= 3.0 {
            122.0 + (102.0 - 122.0) * (distance - 2.0)
        } else {
            102.0
        };
        let height = width * 0.71;
        let lift = 18.0 * (1.0 - distance.min(1.0));
        let x = screen.center().x + rel * spacing;
        let rect = Rect::from_center_size(
            egui::pos2(x, base_y + height / 2.0 - lift),
            Vec2::new(width, height),
        );
        let fade = (1.0 - (distance / 4.3) * 0.5).clamp(0.4, 1.0);

        if selected {
            painter.rect_filled(rect.expand(4.0), 14, accent.gamma_multiply(0.15));
        }
        paint_card(
            ui,
            rect,
            thumbs.get(&key),
            selected,
            accent,
            (255.0 * fade) as u8,
        );

        let response = ui.interact(
            rect,
            egui::Id::new(("hud-dock", item_index)),
            Sense::click(),
        );
        if response.clicked() {
            *clicked = Some(item_index);
        }
    }
}

fn paint_card(
    ui: &egui::Ui,
    rect: Rect,
    thumb: Option<&(TextureId, f32)>,
    selected: bool,
    accent: Color32,
    alpha: u8,
) {
    let painter = ui.painter();
    painter.rect_filled(rect, 12, theme::card());

    if let Some((texture, aspect)) = thumb {
        let uv = cover_uv(*aspect, rect);
        painter.image(
            *texture,
            rect,
            uv,
            Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
        );
    } else {
        painter.rect_filled(rect, 12, Color32::from_rgb(27, 30, 40));
    }

    painter.rect_stroke(
        rect,
        12,
        Stroke::new(
            if selected { 2.0 } else { 1.0_f32 },
            if selected {
                accent
            } else {
                theme::border()
            },
        ),
        StrokeKind::Inside,
    );
}

fn paint_skewed(
    ui: &egui::Ui,
    rect: Rect,
    thumb: Option<&(TextureId, f32)>,
    skew: f32,
    selected: bool,
    accent: Color32,
    alpha: u8,
) {
    let painter = ui.painter();
    let fade = alpha as f32 / 255.0;
    if let Some((texture, _)) = thumb {
        let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        let corners = [
            egui::pos2(rect.left() + skew, rect.top()),
            egui::pos2(rect.right() + skew, rect.top()),
            egui::pos2(rect.right() - skew, rect.bottom()),
            egui::pos2(rect.left() - skew, rect.bottom()),
        ];
        let uvs = [
            uv.left_top(),
            uv.right_top(),
            uv.right_bottom(),
            uv.left_bottom(),
        ];
        let mut mesh = Mesh::with_texture(*texture);
        mesh.vertices = corners
            .iter()
            .zip(uvs.iter())
            .map(|(pos, uv)| Vertex {
                pos: *pos,
                uv: *uv,
                color: Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
            })
            .collect();
        mesh.indices = vec![0, 1, 2, 0, 2, 3];
        painter.add(Shape::mesh(mesh));
    } else {
        painter.rect_filled(rect, 12, theme::card().gamma_multiply(fade));
    }

    let outline = [
        egui::pos2(rect.left() + skew, rect.top()),
        egui::pos2(rect.right() + skew, rect.top()),
        egui::pos2(rect.right() - skew, rect.bottom()),
        egui::pos2(rect.left() - skew, rect.bottom()),
    ];
    painter.add(Shape::closed_line(
        outline.to_vec(),
        Stroke::new(
            if selected { 2.0 } else { 1.0_f32 },
            if selected {
                accent.gamma_multiply(fade)
            } else {
                theme::border().gamma_multiply(fade.max(0.35))
            },
        ),
    ));
}

#[cfg(test)]
mod tests {
    use super::{smooth_towards, wrap_offset};

    #[test]
    fn wrap_offset_uses_the_shortest_path() {
        assert!((wrap_offset(0.0, 9.0, 10.0) - 1.0).abs() < 1e-5);
        assert!((wrap_offset(9.0, 0.0, 10.0) + 1.0).abs() < 1e-5);
        assert_eq!(wrap_offset(5.0, 5.0, 10.0), 0.0);
    }

    #[test]
    fn smooth_towards_converges_without_overshooting() {
        let mut value = 0.0;
        for _ in 0..120 {
            smooth_towards(&mut value, 5.0, 1.0 / 60.0, 16.0);
            assert!(value <= 5.0);
        }
        assert!((value - 5.0).abs() < 0.01);
    }
}
