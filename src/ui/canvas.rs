use crate::core::{
    config::{View, Zone},
    monitor::{Bounds, Monitor},
};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::collections::HashMap;

mod docking;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq)]
enum DragKind {
    Move,
    Resize,
}
struct Drag {
    zone: usize,
    kind: DragKind,
    origin: Bounds,
    pointer: Pos2,
    monitor: String,
    grab: Vec2,
}

pub struct Canvas {
    pub drawing: bool,
    pub snap_enabled: bool,
    pub feedback: Option<String>,
    start: Option<(String, Pos2)>,
    drag: Option<Drag>,
    sizes: HashMap<String, Vec2>,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            drawing: false,
            snap_enabled: true,
            feedback: None,
            start: None,
            drag: None,
            sizes: HashMap::new(),
        }
    }
}

pub fn has_overlaps(view: &View) -> bool {
    view.zones.iter().enumerate().any(|(index, a)| {
        view.zones[index + 1..]
            .iter()
            .any(|b| a.monitor == b.monitor && docking::overlaps(a.bounds, b.bounds))
    })
}

impl Canvas {
    pub fn align(&mut self, view: &mut View) -> bool {
        let mut changed = false;
        for (monitor, size) in &self.sizes {
            let zones: Vec<_> = view
                .zones
                .iter()
                .filter(|zone| &zone.monitor == monitor)
                .map(|zone| zone.bounds)
                .collect();
            let aligned = docking::align(&zones, *size);
            for (zone, bounds) in view
                .zones
                .iter_mut()
                .filter(|zone| &zone.monitor == monitor)
                .zip(aligned)
            {
                changed |= values(zone.bounds) != values(bounds);
                zone.bounds = bounds;
            }
        }
        self.feedback = Some(
            if has_overlaps(view) {
                "Some zones still overlap. Move or resize them to make more space."
            } else {
                "Zone edges aligned."
            }
            .into(),
        );
        changed
    }

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
        self.sizes.clear();
        if monitors.is_empty() {
            ui.painter().text(
                area.center(),
                Align2::CENTER_CENTER,
                "No displays available. Check Activity, then Refresh displays.",
                FontId::proportional(16.0),
                Color32::GRAY,
            );
            return false;
        }
        let screens = preview_screens(area, monitors);
        // Process captured movement once, independently of monitor rendering order.
        let mut changed = self.move_across_monitors(ui, &screens, view);
        for (number, (monitor, rect)) in screens.iter().enumerate() {
            let rect = *rect;
            self.sizes.insert(monitor.name.clone(), rect.size());
            ui.painter().rect(
                rect,
                4.0,
                Color32::from_rgb(29, 40, 49),
                Stroke::new(1.0_f32, Color32::from_rgb(82, 100, 112)),
                StrokeKind::Inside,
            );
            let peers: Vec<_> = view
                .zones
                .iter()
                .enumerate()
                .filter(|(_, zone)| zone.monitor == monitor.name)
                .map(|(index, zone)| (index, zone.bounds))
                .collect();
            let all: Vec<_> = peers.iter().map(|(_, bounds)| *bounds).collect();
            if self.snap_enabled && (self.drawing || self.drag.is_some()) {
                for fraction in [0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0, 0.75] {
                    let color = Color32::from_white_alpha(14);
                    ui.painter().line_segment(
                        [
                            Pos2::new(rect.left() + rect.width() * fraction, rect.top()),
                            Pos2::new(rect.left() + rect.width() * fraction, rect.bottom()),
                        ],
                        Stroke::new(1.0_f32, color),
                    );
                    ui.painter().line_segment(
                        [
                            Pos2::new(rect.left(), rect.top() + rect.height() * fraction),
                            Pos2::new(rect.right(), rect.top() + rect.height() * fraction),
                        ],
                        Stroke::new(1.0_f32, color),
                    );
                }
            }
            if self.drawing {
                let response =
                    ui.interact(rect, ui.id().with((&monitor.name, "draw")), Sense::drag());
                if response.drag_started() {
                    self.feedback = None;
                    self.start = response
                        .interact_pointer_pos()
                        .map(|pos| (monitor.name.clone(), relative(rect, pos)));
                }
                if let Some((name, start)) = &self.start {
                    if name == &monitor.name {
                        if let Some(end) = response.interact_pointer_pos() {
                            let end = relative(rect, rect.clamp(end));
                            let candidate = if self.snap_enabled {
                                docking::draw(*start, end, &all, rect.size())
                            } else {
                                Some(from_points(*start, end))
                            };
                            let preview = zone_rect(
                                rect,
                                candidate.unwrap_or_else(|| from_points(*start, end)),
                            );
                            let color = if candidate.is_some() {
                                Color32::from_rgb(103, 221, 177)
                            } else {
                                Color32::from_rgb(244, 120, 105)
                            };
                            ui.painter().rect_stroke(
                                preview,
                                2.0,
                                Stroke::new(2.0_f32, color),
                                StrokeKind::Inside,
                            );
                            if let Some(bounds) = candidate.filter(|_| self.snap_enabled) {
                                guides(ui, rect, bounds, &all);
                            }
                            if response.drag_stopped() {
                                if let Some(bounds) = candidate
                                    .filter(|_| preview.width() >= 8.0 && preview.height() >= 8.0)
                                {
                                    view.zones.push(Zone {
                                        name: format!("Zone {}", view.zones.len() + 1),
                                        monitor: monitor.name.clone(),
                                        command: String::new(),
                                        application: None,
                                        terminal_command: String::new(),
                                        run_in_terminal: false,
                                        delay_ms: 0,
                                        bounds,
                                    });
                                    *selected = Some(view.zones.len() - 1);
                                    changed = true;
                                    self.drawing = false;
                                } else {
                                    self.feedback=Some("No free space here. Start in an empty area or resize an existing zone.".into());
                                }
                                self.start = None;
                            }
                        }
                    }
                }
            }
            for (index, zone) in view
                .zones
                .iter_mut()
                .enumerate()
                .filter(|(_, zone)| zone.monitor == monitor.name)
            {
                let neighbours: Vec<_> = peers
                    .iter()
                    .filter(|(other, _)| *other != index)
                    .map(|(_, bounds)| *bounds)
                    .collect();
                let old_rect = zone_rect(rect, zone.bounds);
                if !self.drawing {
                    let move_response = ui.interact(
                        old_rect,
                        ui.id().with((index, "move")),
                        Sense::click_and_drag(),
                    );
                    if move_response.clicked() || move_response.drag_started() {
                        *selected = Some(index);
                    }
                    if move_response.drag_started() {
                        self.begin_drag(ui, index, DragKind::Move, zone, rect);
                    }
                    let handle = Rect::from_min_max(old_rect.max - Vec2::splat(14.0), old_rect.max);
                    let resize_response =
                        ui.interact(handle, ui.id().with((index, "resize")), Sense::drag());
                    if resize_response.drag_started() {
                        *selected = Some(index);
                        self.begin_drag(ui, index, DragKind::Resize, zone, rect);
                    }
                    if resize_response.dragged() {
                        if let Some(drag) = self
                            .drag
                            .as_ref()
                            .filter(|drag| drag.zone == index && drag.kind == DragKind::Resize)
                        {
                            if let Some(pos) = resize_response.interact_pointer_pos() {
                                let delta = (pos - drag.pointer) / rect.size();
                                let wanted = Bounds {
                                    width: (drag.origin.width + delta.x).clamp(
                                        0.01_f32.min(1.0 - drag.origin.x),
                                        1.0 - drag.origin.x,
                                    ),
                                    height: (drag.origin.height + delta.y).clamp(
                                        0.01_f32.min(1.0 - drag.origin.y),
                                        1.0 - drag.origin.y,
                                    ),
                                    ..drag.origin
                                };
                                let next = if self.snap_enabled {
                                    docking::resize(
                                        drag.origin,
                                        Pos2::new(
                                            wanted.x + wanted.width,
                                            wanted.y + wanted.height,
                                        ),
                                        &neighbours,
                                        rect.size(),
                                    )
                                } else {
                                    Some(wanted)
                                };
                                if let Some(next) = next {
                                    changed |= values(zone.bounds) != values(next);
                                    zone.bounds = next;
                                }
                            }
                        }
                    }
                    if resize_response.drag_stopped() {
                        self.drag = None;
                    }
                }
                let zone_area = zone_rect(rect, zone.bounds);
                let active = *selected == Some(index);
                let collision = neighbours
                    .iter()
                    .any(|other| docking::overlaps(zone.bounds, *other));
                let color = if collision {
                    Color32::from_rgb(244, 140, 105)
                } else if active {
                    Color32::from_rgb(103, 221, 177)
                } else {
                    Color32::from_rgb(98, 157, 187)
                };
                ui.painter().rect(
                    zone_area.shrink(2.0),
                    3.0,
                    color.gamma_multiply(0.18),
                    Stroke::new(if active { 2.0_f32 } else { 1.0_f32 }, color),
                    StrokeKind::Inside,
                );
                let text_y = zone_area.top() + 8.0;
                ui.painter().with_clip_rect(zone_area.shrink(5.0)).text(
                    Pos2::new(zone_area.left() + 8.0, text_y),
                    Align2::LEFT_TOP,
                    zone.application
                        .as_ref()
                        .map(|app| app.name.as_str())
                        .unwrap_or(&zone.name),
                    FontId::proportional(13.0),
                    color,
                );
                if !self.drawing {
                    ui.painter().rect_filled(
                        Rect::from_min_max(zone_area.max - Vec2::splat(14.0), zone_area.max)
                            .shrink(3.0),
                        2.0,
                        color,
                    );
                }
                if self.snap_enabled && self.drag.as_ref().is_some_and(|drag| drag.zone == index) {
                    guides(ui, rect, zone.bounds, &neighbours);
                }
            }
            // Captions live outside the editable screen, in a dedicated row gutter.
            let caption = monitor_caption(rect);
            ui.painter().with_clip_rect(caption).text(
                caption.left_center(),
                Align2::LEFT_CENTER,
                format!("{} · {}", number + 1, monitor.name),
                FontId::proportional(11.0),
                Color32::from_rgb(153, 171, 181),
            );
            ui.interact(
                caption,
                ui.id().with((&monitor.name, "caption")),
                Sense::hover(),
            )
            .on_hover_text(format!(
                "{} · {} × {}",
                monitor.display_name(number),
                monitor.width,
                monitor.height
            ));
        }
        changed
    }

    fn move_across_monitors(
        &mut self,
        ui: &egui::Ui,
        screens: &[(&Monitor, Rect)],
        view: &mut View,
    ) -> bool {
        let Some(drag) = self
            .drag
            .as_ref()
            .filter(|drag| drag.kind == DragKind::Move)
        else {
            return false;
        };
        let Some(zone) = view.zones.get(drag.zone) else {
            self.drag = None;
            return false;
        };
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            let zone = &mut view.zones[drag.zone];
            let changed =
                zone.monitor != drag.monitor || values(zone.bounds) != values(drag.origin);
            zone.monitor.clone_from(&drag.monitor);
            zone.bounds = drag.origin;
            self.drag = None;
            self.feedback = None;
            return changed;
        }
        let mut changed = false;
        if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
            // In a gap/outside the preview, retain edge clamping on the last valid screen.
            let target = screens
                .iter()
                .find(|(_, rect)| rect.contains(pointer))
                .or_else(|| {
                    screens
                        .iter()
                        .find(|(monitor, _)| monitor.name == zone.monitor)
                });
            if let Some((monitor, rect)) = target {
                let position = relative(*rect, pointer) - drag.grab;
                let wanted = Bounds {
                    x: position.x,
                    y: position.y,
                    ..drag.origin
                };
                let neighbours: Vec<_> = view
                    .zones
                    .iter()
                    .enumerate()
                    .filter(|(index, zone)| *index != drag.zone && zone.monitor == monitor.name)
                    .map(|(_, zone)| zone.bounds)
                    .collect();
                let next = if self.snap_enabled {
                    docking::move_zone(wanted, &neighbours, rect.size())
                } else {
                    Some(Bounds {
                        x: wanted.x.clamp(0.0, 1.0 - wanted.width),
                        y: wanted.y.clamp(0.0, 1.0 - wanted.height),
                        ..wanted
                    })
                };
                if let Some(bounds) = next {
                    let zone = &mut view.zones[drag.zone];
                    changed = zone.monitor != monitor.name || values(zone.bounds) != values(bounds);
                    zone.monitor.clone_from(&monitor.name);
                    zone.bounds = bounds;
                    self.feedback = None;
                    ui.painter().rect_stroke(
                        *rect,
                        4.0,
                        Stroke::new(2.0_f32, Color32::from_rgb(103, 221, 177)),
                        StrokeKind::Outside,
                    );
                } else {
                    self.feedback = Some(
                        "No free space on this monitor. The zone stays at its last valid position."
                            .into(),
                    );
                    ui.painter().rect_stroke(
                        *rect,
                        4.0,
                        Stroke::new(2.0_f32, Color32::from_rgb(244, 120, 105)),
                        StrokeKind::Outside,
                    );
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
        }
        if ui.input(|input| {
            input.pointer.button_released(egui::PointerButton::Primary)
                || !input.pointer.primary_down()
        }) {
            self.drag = None;
        }
        changed
    }

    fn begin_drag(&mut self, ui: &egui::Ui, index: usize, kind: DragKind, zone: &Zone, rect: Rect) {
        if let Some(pointer) = ui.input(|input| input.pointer.press_origin()) {
            self.feedback = None;
            ui.ctx().request_repaint();
            self.drag = Some(Drag {
                zone: index,
                kind,
                origin: zone.bounds,
                pointer,
                monitor: zone.monitor.clone(),
                grab: relative(rect, pointer) - Pos2::new(zone.bounds.x, zone.bounds.y),
            });
        }
    }
}

