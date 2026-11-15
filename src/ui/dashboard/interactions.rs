//! Drives the real editor with simulated pointer and keyboard input; no GPU needed.
use super::snapshots::{dashboard, harness};
use super::*;
use egui_kittest::kittest::Queryable;

fn temp_layouts(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "herdctrl-ui-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    directory.join("layouts.json")
}

#[test]
fn plus_adds_a_view_and_starts_drawing() {
    let mut harness = harness(dashboard(PathBuf::new()), Theme::Dark);
    harness.get_by_label("+").click();
    harness.run_steps(2);
    let state = harness.state();
    assert_eq!(state.config.views.len(), 3);
    assert_eq!(state.selected_view, 2);
    assert!(state.canvas.drawing);
    assert!(state.dirty);
}

#[test]
fn ctrl_s_saves_and_clears_the_unsaved_state() {
    let path = temp_layouts("save");
    let mut harness = harness(dashboard(path.clone()), Theme::Light);
    harness.state_mut().config.views[0].name = "Desk".into();
    harness.state_mut().dirty = true;
    harness.run_steps(1);
    harness.press_key_modifiers(egui::Modifiers::COMMAND, egui::Key::S);
    harness.run_steps(2);
    assert!(!harness.state().dirty);
    let saved = Config::load(&path).unwrap();
    assert_eq!(saved.views[0].name, "Desk");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn delete_view_goes_through_the_menu_and_a_confirmation() {
    let mut harness = harness(dashboard(PathBuf::new()), Theme::Dark);
    harness.get_by_label("…").click();
    harness.run_steps(2);
    harness.get_by_label("Delete view…").click();
    harness.run_steps(2);
    assert_eq!(harness.state().confirm_delete, Some(0));
    assert_eq!(
        harness.state().config.views.len(),
        2,
        "not before confirming"
    );
    harness.get_by_label("Delete view").click();
    harness.run_steps(2);
    let state = harness.state();
    assert_eq!(state.config.views.len(), 1);
    assert_eq!(state.config.views[0].name, "Writing");
    assert!(state.dirty);
}

#[test]
fn picker_moves_with_arrow_keys_and_chooses_with_enter() {
    let mut start = dashboard(PathBuf::new());
    // Zone 1 holds Zen; the picker lists apps used in views first, Zen at the top.
    start.app_picker = Some(AppPicker::new(0, 0));
    let mut harness = harness(start, Theme::Dark);
    harness.press_key(egui::Key::ArrowDown);
    harness.run_steps(1);
    harness.press_key(egui::Key::Enter);
    harness.run_steps(2);
    let state = harness.state();
    assert!(state.app_picker.is_none());
    let chosen = state.config.views[0].zones[0].application.as_ref().unwrap();
    assert_eq!(chosen.id, "org.gnome.Ptyxis");
    assert!(state.dirty);
}
