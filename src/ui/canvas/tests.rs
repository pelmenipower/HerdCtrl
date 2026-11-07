use super::*;

struct Editor {
    context: egui::Context,
    canvas: Canvas,
    view: View,
    selected: Option<usize>,
    time: f64,
    monitors: Vec<Monitor>,
}

impl Editor {
    fn new() -> Self {
        let mut editor = Self {
            context: egui::Context::default(),
            canvas: Canvas::default(),
            view: View {
                name: "Editor acceptance".into(),
                hotkey: String::new(),
                zones: Vec::new(),
            },
            selected: None,
            time: 0.0,
            monitors: vec![Monitor {
                name: "DP-1".into(),
                x: -1920,
                y: -200,
                width: 1920,
                height: 1080,
                scale: 1.5,
            }],
        };
        editor.frame(Vec::new());
        editor
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 0.02;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 600.0))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let _ = self.context.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                self.canvas
                    .show(ui, &self.monitors, &mut self.view, &mut self.selected);
            });
        });
    }

    fn pointer(&mut self, x: f32, y: f32, pressed: Option<bool>) {
        let pos = Pos2::new(x, y);
        let mut events = vec![egui::Event::PointerMoved(pos)];
        if let Some(pressed) = pressed {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        self.frame(events);
    }

    fn draw(&mut self, intermediate: Pos2) {
        self.canvas.drawing = true;
        self.frame(Vec::new());
        self.pointer(350.0, 100.0, Some(true));
        self.pointer(intermediate.x, intermediate.y, None);
        self.pointer(600.0, 220.0, None);
        self.pointer(600.0, 220.0, Some(false));
        self.frame(Vec::new());
    }
}

fn values(bounds: Bounds) -> [f32; 4] {
    [bounds.x, bounds.y, bounds.width, bounds.height]
}

#[test]
fn drawing_preserves_press_origin_independent_of_pointer_sampling() {
    let mut first = Editor::new();
    first.draw(Pos2::new(370.0, 120.0));
    let mut second = Editor::new();
    second.draw(Pos2::new(400.0, 150.0));
    assert_eq!(first.view.zones.len(), 1);
    assert_eq!(second.view.zones.len(), 1);
    assert_eq!(
        values(first.view.zones[0].bounds),
        values(second.view.zones[0].bounds)
    );
    assert!(first.view.zones[0].bounds.valid());
    assert_eq!(first.selected, Some(0));
    assert!(!first.canvas.drawing);
}

#[test]
fn moving_clamps_zone_to_monitor_without_resizing() {
    let mut editor = Editor::new();
    editor.draw(Pos2::new(370.0, 120.0));
    let original = editor.view.zones[0].bounds;
    editor.pointer(450.0, 160.0, Some(true));
    editor.pointer(470.0, 180.0, None);
    editor.pointer(950.0, 550.0, None);
    editor.pointer(950.0, 550.0, Some(false));
    let moved = editor.view.zones[0].bounds;
    assert!(moved.x > original.x && moved.y > original.y);
    assert_eq!(moved.width, original.width);
    assert_eq!(moved.height, original.height);
    assert!(moved.valid());
    assert!((moved.x + moved.width - 1.0).abs() < 0.00001);
    assert!((moved.y + moved.height - 1.0).abs() < 0.00001);
}

#[test]
fn resizing_keeps_origin_and_changes_dimensions() {
    let mut editor = Editor::new();
    editor.draw(Pos2::new(370.0, 120.0));
    let original = editor.view.zones[0].bounds;
    editor.pointer(595.0, 215.0, Some(true));
    editor.pointer(610.0, 230.0, None);
    editor.pointer(650.0, 260.0, None);
    editor.pointer(650.0, 260.0, Some(false));
    let resized = editor.view.zones[0].bounds;
    assert_eq!(resized.x, original.x);
    assert_eq!(resized.y, original.y);
    assert!(resized.width > original.width);
    assert!(resized.height > original.height);
    assert!(resized.valid());
}