// Keep the physical screen aspect ratios. Extra space between preview rows is
// presentation-only: all editing and placement still use the full screen rect.
fn preview_screens(area: Rect, monitors: &[Monitor]) -> Vec<(&Monitor, Rect)> {
    if monitors.is_empty() {
        return Vec::new();
    }
    let min_x = monitors.iter().map(|monitor| monitor.x).min().unwrap() as f32;
    let min_y = monitors.iter().map(|monitor| monitor.y).min().unwrap() as f32;
    let max_x = monitors
        .iter()
        .map(|monitor| monitor.x as f32 + monitor.width as f32)
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = monitors
        .iter()
        .map(|monitor| monitor.y as f32 + monitor.height as f32)
        .fold(f32::NEG_INFINITY, f32::max);
    let mut rows: Vec<_> = monitors.iter().map(|monitor| monitor.y).collect();
    rows.sort_unstable();
    rows.dedup();
    let gap = 22.0_f32;
    let extra = gap * (rows.len() - 1) as f32;
    let factor = ((area.width() - 44.0).max(1.0) / (max_x - min_x))
        .min((area.height() - 64.0 - extra).max(1.0) / (max_y - min_y));
    let origin = area.center()
        - (Vec2::new(max_x - min_x, max_y - min_y) * factor + Vec2::new(0.0, extra)) / 2.0;
    monitors
        .iter()
        .map(|monitor| {
            let row = rows.binary_search(&monitor.y).expect("monitor row exists");
            let rect = Rect::from_min_size(
                origin
                    + Vec2::new(monitor.x as f32 - min_x, monitor.y as f32 - min_y) * factor
                    + Vec2::new(0.0, row as f32 * gap),
                Vec2::new(monitor.width as f32, monitor.height as f32) * factor,
            );
            (monitor, rect)
        })
        .collect()
}

