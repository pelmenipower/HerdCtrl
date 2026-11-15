use super::theme;
use crate::core::config::Zone;
use eframe::egui::{self, Align2, FontId, Rect, Sense, Stroke, Vec2};

#[derive(Clone)]
struct ZoneDrag {
    view: usize,
    source: usize,
}

/// Reorder only the list; geometry and application assignments travel with the zone.
fn reorder(zones: &mut Vec<Zone>, selected: &mut Option<usize>, from: usize, slot: usize) -> bool {
    if from >= zones.len() || slot > zones.len() {
        return false;
    }
    let to = slot - usize::from(slot > from);
    if from == to {
        return false;
    }
    let zone = zones.remove(from);
    zones.insert(to, zone);
    if let Some(index) = selected {
        *index = if *index == from {
            to
        } else if from < *index && *index <= to {
            *index - 1
        } else if to <= *index && *index < from {
            *index + 1
        } else {
            *index
        };
    }
    true
}

pub fn show(
    ui: &mut egui::Ui,
    view: usize,
    zones: &mut Vec<Zone>,
    selected: &mut Option<usize>,
) -> bool {
    show_list(ui, view, zones, selected).0
}

fn show_list(
    ui: &mut egui::Ui,
    view: usize,
    zones: &mut Vec<Zone>,
    selected: &mut Option<usize>,
) -> (bool, Vec<Rect>) {
    if zones.is_empty() {
        return (false, Vec::new());
    }
    let p = theme::palette(ui.ctx());
    let mut rects = Vec::with_capacity(zones.len());
    let list = ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (index, zone) in zones.iter().enumerate() {
            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), 30.0),
                Sense::click_and_drag(),
            );
            let active = *selected == Some(index);
            if active || response.hovered() {
                ui.painter().rect(
                    rect,
                    6.0,
                    if active { p.raised } else { p.hover },
                    Stroke::new(1.0_f32, if active { p.border } else { p.hover }),
                    egui::StrokeKind::Inside,
                );
            }
            // Paint the grip so it also works with fonts lacking drag-handle glyphs.
            for x in [9.0, 13.0] {
                for y in [-4.0, 0.0, 4.0] {
                    ui.painter().circle_filled(
                        egui::pos2(rect.left() + x, rect.center().y + y),
                        1.0_f32,
                        p.muted,
                    );
                }
            }
            let badge = Rect::from_center_size(
                egui::pos2(rect.left() + 30.0, rect.center().y),
                Vec2::splat(18.0),
            );
            ui.painter()
                .rect_filled(badge, 4.0, if active { p.accent } else { p.raised });
            ui.painter().text(
                badge.center(),
                Align2::CENTER_CENTER,
                (index + 1).to_string(),
                FontId::monospace(11.0),
                if active { p.accent_on } else { p.muted },
            );
            let monitor =
                ui.painter()
                    .layout_no_wrap(zone.monitor.clone(), FontId::monospace(11.0), p.muted);
            let monitor_x = rect.right() - monitor.size().x - 8.0;
            ui.painter().galley(
                egui::pos2(monitor_x, rect.center().y - monitor.size().y / 2.0),
                monitor,
                p.muted,
            );
            let name = zone
                .application
                .as_ref()
                .map_or(zone.name.as_str(), |app| app.name.as_str());
            let mut job = egui::text::LayoutJob::default();
            job.append(
                name,
                0.0,
                egui::TextFormat::simple(FontId::proportional(13.0), p.text),
            );
            if !zone.terminal_command.trim().is_empty() {
                job.append(
                    &format!(" · {}", zone.terminal_command.trim()),
                    0.0,
                    egui::TextFormat::simple(FontId::monospace(12.0), p.muted),
                );
            }
            job.wrap = egui::text::TextWrapping::truncate_at_width(
                (monitor_x - badge.right() - 16.0).max(10.0),
            );
            let galley = ui.fonts(|fonts| fonts.layout_job(job));
            ui.painter().galley(
                egui::pos2(badge.right() + 8.0, rect.center().y - galley.size().y / 2.0),
                galley,
                p.text,
            );
            rects.push(rect);
            if response.clicked() {
                *selected = Some(index);
            }
            response.dnd_set_drag_payload(ZoneDrag {
                view,
                source: index,
            });
            response
                .on_hover_cursor(egui::CursorIcon::Grab)
                .on_hover_text(format!("{} — drag to change the launch order", zone.name));
        }
    });
    let mut changed = false;
    if let Some(payload) = egui::DragAndDrop::payload::<ZoneDrag>(ui.ctx()) {
        if payload.view == view && payload.source < zones.len() {
            if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                // Use the nearest row, including the gaps between rows.
                if list.response.rect.expand(8.0).contains(pointer) {
                    let (index, rect) = rects
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            a.distance_sq_to_pos(pointer)
                                .total_cmp(&b.distance_sq_to_pos(pointer))
                        })
                        .expect("nonempty zone list");
                    let after = pointer.y > rect.center().y;
                    let slot = index + usize::from(after);
                    let y = if after {
                        rect.bottom() + 1.0
                    } else {
                        rect.top() - 1.0
                    };
                    ui.painter()
                        .hline(rect.x_range(), y, egui::Stroke::new(2.0_f32, p.accent));
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    if ui.input(|input| input.pointer.any_released()) {
                        egui::DragAndDrop::clear_payload(ui.ctx());
                        changed = reorder(zones, selected, payload.source, slot);
                    }
                }
            }
        }
    }
    (changed, rects)
}

#[cfg(test)]
mod tests;
