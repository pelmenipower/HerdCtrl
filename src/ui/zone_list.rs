use crate::core::config::Zone;
use eframe::egui::{self, Rect};

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
    ui.small("Zones · Drag an entry to change its order");
    let mut rects = Vec::with_capacity(zones.len());
    let list = ui.horizontal_wrapped(|ui| {
        for (index, zone) in zones.iter().enumerate() {
            let name = zone
                .application
                .as_ref()
                .map_or(zone.name.as_str(), |app| app.name.as_str());
            let response = ui.add(
                egui::Button::new(format!("    {} · {}", index + 1, name))
                    .wrap_mode(egui::TextWrapMode::Extend)
                    .selected(*selected == Some(index))
                    .sense(egui::Sense::click_and_drag()),
            );
            // Paint the grip so it also works with fonts lacking drag-handle glyphs.
            let grip_color = ui.style().interact(&response).fg_stroke.color;
            for x in [6.0, 10.0] {
                for y in [-3.5, 0.0, 3.5] {
                    ui.painter().circle_filled(
                        egui::pos2(response.rect.left() + x, response.rect.center().y + y),
                        0.8_f32,
                        grip_color,
                    );
                }
            }
            rects.push(response.rect);
            if response.clicked() {
                *selected = Some(index);
            }
            response.dnd_set_drag_payload(ZoneDrag {
                view,
                source: index,
            });
            response
                .on_hover_cursor(egui::CursorIcon::Grab)
                .on_hover_text(format!("{} — drag to reorder; click to edit", zone.name));
        }
    });
    let mut changed = false;
    if let Some(payload) = egui::DragAndDrop::payload::<ZoneDrag>(ui.ctx()) {
        if payload.view == view && payload.source < zones.len() {
            if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                // Use the nearest chip, including inter-chip gaps and wrapped rows.
                if list.response.rect.expand(8.0).contains(pointer) {
                    let (index, rect) = rects
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            a.distance_sq_to_pos(pointer)
                                .total_cmp(&b.distance_sq_to_pos(pointer))
                        })
                        .expect("nonempty zone list");
                    let after = pointer.x > rect.center().x;
                    let slot = index + usize::from(after);
                    let x = if after {
                        rect.right() + 2.0
                    } else {
                        rect.left() - 2.0
                    };
                    ui.painter().vline(
                        x,
                        rect.y_range().expand(3.0),
                        egui::Stroke::new(3.0_f32, ui.visuals().selection.stroke.color),
                    );
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