fn monitor_caption(screen: Rect) -> Rect {
    Rect::from_min_size(
        screen.left_top() + Vec2::new(2.0, -18.0),
        Vec2::new((screen.width() - 4.0).max(0.0), 14.0),
    )
}

fn values(bounds: Bounds) -> [f32; 4] {
    [bounds.x, bounds.y, bounds.width, bounds.height]
}
fn relative(rect: Rect, pos: Pos2) -> Pos2 {
    let pos = (pos - rect.min) / rect.size();
    Pos2::new(pos.x, pos.y)
}
fn from_points(start: Pos2, end: Pos2) -> Bounds {
    Bounds {
        x: start.x.min(end.x),
        y: start.y.min(end.y),
        width: (end.x - start.x).abs(),
        height: (end.y - start.y).abs(),
    }
}
fn zone_rect(rect: Rect, bounds: Bounds) -> Rect {
    Rect::from_min_size(
        rect.min + Vec2::new(bounds.x * rect.width(), bounds.y * rect.height()),
        Vec2::new(bounds.width * rect.width(), bounds.height * rect.height()),
    )
}
fn guides(ui: &egui::Ui, rect: Rect, bounds: Bounds, others: &[Bounds]) {
    let (x, y) = docking::lines(others);
    let stroke = Stroke::new(1.5_f32, Color32::from_rgba_unmultiplied(103, 221, 177, 180));
    for x in x {
        if (x - bounds.x).abs() < 0.00001 || (x - bounds.x - bounds.width).abs() < 0.00001 {
            let p = rect.left() + x * rect.width();
            ui.painter().line_segment(
                [Pos2::new(p, rect.top()), Pos2::new(p, rect.bottom())],
                stroke,
            );
        }
    }
    for y in y {
        if (y - bounds.y).abs() < 0.00001 || (y - bounds.y - bounds.height).abs() < 0.00001 {
            let p = rect.top() + y * rect.height();
            ui.painter().line_segment(
                [Pos2::new(rect.left(), p), Pos2::new(rect.right(), p)],
                stroke,
            );
        }
    }
}
