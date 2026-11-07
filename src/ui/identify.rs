//! Short-lived native label windows, placed only through the selected OS backend.
//! A single helper process creates one viewport at a time so each compositor
//! handle can be associated with a monitor without matching unrelated titles.
use crate::{
    core::monitor::{sort_for_display, Monitor},
    os::Backend,
};
use eframe::egui::{self, Color32, RichText};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const SHOW_TIME: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Label {
    number: usize,
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn labels(monitors: &[Monitor]) -> Vec<Label> {
    monitors
        .iter()
        .enumerate()
        .map(|(index, m)| Label {
            number: index + 1,
            name: m.name.clone(),
            x: m.x,
            y: m.y,
            width: m.width,
            height: m.height,
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct Placement {
    pub number: usize,
    pub monitor: String,
}

pub enum Event {
    Shown(Vec<Placement>),
    Finished(Result<(), String>),
}

#[derive(Default)]
pub struct Identification {
    stop: Option<mpsc::Sender<()>>,
    events: Option<mpsc::Receiver<Event>>,
}

impl Identification {
    pub fn active(&self) -> bool {
        self.events.is_some()
    }
    pub fn start(&mut self, backend: Backend, monitors: Vec<Monitor>) {
        if self.active() {
            return;
        }
        let (stop, cancel) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        self.stop = Some(stop);
        self.events = Some(rx);
        thread::spawn(move || {
            let result = std::env::current_exe()
                .map_err(|error| error.to_string())
                .and_then(|binary| session(&binary, backend, &monitors, &cancel, &tx));
            let _ = tx.send(Event::Finished(result));
        });
    }
    pub fn stop(&mut self) {
        if let Some(stop) = &self.stop {
            let _ = stop.send(());
        }
    }
    pub fn poll(&mut self) -> Option<Event> {
        let event = match self.events.as_ref()?.try_recv() {
            Ok(event) => event,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Event::Finished(Err("Monitor identification ended unexpectedly".into()))
            }
        };
        if matches!(event, Event::Finished(_)) {
            self.events = None;
            self.stop = None;
        }
        Some(event)
    }
}
impl Drop for Identification {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Helper(Child);
impl Drop for Helper {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn cancelled(cancel: &mpsc::Receiver<()>) -> bool {
    !matches!(cancel.try_recv(), Err(mpsc::TryRecvError::Empty))
}

fn session(
    binary: &Path,
    backend: Backend,
    monitors: &[Monitor],
    cancel: &mpsc::Receiver<()>,
    events: &mpsc::Sender<Event>,
) -> Result<(), String> {
    if monitors.is_empty() || monitors.len() > 32 {
        return Err("Identify requires between 1 and 32 connected monitors".into());
    }
    let mut current = backend.monitors()?;
    sort_for_display(&mut current);
    if current.len() != monitors.len()
        || current.iter().zip(monitors).any(|(a, b)| {
            (a.name.as_str(), a.x, a.y, a.width, a.height, a.scale)
                != (b.name.as_str(), b.x, b.y, b.width, b.height, b.scale)
        })
    {
        return Err("Displays changed. Refresh displays, then identify them again.".into());
    }
    let raw = serde_json::to_string(&labels(monitors)).map_err(|error| error.to_string())?;
    let mut helper = Helper(
        Command::new(binary)
            .args(["--identify-monitors", &raw])
            .env_remove("DESKTOP_STARTUP_ID")
            .env_remove("XDG_ACTIVATION_TOKEN")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| error.to_string())?,
    );
    let pid = helper.0.id();
    let mut known = HashSet::new();
    let mut placed = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    for (index, monitor) in monitors.iter().enumerate() {
        let window = loop {
            if cancelled(cancel) {
                return Ok(());
            }
            if helper
                .0
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                return Err("Monitor label window could not start".into());
            }
            let windows: Vec<_> = backend
                .windows()?
                .into_iter()
                .filter(|window| window.pid == pid && !known.contains(&window.id))
                .collect();
            if windows.len() > 1 {
                return Err("Could not associate a monitor label with its window".into());
            }
            if let Some(window) = windows.into_iter().next() {
                break window;
            }
            if Instant::now() >= deadline {
                return Err("Monitor label window did not appear in time".into());
            }
            thread::sleep(Duration::from_millis(50));
        };
        let width = 260.min(monitor.width);
        let height = 180.min(monitor.height);
        let x = monitor
            .x
            .checked_add(((monitor.width - width) / 2) as i32)
            .ok_or("Invalid monitor coordinates")?;
        let y = monitor
            .y
            .checked_add(((monitor.height - height) / 2) as i32)
            .ok_or("Invalid monitor coordinates")?;
        backend.position(&window, monitor, (x, y, width, height))?;
        known.insert(window.id.clone());
        placed.push(Placement {
            number: index + 1,
            monitor: monitor.name.clone(),
        });
        if index + 1 < monitors.len() {
            helper
                .0
                .stdin
                .as_mut()
                .ok_or("Monitor label channel closed")?
                .write_all(b"N")
                .map_err(|error| error.to_string())?;
        }
    }
    helper
        .0
        .stdin
        .as_mut()
        .ok_or("Monitor label channel closed")?
        .write_all(b"S")
        .map_err(|error| error.to_string())?;
    let _ = events.send(Event::Shown(placed));
    let deadline = Instant::now() + SHOW_TIME + Duration::from_secs(2);
    loop {
        if cancelled(cancel) {
            return Ok(());
        }
        if let Some(status) = helper.0.try_wait().map_err(|error| error.to_string())? {
            return if status.success() {
                Ok(())
            } else {
                Err("Monitor labels closed unexpectedly".into())
            };
        }
        if Instant::now() >= deadline {
            return Err("Monitor labels timed out while closing".into());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

struct State {
    created: usize,
    shown: Option<Instant>,
    cancelled: bool,
}
struct LabelsApp {
    labels: Vec<Label>,
    state: Arc<Mutex<State>>,
    commands: mpsc::Receiver<u8>,
    started: Instant,
}

pub fn run_helper(raw: &str) -> Result<(), Box<dyn std::error::Error>> {
    let labels: Vec<Label> = serde_json::from_str(raw)?;
    if labels.is_empty()
        || labels.len() > 32
        || labels.iter().enumerate().any(|(index, label)| {
            label.number != index + 1
                || label.name.is_empty()
                || label.width == 0
                || label.height == 0
        })
    {
        return Err("Invalid monitor labels".into());
    }
    let (tx, commands) = mpsc::channel();
    thread::spawn(move || {
        let mut byte = [0u8; 1];
        let mut stdin = std::io::stdin().lock();
        while stdin.read_exact(&mut byte).is_ok() {
            if tx.send(byte[0]).is_err() {
                return;
            }
        }
        let _ = tx.send(b'Q');
    });
    let options = eframe::NativeOptions {
        viewport: builder(&labels[0]),
        ..Default::default()
    };
    eframe::run_native(
        "HerdCtrl monitor identification",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_embed_viewports(false);
            Ok(Box::new(LabelsApp {
                labels,
                state: Arc::new(Mutex::new(State {
                    created: 1,
                    shown: None,
                    cancelled: false,
                })),
                commands,
                started: Instant::now(),
            }))
        }),
    )
    .map_err(Into::into)
}

fn builder(label: &Label) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(format!(
            "HerdCtrl monitor {} ({})",
            label.number, label.name
        ))
        .with_app_id("herdctrl")
        .with_inner_size([260.0, 180.0])
        .with_min_inner_size([1.0, 1.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_mouse_passthrough(true)
        .with_active(false)
        .with_taskbar(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
}

impl eframe::App for LabelsApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            while let Ok(command) = self.commands.try_recv() {
                match command {
                    b'N' => state.created = (state.created + 1).min(self.labels.len()),
                    b'S' => state.shown = Some(Instant::now()),
                    b'Q' => state.cancelled = true,
                    _ => {}
                }
            }
            if self.started.elapsed() > Duration::from_secs(15) {
                state.cancelled = true;
            }
        }
        render(ctx, &self.labels[0], &self.state);
        let created = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .created;
        for (index, label) in self.labels.iter().enumerate().take(created).skip(1) {
            let label = label.clone();
            let state = self.state.clone();
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of(("monitor-label", index)),
                builder(&label),
                move |ctx, _class| {
                    render(ctx, &label, &state);
                },
            );
        }
    }
}

fn render(ctx: &egui::Context, label: &Label, state: &Arc<Mutex<State>>) {
    ctx.request_repaint_after(Duration::from_millis(50));
    let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
    if ctx.input(|input| input.viewport().close_requested() || input.key_pressed(egui::Key::Escape))
    {
        state.cancelled = true;
    }
    if state.cancelled
        || state
            .shown
            .is_some_and(|start| start.elapsed() >= SHOW_TIME)
    {
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
        return;
    }
    let shown = state.shown.is_some();
    drop(state);
    let frame = if shown {
        egui::Frame::NONE
            .fill(Color32::from_rgb(14, 36, 35))
            .stroke(egui::Stroke::new(2.0_f32, Color32::from_rgb(103, 221, 177)))
            .corner_radius(12.0)
            .inner_margin(12.0)
    } else {
        egui::Frame::NONE
    };
    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
        if !shown {
            return;
        }
        ui.vertical_centered(|ui| {
            let font = (ui.available_height() * 0.5).min(72.0);
            ui.label(
                RichText::new(label.number.to_string())
                    .size(font)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.label(
                RichText::new(&label.name)
                    .size(18.0)
                    .color(Color32::from_rgb(166, 235, 210)),
            );
            ui.label(
                RichText::new(format!("{} × {}", label.width, label.height))
                    .color(Color32::LIGHT_GRAY),
            );
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numbers_follow_spatial_order_with_negative_offsets_without_changing_connectors() {
        let mut monitors = vec![
            Monitor {
                name: "DP-right".into(),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                scale: 1.5,
            },
            Monitor {
                name: "DP-left".into(),
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
                scale: 1.0,
            },
            Monitor {
                name: "DP-top".into(),
                x: 0,
                y: -1200,
                width: 1920,
                height: 1200,
                scale: 2.0,
            },
        ];
        sort_for_display(&mut monitors);
        let labels = labels(&monitors);
        assert_eq!(
            labels
                .iter()
                .map(|label| (label.number, label.name.as_str()))
                .collect::<Vec<_>>(),
            [(1, "DP-top"), (2, "DP-left"), (3, "DP-right")]
        );
        assert_eq!(monitors[1].display_name(1), "Monitor 2 · DP-left");
        assert_eq!(
            (labels[0].x, labels[0].y, labels[0].width),
            (0, -1200, 1920)
        );
    }

    #[test]
    #[ignore = "opens native monitor labels; build release and set HERDCTRL_LIVE_GNOME=1"]
    fn gnome_live_identification_positions_and_cleans_up_labels() {
        assert_eq!(std::env::var("HERDCTRL_LIVE_GNOME").as_deref(), Ok("1"));
        assert_eq!(Backend::detect(), Backend::Gnome);
        let binary =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/herdctrl");
        let mut monitors = Backend::Gnome.monitors().unwrap();
        sort_for_display(&mut monitors);
        for cancel_early in [true, false] {
            let before: HashSet<_> = Backend::Gnome
                .windows()
                .unwrap()
                .into_iter()
                .map(|window| window.id)
                .collect();
            let (stop, cancel) = mpsc::channel();
            let (events, rx) = mpsc::channel();
            let copy = monitors.clone();
            let binary = binary.clone();
            let worker =
                thread::spawn(move || session(&binary, Backend::Gnome, &copy, &cancel, &events));
            let Event::Shown(placed) = rx
                .recv_timeout(Duration::from_secs(12))
                .expect("Identification did not show labels")
            else {
                panic!("Expected shown event");
            };
            assert_eq!(placed.len(), monitors.len());
            assert_eq!(
                placed
                    .iter()
                    .map(|label| label.monitor.as_str())
                    .collect::<Vec<_>>(),
                monitors
                    .iter()
                    .map(|monitor| monitor.name.as_str())
                    .collect::<Vec<_>>()
            );
            let added: Vec<_> = Backend::Gnome
                .windows()
                .unwrap()
                .into_iter()
                .filter(|window| !before.contains(&window.id))
                .collect();
            assert_eq!(
                added.len(),
                monitors.len(),
                "Expected one helper viewport per monitor"
            );
            let pid = added[0].pid;
            assert!(added.iter().all(|window| window.pid == pid));
            let xids = crate::os::linux::output("xprop", &["-root", "_NET_CLIENT_LIST"]).unwrap();
            let mut checked = 0;
            for xid in xids
                .split_whitespace()
                .filter(|field| field.starts_with("0x"))
                .map(|field| field.trim_end_matches(','))
            {
                let Ok(owner) = crate::os::linux::output("xprop", &["-id", xid, "_NET_WM_PID"])
                else {
                    continue;
                };
                if owner
                    .rsplit_once(" = ")
                    .and_then(|(_, pid)| pid.trim().parse::<u32>().ok())
                    != Some(pid)
                {
                    continue;
                }
                let title =
                    crate::os::linux::output("xprop", &["-id", xid, "_NET_WM_NAME"]).unwrap();
                for (index, monitor) in monitors.iter().enumerate() {
                    if !title.contains(&format!("monitor {} (", index + 1)) {
                        continue;
                    }
                    let geometry = crate::os::linux::output("xwininfo", &["-id", xid]).unwrap();
                    let value = |key: &str| {
                        geometry
                            .lines()
                            .find_map(|line| line.trim().strip_prefix(key))
                            .unwrap()
                            .trim()
                            .parse::<i32>()
                            .unwrap()
                    };
                    let expected = (
                        monitor.x + ((monitor.width - 260.min(monitor.width)) / 2) as i32,
                        monitor.y + ((monitor.height - 180.min(monitor.height)) / 2) as i32,
                    );
                    assert_eq!(
                        (
                            value("Absolute upper-left X:"),
                            value("Absolute upper-left Y:")
                        ),
                        expected,
                        "Wrong label position for {}",
                        monitor.name
                    );
                    checked += 1;
                }
            }
            assert_eq!(
                checked,
                monitors.len(),
                "Not every numbered window was verified"
            );
            if cancel_early {
                stop.send(()).unwrap();
            }
            worker.join().unwrap().unwrap();
            drop(stop);
            let deadline = Instant::now() + Duration::from_secs(3);
            while Backend::Gnome
                .windows()
                .unwrap()
                .iter()
                .any(|window| window.pid == pid)
            {
                assert!(
                    Instant::now() < deadline,
                    "Label windows were not cleaned up"
                );
                thread::sleep(Duration::from_millis(50));
            }
            eprintln!("Identified {} monitors at their actual centers, cleanup passed (cancel={cancel_early})",checked);
        }
    }
}
