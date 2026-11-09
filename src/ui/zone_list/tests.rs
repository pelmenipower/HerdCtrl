use super::*;
use crate::core::config::{Config, View};
use egui::{Event, Pos2, Vec2};

struct Editor {
    context: egui::Context,
    zones: Vec<Zone>,
    selected: Option<usize>,
    rects: Vec<Rect>,
    changed: bool,
    width: f32,
    view: usize,
    time: f64,
}

impl Editor {
    fn new(width: f32) -> Self {
        let zones = (0..4)
            .map(|index| {
                serde_json::from_value(serde_json::json!({
                    "name": format!("Zone {index}"), "monitor": format!("DP-{index}"),
                    "application": {"id": format!("app-{index}"), "name": format!("App {index}")},
                    "terminal_command": format!("program-{index}"), "delay_ms": index * 100,
                    "bounds": {"x": 0.0, "y": 0.0, "width": 0.5, "height": 1.0}
                }))
                .unwrap()
            })
            .collect();
        let mut editor = Self {
            context: egui::Context::default(),
            zones,
            selected: Some(1),
            rects: Vec::new(),
            changed: false,
            width,
            view: 0,
            time: 0.0,
        };
        editor.frame(Vec::new());
        editor.frame(Vec::new());
        editor
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.02;
        let _ = self.context.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(self.width, 400.0),
                )),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (changed, rects) =
                        show_list(ui, self.view, &mut self.zones, &mut self.selected);
                    self.changed |= changed;
                    self.rects = rects;
                });
            },
        );
    }

    fn pointer(&mut self, pos: Pos2, pressed: Option<bool>) {
        let mut events = vec![Event::PointerMoved(pos)];
        if let Some(pressed) = pressed {
            events.push(Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        self.frame(events);
    }

    fn drag(&mut self, from: usize, target: Pos2) {
        self.pointer(self.rects[from].center(), Some(true));
        self.pointer(target, None);
        self.frame(Vec::new());
        assert!(!self.changed, "no reorder before release");
        self.pointer(target, Some(false));
        self.frame(Vec::new());
    }

    fn names(&self) -> Vec<&str> {
        self.zones.iter().map(|zone| zone.name.as_str()).collect()
    }
}

#[test]
fn pointer_drag_reorders_right_preserves_selected_zone_and_saved_assignments() {
    let mut editor = Editor::new(800.0);
    let original: Vec<_> = editor
        .zones
        .iter()
        .map(|zone| serde_json::to_value(zone).unwrap())
        .collect();
    let target = editor.rects[3].center_bottom() - Vec2::new(0.0, 2.0);
    editor.drag(1, target);
    assert!(editor.changed);
    assert_eq!(editor.names(), ["Zone 0", "Zone 2", "Zone 3", "Zone 1"]);
    assert_eq!(editor.selected, Some(3));
    let config = Config {
        views: vec![View {
            name: "Work".into(),
            hotkey: "Super+KeyD".into(),
            zones: editor.zones,
        }],
    };
    let restored: Config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
    for (zone, original_index) in restored.views[0].zones.iter().zip([0, 2, 3, 1]) {
        assert_eq!(
            serde_json::to_value(zone).unwrap(),
            original[original_index]
        );
    }
    assert_eq!(restored.views[0].hotkey, "Super+KeyD");
}

#[test]
fn pointer_drag_up_preserves_other_selection() {
    let mut editor = Editor::new(230.0);
    assert!(
        editor.rects[3].top() > editor.rects[0].top(),
        "{:?}",
        editor.rects
    );
    let target = editor.rects[0].center_top() + Vec2::new(0.0, 2.0);
    editor.drag(3, target);
    assert!(editor.changed);
    assert_eq!(editor.names(), ["Zone 3", "Zone 0", "Zone 1", "Zone 2"]);
    assert_eq!(editor.selected, Some(2));
}

#[test]
fn clicking_selects_without_reordering_and_outside_drop_cancels() {
    let mut editor = Editor::new(800.0);
    let position = editor.rects[2].center();
    editor.pointer(position, Some(true));
    editor.pointer(position, Some(false));
    assert_eq!(editor.selected, Some(2));
    assert!(!editor.changed);
    editor.drag(2, Pos2::new(500.0, 300.0));
    assert!(!editor.changed);
    assert_eq!(editor.names(), ["Zone 0", "Zone 1", "Zone 2", "Zone 3"]);
    assert!(!egui::DragAndDrop::has_any_payload(&editor.context));
}

#[test]
fn escape_cancels_and_drag_cannot_reorder_another_view() {
    let mut editor = Editor::new(800.0);
    editor.pointer(editor.rects[0].center(), Some(true));
    let target = editor.rects[3].center_bottom() - Vec2::new(0.0, 2.0);
    editor.pointer(target, None);
    editor.frame(Vec::new());
    editor.frame(vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    editor.pointer(target, Some(false));
    assert!(!editor.changed);
    egui::DragAndDrop::set_payload(
        &editor.context,
        ZoneDrag {
            view: 42,
            source: 0,
        },
    );
    editor.pointer(target, Some(false));
    assert!(!editor.changed);
}

#[test]
fn selected_neighbor_follows_identity_and_adjacent_drop_is_noop() {
    let mut editor = Editor::new(800.0);
    assert!(reorder(&mut editor.zones, &mut editor.selected, 0, 4));
    assert_eq!(editor.selected, Some(0));
    assert!(!reorder(&mut editor.zones, &mut editor.selected, 0, 1));
    assert!(!reorder(&mut editor.zones, &mut editor.selected, 99, 0));
}
