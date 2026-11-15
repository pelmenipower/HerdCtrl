# Changelog

All notable changes to HerdCtrl. Deutsche Fassung: [CHANGELOG.md](CHANGELOG.md). Versions follow [Semantic Versioning](https://semver.org/). Only the maintainer sets versions and publishes releases; until then, new entries go under **Unreleased**.

## [Unreleased] – 0.1.0

First version, developed on `feature/initial-app` from 2026-10-31 to 2026-11-10.

### Editor

- New: draw views as zones on a true-to-scale preview of all monitors. Zones can be moved, resized and dragged onto other monitors. They snap to borders, halves, thirds, quarters and neighbouring zones without overlapping. **Align edges** tidies them up, and overlaps are flagged instead of rewritten.
- New: application picker for installed desktop apps, with search and keyboard control. Apps already in use come first, terminals are marked, and **Custom command…** stays available.
- New: programs inside a terminal (for example `herdr` in Ptyxis) and custom commands with **Run in terminal**. The default terminal comes from `xdg-terminal-exec` or a list of known terminals.
- New: record hotkeys by pressing the whole combination, including Super. Combinations without a modifier are refused.
- New: "Werkbank" interface with dark, light and system themes and embedded Inter and JetBrains Mono fonts. It adds status pills in the header, a zone list in launch order, a properties panel, a first-start checklist, Ctrl+S to save and a prompt on close.
- New: **Identify monitors** shows monitor numbers on the screens. **Duplicate view** copies a view without its hotkey, and **Delete view** asks for confirmation.

### Launching and windows

- New: open windows are reused instead of started twice. Several zones of the same app each get their own window, never guessed from window titles. New windows are opened through the desktop entry's new-window action.
- New: progress per zone (*opening…*, *placed*, *failed*), **Retry zone** for failed zones and a log. Error messages name the app and the timeout.
- New: newly opened windows are placed again after 0.25, 0.75 and 1.75 seconds, because apps such as Ptyxis or Zen restore their remembered size afterwards.
- New: on GNOME, zones fill the work area, so the top bar and dock stay free.
- Fixed: apps that updated themselves (executable shown as *(deleted)*) are recognized again.
- Fixed: launches of the same app run one after another, so zones no longer take each other's windows.

### Memory

- New: the editor draws with Vulkan and opens only the driver of the installed GPU. It needs 28.8 to 30.5 instead of 77.3 MB and is visible after 30 instead of 59 ms (Intel GPU, GNOME 50). egui's bundled fonts are dropped; three symbols were replaced or are painted instead. On laptops with two GPUs, the discrete one stays asleep. Without a usable Vulkan device it falls back to OpenGL by itself (then about 85 MB); `HERDCTRL_RENDERER=glow` forces that. Programs HerdCtrl starts do not inherit the editor's driver selection. The binary grows to 9.2 MB.

### Tray and hotkeys

- New: `herdctrl --tray` owns the global hotkeys with about 2 MB of its own memory, even while the editor is closed. Its menu launches views, opens the editor, toggles **Start at login** and quits HerdCtrl. It picks up configuration changes within about a second and reports errors as desktop notifications.
- Fixed: without a tray host (for example while the screen is locked), the tray's memory grew with every attempt to register the icon (observed: from 1.8 to 19 MB of its own memory during an hour of screen lock). It now waits for a host instead of registering the icon again and again.

### GNOME 50 / Wayland

- New: bundled GNOME Shell extension with a versioned D-Bus interface for monitors, work area, windows and global shortcuts. HerdCtrl calls it directly over D-Bus instead of spawning `gdbus`.

### Documentation and checks

- New: release workflow for `v*` tags. It builds the Linux files, signs `SHA256SUMS` keylessly with Sigstore, adds a GitHub build attestation and only creates a draft for the maintainer to publish. Both manuals describe how to verify a download.
- Docs: README modelled on Shellherd with a gallery, a complete user guide in English and German, the design system, instructions for the screenshots, `CLAUDE.md`, and the `run-app` and `feature-pipeline` skills.
- Tests: interaction tests with `egui_kittest` that need no GPU. Also opt-in screenshots, an X11 integration under Xvfb and Openbox in CI (now measured with `xwininfo`), a Rust↔GJS protocol test and live GNOME tests.
- Measured: see *Memory*; the tray needs 1.8 MB of its own memory. Details in [VALIDATION.md](VALIDATION.md).
