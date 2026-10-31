use crate::core::{
    config::{View, Zone},
    monitor::{Bounds, Monitor},
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

#[derive(Default)]
pub struct Canvas {
    pub drawing: bool,
    start: Option<(String, Pos2)>,
}

impl Canvas {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        monitors: &[Monitor],
        view: &mut View,
        selected: &mut Option<usize>,
    ) -> bool {
        let (area, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 310.0), Sense::hover());
        ui.painter()
            .rect_filled(area, 12.0, Color32::from_rgb(18, 25, 31));
        if monitors.is_empty() {
            ui.painter().text(
                area.center(),
                Align2::CENTER_CENTER,
                "Connect an X11 display to edit zones",
                FontId::proportional(16.0),
                Color32::GRAY,
            );
            return false;
        }
        let min_x = monitors.iter().map(|m| m.x).min().unwrap() as f32;
        let min_y = monitors.iter().map(|m| m.y).min().unwrap() as f32;
        let max_x = monitors
            .iter()
            .map(|m| m.x as f32 + m.width as f32)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = monitors
            .iter()
            .map(|m| m.y as f32 + m.height as f32)
            .fold(f32::NEG_INFINITY, f32::max);
        let factor =
            ((area.width() - 44.0) / (max_x - min_x)).min((area.height() - 64.0) / (max_y - min_y));
        let origin = area.center() - Vec2::new(max_x - min_x, max_y - min_y) * factor / 2.0;
        let mut changed = false;
        for monitor in monitors {
            let rect = Rect::from_min_size(
                origin + Vec2::new(monitor.x as f32 - min_x, monitor.y as f32 - min_y) * factor,
                Vec2::new(monitor.width as f32, monitor.height as f32) * factor,
            );
            ui.painter().rect(
                rect,
                4.0,
                Color32::from_rgb(29, 40, 49),
                Stroke::new(1.0_f32, Color32::from_rgb(82, 100, 112)),
                StrokeKind::Inside,
            );
            ui.painter().text(
                rect.left_bottom() + Vec2::new(0.0, 8.0),
                Align2::LEFT_TOP,
                format!(
                    "{} · {}×{} · {:.2}×",
                    monitor.name, monitor.width, monitor.height, monitor.scale
                ),
                FontId::proportional(11.0),
                Color32::GRAY,
            );
            if self.drawing {
                let response =
                    ui.interact(rect, ui.id().with((&monitor.name, "draw")), Sense::drag());
                if response.drag_started() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        self.start = Some((monitor.name.clone(), pos));
                    }
                }
                if let Some((name, start)) = &self.start {
                    if name == &monitor.name {
                        if let Some(end) = response.interact_pointer_pos() {
                            let preview = Rect::from_two_pos(*start, rect.clamp(end));
                            ui.painter().rect_stroke(
                                preview,
                                2.0,
                                Stroke::new(2.0_f32, Color32::from_rgb(103, 221, 177)),
                                StrokeKind::Inside,
                            );
                            if response.drag_stopped() {
                                if preview.width() >= 8.0 && preview.height() >= 8.0 {
                                    view.zones.push(Zone {
                                        name: format!("Zone {}", view.zones.len() + 1),
                                        monitor: monitor.name.clone(),
                                        command: String::new(),
                                        delay_ms: 0,
                                        bounds: Bounds {
                                            x: (preview.left() - rect.left()) / rect.width(),
                                            y: (preview.top() - rect.top()) / rect.height(),
                                            width: preview.width() / rect.width(),
                                            height: preview.height() / rect.height(),
                                        },
                                    });
                                    *selected = Some(view.zones.len() - 1);
                                    changed = true;
                                }
                                self.start = None;
                                self.drawing = false;
                            }
                        }
                    }
                }
            }
            for (index, zone) in view
                .zones
                .iter_mut()
                .enumerate()
                .filter(|(_, z)| z.monitor == monitor.name)
            {
                let bounds = zone.bounds;
                let zone_rect = Rect::from_min_size(
                    rect.min + Vec2::new(bounds.x * rect.width(), bounds.y * rect.height()),
                    Vec2::new(bounds.width * rect.width(), bounds.height * rect.height()),
                );
                let active = *selected == Some(index);
                let color = if active {
                    Color32::from_rgb(103, 221, 177)
                } else {
                    Color32::from_rgb(98, 157, 187)
                };
                ui.painter().rect(
                    zone_rect.shrink(2.0),
                    3.0,
                    color.gamma_multiply(0.18),
                    Stroke::new(if active { 2.0_f32 } else { 1.0_f32 }, color),
                    StrokeKind::Inside,
                );
                ui.painter().text(
                    zone_rect.left_top() + Vec2::splat(10.0),
                    Align2::LEFT_TOP,
                    &zone.name,
                    FontId::proportional(13.0),
                    color,
                );
                if !self.drawing {
                    let response = ui.interact(
                        zone_rect,
                        ui.id().with((index, "move")),
                        Sense::click_and_drag(),
                    );
                    if response.clicked() || response.drag_started() {
                        *selected = Some(index);
                    }
                    if response.dragged() {
                        let delta = ui.input(|i| i.pointer.delta());
                        zone.bounds.x = (bounds.x + delta.x / rect.width())
                            .clamp(0.0, (1.0 - bounds.width).max(0.0));
                        zone.bounds.y = (bounds.y + delta.y / rect.height())
                            .clamp(0.0, (1.0 - bounds.height).max(0.0));
                        changed = true;
                    }
                    let handle =
                        Rect::from_min_max(zone_rect.max - Vec2::splat(14.0), zone_rect.max);
                    ui.painter().rect_filled(handle.shrink(3.0), 2.0, color);
                    let resize =
                        ui.interact(handle, ui.id().with((index, "resize")), Sense::drag());
                    if resize.drag_started() {
                        *selected = Some(index);
                    }
                    if resize.dragged() {
                        let delta = ui.input(|i| i.pointer.delta());
                        zone.bounds.width = (bounds.width + delta.x / rect.width())
                            .clamp(0.01_f32.min(1.0 - bounds.x), 1.0 - bounds.x);
                        zone.bounds.height = (bounds.height + delta.y / rect.height())
                            .clamp(0.01_f32.min(1.0 - bounds.y), 1.0 - bounds.y);
                        changed = true;
                    }
                }
            }
        }
        changed
    }
}