#[test]
fn drawing_outside_monitor_clamps_the_finished_zone() {
    let mut editor = Editor::new();
    editor.canvas.drawing = true;
    editor.frame(Vec::new());
    editor.pointer(350.0, 100.0, Some(true));
    editor.pointer(370.0, 120.0, None);
    editor.pointer(950.0, 550.0, None);
    editor.pointer(950.0, 550.0, Some(false));
    assert_eq!(editor.view.zones.len(), 1);
    let bounds = editor.view.zones[0].bounds;
    assert!(bounds.valid());
    assert!((bounds.x + bounds.width - 1.0).abs() < 0.00001);
    assert!((bounds.y + bounds.height - 1.0).abs() < 0.00001);
    assert!(!editor.canvas.drawing);
}

#[test]
fn imprecise_adjacent_draws_dock_without_overlap_and_resize_cannot_cross_neighbour() {
    let mut editor = Editor::new();
    let left = 281.33334;
    let top = 40.0;
    let width = 437.33334;
    let height = 246.0;
    editor.canvas.drawing = true;
    editor.frame(Vec::new());
    editor.pointer(left + 2.0, top + 2.0, Some(true));
    editor.pointer(left + width * 0.5 + 4.0, top + height - 2.0, None);
    editor.pointer(left + width * 0.5 + 4.0, top + height - 2.0, Some(false));
    editor.canvas.drawing = true;
    editor.frame(Vec::new());
    editor.pointer(left + width * 0.5 + 3.0, top + 2.0, Some(true));
    editor.pointer(left + width - 2.0, top + height - 2.0, None);
    editor.pointer(left + width - 2.0, top + height - 2.0, Some(false));
    assert_eq!(editor.view.zones.len(), 2);
    assert_eq!(values(editor.view.zones[0].bounds), [0.0, 0.0, 0.5, 1.0]);
    assert_eq!(values(editor.view.zones[1].bounds), [0.5, 0.0, 0.5, 1.0]);
    editor.pointer(left + width * 0.5 - 5.0, top + height - 5.0, Some(true));
    editor.pointer(left + width * 0.7, top + height - 5.0, None);
    editor.pointer(left + width * 0.7, top + height - 5.0, Some(false));
    assert!(!has_overlaps(&editor.view));
    assert_eq!(editor.view.zones[0].bounds.width, 0.5);
}

#[test]
fn slow_drag_can_leave_a_snap_line_without_sticking_or_changing_size() {
    let mut editor = Editor::new();
    editor.draw(Pos2::new(370.0, 120.0));
    editor.view.zones[0].bounds = Bounds {
        x: 0.5,
        y: 0.25,
        width: 0.25,
        height: 0.5,
    };
    editor.frame(Vec::new());
    editor.pointer(530.0, 150.0, Some(true));
    for x in (532..=550).step_by(2) {
        editor.pointer(x as f32, 150.0, None);
    }
    editor.pointer(550.0, 150.0, Some(false));
    let result = editor.view.zones[0].bounds;
    assert!(
        result.x > 0.53,
        "Drag stuck to the original guide: {}",
        result.x
    );
    assert_eq!((result.width, result.height), (0.25, 0.5));
}

#[test]
fn align_edges_repairs_existing_overlap_without_changing_assignments() {
    let mut editor = Editor::new();
    editor.draw(Pos2::new(370.0, 120.0));
    editor.view.zones[0].bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 0.52616876,
        height: 1.0,
    };
    editor.view.zones[0].command = "keep this command".into();
    let mut right = editor.view.zones[0].clone();
    right.name = "Right".into();
    right.bounds = Bounds {
        x: 0.50520825,
        y: 0.0,
        width: 0.49479175,
        height: 1.0,
    };
    editor.view.zones.push(right);
    // Same four-monitor preview scale as the user's canvas.
    editor
        .canvas
        .sizes
        .insert("DP-1".into(), Vec2::new(205.0, 130.0));
    assert!(has_overlaps(&editor.view));
    assert!(editor.canvas.align(&mut editor.view));
    assert!(!has_overlaps(&editor.view));
    assert_eq!(values(editor.view.zones[0].bounds), [0.0, 0.0, 0.5, 1.0]);
    assert_eq!(values(editor.view.zones[1].bounds), [0.5, 0.0, 0.5, 1.0]);
    assert!(editor
        .view
        .zones
        .iter()
        .all(|zone| zone.command == "keep this command"));
    assert_eq!(editor.view.zones[1].name, "Right");
}

impl Editor {
    fn multi_monitor() -> Self {
        let mut editor = Self::new();
        editor.draw(Pos2::new(370.0, 120.0));
        editor.monitors = vec![
            Monitor {
                name: "DP-1".into(),
                x: -1920,
                y: -1080,
                width: 1920,
                height: 1080,
                scale: 1.5,
            },
            Monitor {
                name: "DP-2".into(),
                x: 0,
                y: -1080,
                width: 2560,
                height: 1440,
                scale: 2.0,
            },
            Monitor {
                name: "DP-3".into(),
                x: -1920,
                y: 800,
                width: 1920,
                height: 1200,
                scale: 1.0,
            },
        ];
        editor.view.zones[0].bounds = Bounds {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 0.5,
        };
        editor.view.zones[0].command = "keep program".into();
        editor.view.zones[0].application = Some(crate::core::config::Application {
            id: "org.gnome.Ptyxis".into(),
            name: "Terminal".into(),
        });
        editor.view.zones[0].terminal_command = "herdr".into();
        editor.view.zones[0].delay_ms = 200;
        editor.frame(Vec::new());
        editor
    }

    fn screen(&self, name: &str) -> Rect {
        // Fixed 1000x600 test surface: canvas is 984x310 at (8,8).
        // Desktop extents are (-1920,-1080)..(2560,2000).
        let factor = 224.0 / 3080.0;
        let origin = Pos2::new(500.0, 163.0)
            - (Vec2::new(4480.0, 3080.0) * factor + Vec2::new(0.0, 22.0)) / 2.0;
        let monitor = self
            .monitors
            .iter()
            .find(|monitor| monitor.name == name)
            .unwrap();
        Rect::from_min_size(
            origin
                + Vec2::new((monitor.x + 1920) as f32, (monitor.y + 1080) as f32) * factor
                + Vec2::new(0.0, if monitor.y == 800 { 22.0 } else { 0.0 }),
            Vec2::new(monitor.width as f32, monitor.height as f32) * factor,
        )
    }

    fn transfer(&mut self, source: &str, target: &str) {
        let from = self.screen(source).lerp_inside(Vec2::splat(0.25));
        let to = self.screen(target).lerp_inside(Vec2::splat(0.75));
        self.pointer(from.x, from.y, Some(true));
        self.pointer(to.x, to.y, None);
        self.pointer(to.x, to.y, Some(false));
        self.frame(Vec::new());
    }
}

#[test]
fn zone_moves_between_monitors_in_both_directions_and_persists_connector() {
    let mut editor = Editor::multi_monitor();
    let mut metadata = serde_json::to_value(&editor.view.zones[0]).unwrap();
    metadata.as_object_mut().unwrap().remove("monitor");
    metadata.as_object_mut().unwrap().remove("bounds");
    editor.transfer("DP-1", "DP-2");
    let mut moved_metadata = serde_json::to_value(&editor.view.zones[0]).unwrap();
    moved_metadata.as_object_mut().unwrap().remove("monitor");
    moved_metadata.as_object_mut().unwrap().remove("bounds");
    assert_eq!(moved_metadata, metadata);
    assert_eq!(editor.view.zones.len(), 1);
    assert_eq!(editor.view.zones[0].monitor, "DP-2");
    assert_eq!(values(editor.view.zones[0].bounds), [0.5, 0.5, 0.5, 0.5]);
    assert_eq!(editor.selected, Some(0));
    // Grab the moved zone at its center and return onto the earlier-rendered monitor.
    let from = editor.screen("DP-2").lerp_inside(Vec2::splat(0.75));
    let to = editor.screen("DP-1").lerp_inside(Vec2::splat(0.25));
    editor.pointer(from.x, from.y, Some(true));
    editor.pointer(to.x, to.y, None);
    editor.pointer(to.x, to.y, Some(false));
    assert_eq!(editor.view.zones[0].monitor, "DP-1");
    assert_eq!(values(editor.view.zones[0].bounds), [0.0, 0.0, 0.5, 0.5]);
    assert_eq!(editor.view.zones[0].command, "keep program");
    let saved = serde_json::to_string(&editor.view).unwrap();
    let restored: View = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored.zones[0].monitor, "DP-1");
    assert_eq!(values(restored.zones[0].bounds), [0.0, 0.0, 0.5, 0.5]);
}

#[test]
fn transfer_crosses_monitor_gaps_and_works_with_snapping_disabled() {
    let mut editor = Editor::multi_monitor();
    editor.canvas.snap_enabled = false;
    editor.transfer("DP-1", "DP-3");
    assert_eq!(editor.view.zones[0].monitor, "DP-3");
    let bounds = editor.view.zones[0].bounds;
    assert!(bounds.valid());
    assert_eq!((bounds.width, bounds.height), (0.5, 0.5));
    assert!((bounds.x - 0.5).abs() < 0.00001 && (bounds.y - 0.5).abs() < 0.00001);
}

#[test]
fn occupied_monitor_rejects_transfer_and_escape_restores_original_monitor() {
    let mut editor = Editor::multi_monitor();
    let mut occupied = editor.view.zones[0].clone();
    occupied.monitor = "DP-2".into();
    occupied.bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };
    editor.view.zones.push(occupied);
    editor.frame(Vec::new());
    editor.transfer("DP-1", "DP-2");
    assert_eq!(editor.view.zones[0].monitor, "DP-1");
    assert!(!has_overlaps(&editor.view));
    assert!(editor
        .canvas
        .feedback
        .as_deref()
        .unwrap()
        .contains("No free space"));
    let from = editor.screen("DP-1").lerp_inside(Vec2::splat(0.25));
    let to = editor.screen("DP-3").lerp_inside(Vec2::splat(0.75));
    editor.pointer(from.x, from.y, Some(true));
    editor.pointer(to.x, to.y, None);
    editor.frame(Vec::new());
    assert_eq!(editor.view.zones[0].monitor, "DP-3");
    editor.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    editor.pointer(to.x, to.y, Some(false));
    assert_eq!(editor.view.zones[0].monitor, "DP-1");
    assert_eq!(values(editor.view.zones[0].bounds), [0.0, 0.0, 0.5, 0.5]);
}

#[test]
fn transfer_docks_in_free_space_beside_target_monitor_zone() {
    let mut editor = Editor::multi_monitor();
    editor.view.zones[0].bounds.height = 1.0;
    let mut neighbour = editor.view.zones[0].clone();
    neighbour.monitor = "DP-2".into();
    neighbour.bounds.x = 0.5;
    editor.view.zones.push(neighbour);
    editor.frame(Vec::new());
    editor.transfer("DP-1", "DP-2");
    assert_eq!(editor.view.zones[0].monitor, "DP-2");
    assert_eq!(values(editor.view.zones[0].bounds), [0.0, 0.0, 0.5, 1.0]);
    assert_eq!(values(editor.view.zones[1].bounds), [0.5, 0.0, 0.5, 1.0]);
    assert!(!has_overlaps(&editor.view));
}

#[test]
fn monitor_captions_have_separate_space_without_distorting_screens() {
    let monitors = vec![
        Monitor {
            name: "Portrait".into(),
            x: -1080,
            y: 0,
            width: 1080,
            height: 1920,
            scale: 1.5,
        },
        Monitor {
            name: "Top".into(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale: 1.0,
        },
        Monitor {
            name: "Bottom".into(),
            x: 0,
            y: 1080,
            width: 1920,
            height: 1080,
            scale: 2.0,
        },
    ];
    let area = Rect::from_min_size(Pos2::new(8.0, 8.0), Vec2::new(984.0, 310.0));
    let screens = preview_screens(area, &monitors);
    for (monitor, screen) in &screens {
        let caption = monitor_caption(*screen);
        assert!(area.contains_rect(caption));
        assert!(area.contains_rect(*screen));
        assert!(
            (screen.aspect_ratio() - monitor.width as f32 / monitor.height as f32).abs() < 0.00001
        );
        for (_, other) in &screens {
            assert!(
                !caption.intersects(*other),
                "caption overlaps editable screen"
            );
        }
        let full_zone = zone_rect(
            *screen,
            Bounds {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
        );
        assert_eq!(full_zone, *screen);
    }
}
