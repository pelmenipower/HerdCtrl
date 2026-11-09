# HerdCtrl

A native Rust desktop application for arranging application windows into named workspaces. The product specification is [PRD.md](PRD.md); contributors must follow [AGENTS.md](AGENTS.md).

## Build and run

Platforms: Ubuntu with an **X11** desktop session and an EWMH-compatible window manager, or **GNOME 50 on Wayland** with the bundled HerdCtrl Shell extension. Other Wayland compositors are not supported.

```sh
sudo apt-get install build-essential pkg-config libx11-dev libxi-dev libxcursor-dev libxrandr-dev libxkbcommon-dev libxkbcommon-x11-0 libxcb-xkb1 libgl1-mesa-dev wmctrl x11-xserver-utils
cargo build --release --locked
./target/release/herdctrl
```

Install a stable Rust toolchain using rustup before building. X11 runtime tools are `xrandr`, `wmctrl`, and GNU `timeout`. GNOME/Wayland talks to the Shell extension over the session D-Bus directly instead of using `wmctrl`; `scripts/check-gnome.py` additionally needs `gdbus` (Ubuntu package `libglib2.0-bin`). The current Rust GUI renders through X11/XWayland and system OpenGL libraries; the GNOME bridge can control both native Wayland and XWayland windows. This application has no Electron or Node.js runtime.

## GNOME 50 / Wayland setup

Install and enable the bundled extension once, from a terminal in this repository:

```sh
bash scripts/install-gnome-extension.sh
```

This installs the extension for your user with GNOME's own installer. If GNOME has not discovered it yet, log out and back in, then enable it:

```sh
gnome-extensions enable herdctrl@herdctrl.local
```

When updating an already loaded extension, log out and back in to load the new code. Start HerdCtrl and select **Refresh displays**. The status should say **GNOME / Wayland**, and connected monitors should appear in the canvas. A read-only diagnostic is available:

```sh
python3 scripts/check-gnome.py
```

Global shortcuts use compositor grabs via the extension. Conflicts appear in Activity. Keep HerdCtrl open; stale grabs expire within about 20 seconds after a crash or exit, and disabling the extension releases them immediately. Only one HerdCtrl instance can own GNOME shortcuts at a time, with up to 256 configured shortcuts. GNOME shortcuts identify letter keys according to the current keyboard layout; the X11 backend uses physical key codes.

To build the distributable extension ZIP separately, run `python3 scripts/package-gnome-extension.py`. It is written to `target/herdctrl@herdctrl.local.shell-extension.zip`. GNOME 50 is the compatibility target. Launching, resizing and monitor assignment have been tested on GNOME 50.1 with four physical displays; remaining acceptance limits are recorded in `VALIDATION.md`.

If startup reports `Library libxkbcommon-x11.so could not be loaded`, install both `libxkbcommon-x11-0` and `libxcb-xkb1`. These libraries are required even in a Wayland session because the Rust GUI uses XWayland.

An opt-in live launch-engine test opens temporary undecorated GTK windows on every connected monitor, using both native Wayland and XWayland. It checks dimensions and monitor assignment, exact XWayland coordinates, no-window timeouts and disconnected-target rejection. Requires `python3-gi`, `gir1.2-gtk-4.0` and `xwininfo` (`x11-utils`); test windows close automatically. It does not change saved profiles:

```sh
HERDCTRL_LIVE_GNOME=1 cargo test --locked gnome_live_launch_and_resize_on_each_monitor -- --ignored --nocapture --test-threads=1
```

The report is written to `target/gnome-live-acceptance.json`. Native Wayland absolute window origins, fractional scaling and monitor hotplug still require separate acceptance.

Monitor identification uses temporary native viewports from the same executable and the existing backend for their placement. It needs no extension update or additional runtime. To check the actual window positions, automatic expiry and cancellation on GNOME, build the release executable first, then run:

```sh
cargo build --release --locked
HERDCTRL_LIVE_GNOME=1 cargo test --locked gnome_live_identification_positions_and_cleans_up_labels -- --ignored --nocapture --test-threads=1
```

The test also requires `xprop` and `xwininfo` from `x11-utils`. It opens labels twice and leaves saved layouts unchanged.

## Use

1. Create a named view.
2. Select **Draw a zone**, then drag a rectangle on a display. The application picker opens automatically.
3. Search for an installed application and select it, or press Enter to select the first search result. Its name appears on the zone. Select a zone and use **Change…** to choose another app. Drag zones within a monitor or directly onto another monitor; resize with the bottom-right handle. A transfer keeps the relative size and application assignment, updates the Monitor property, and snaps into free space on the destination. A full destination leaves the zone at its last valid position; Escape restores the original monitor and bounds. **Advanced** contains optional custom commands, launch delay and numeric bounds (fractions from 0 to 1).
4. Optionally assign a global hotkey: toggle the modifiers (Super, Ctrl, Alt, Shift), click **Set key…** and press the key. Escape cancels recording; focusing a text field stops it; unsupported keys show a hint. **Clear** removes the hotkey. At least one modifier is required. The layout stores it as text such as `Super+Shift+KeyD`.
5. Save layouts, then launch the selected view. The tray process picks up saved hotkeys within about a second.

**Delete view** asks for confirmation. The deletion, and the changed hotkey assignment of the remaining views, takes effect when you save.

### Tray and global hotkeys

Global hotkeys are owned by a small background process, `herdctrl --tray`, with an icon in the top bar (on GNOME this needs the AppIndicator extension, enabled by default on Ubuntu). It has no editor window or renderer, so it stays running with little memory after you close the editor. Starting `herdctrl` opens the editor and starts the tray if it is not running yet; saving layouts restarts it if you quit it. The tray reloads `layouts.json` when it changes.

The tray menu lists every view with its hotkey to launch it, plus **Edit layouts…**, **Start at login** (writes or removes `~/.config/autostart/herdctrl.desktop`) and **Quit HerdCtrl**. Only one tray and one editor run per layouts file; a second start exits. Successful placements stay quiet; launch errors, taken shortcuts and configuration errors appear as desktop notifications. The desktop may already reserve a shortcut.

**Duplicate view** in the sidebar copies the selected view and opens the copy for editing. It keeps all zones, apps, terminal programs, monitor assignments and bounds. Copies get a unique name and an empty hotkey to avoid shortcut conflicts. Edit the name as needed, then Save layouts.

The zone entries below the monitors can be dragged to reorder them within the current view. A marker shows where the entry will land, including across wrapped rows. Click an entry to edit it; press Escape or drop outside the list to cancel a drag. Save layouts to keep the new order. The selected zone, its app and its monitor bounds travel with the entry.

Monitor numbers and connector names appear as small captions above each screen preview, with space between preview rows so captions cannot cover zones. Hover a caption for its resolution.

**Identify monitors** briefly shows a large matching number on every screen. Use those same numbers in the canvas and each zone's Monitor menu; **Hide numbers** dismisses the labels early. Numbers follow the current screen arrangement (top to bottom, left to right). Layouts continue to store connector names, so renumbering after a display change does not reassign a saved zone.

**Snap zones** is enabled by default. Drawing, moving and resizing dock to screen borders, halves, thirds, quarters and neighbouring edges while avoiding overlaps. Guides appear during a gesture. **Align edges** tidies existing imprecise zones; overlapping zones are highlighted and explained. Very crowded layouts may still require manual resizing. Turn snapping off for deliberate free placement; exact numeric edits under Advanced remain available. Overlapping saved/numeric layouts are preserved and flagged rather than silently rewritten.

Launching a view arranges an already open window of each selected application. A missing application window is started automatically. If several matching windows exist, HerdCtrl chooses the lowest stable window ID not already assigned to another zone in that view; it does not use window titles to guess. Repeated launches reuse those windows.

For a terminal session, select **Terminal**, then enter the program under **Program in terminal**, for example `herdr`. HerdCtrl opens the terminal and runs that program inside it. Repeated launches reuse the matching session. Existing simple terminal programs can also be recognized when the terminal process owns a single window; shared multi-window terminal processes are treated conservatively. For a custom command, **Advanced → Run in terminal** uses your default terminal instead (the `xdg-terminal-exec` choice, otherwise the first installed of Ptyxis, GNOME Terminal, Console, Ghostty, Konsole, Alacritty, kitty, WezTerm or foot). Enabling this is necessary for interactive programs entered as custom commands.

Views are stored in `$XDG_CONFIG_HOME/herdctrl/layouts.json`, falling back to `~/.config/herdctrl/layouts.json`. Saves use a private temporary file and atomic rename. Invalid existing configuration causes a startup error and is not silently overwritten. A stale `layouts.json.tmp` after interruption must be inspected and removed before saving again. Run one HerdCtrl instance per configuration directory.

Application discovery reads the current user's and system's XDG desktop entries on a background worker, including localized names and desktop visibility settings. The layout stores the selected application's ID and name; launch workers resolve its current executable and arguments. Applications launch directly without a shell. New GNOME Terminal (Ptyxis) and Text Editor instances use their independent-instance option so their windows can be matched reliably. Terminal programs run through `sh -c` inside the selected terminal. A removed application produces a message asking you to choose another one.

Custom commands remain optional under **Advanced** and run through `sh -c` with the current user's privileges. Entering one replaces the application selection. Only launch trusted commands. For custom terminal sessions, include the terminal executable, for example `xterm -e htop`. Existing command-based layouts still load.

## Window matching and limitations

Launch workers run outside the GUI thread. They match existing application windows using a dedicated native executable or desktop-entry process identity. Shared interpreters alone cannot identify an app, and terminal-program sessions are matched separately. Each window is reserved once per view; complete view runs are serialized on a worker to avoid duplicate launches on rapid repeated activation. If no eligible window exists, workers track the spawned process and descendants, including new windows delegated to an identifiable existing application process. X11 uses `wmctrl`; GNOME uses the extension's Mutter window API. Both recheck window PID before movement. Polling expires after five seconds following launch delay; individual helper calls have a two-second timeout. Launched applications remain running after a positioning timeout.

Some wrapper scripts, sandboxed apps and shared terminal processes cannot be identified confidently. HerdCtrl does not guess from a title; it can start a new window or report a timeout in those cases. If multiple zones need the same application, existing windows are used first; each further zone opens another window. All zones of the same selected application (including terminal zones with a program) launch one after another, so each costs up to its launch delay plus five seconds. Apps that are already running are started through their desktop entry's new-window action when they provide one. An app that is running but has no such action usually only focuses its existing window; that zone then reports that no second window opened. A new window is first matched to the zone's own process tree; terminal servers that host all windows in one process (e.g. GNOME Console, GNOME Terminal) are matched by launch order. Mixing an application zone and a custom command for the same program in one view is not supported, because their windows cannot be told apart reliably. For custom non-terminal commands, use a new-instance option when needed. Very short-lived intermediaries may exit before descendant discovery. Window managers and applications can enforce minimum sizes or adjust decorations, so pixel-perfect outer-window placement needs integration verification on the target desktop.

On X11, monitor geometry comes from `xrandr --verbose`, including negative offsets. The canvas displays the XRandR horizontal transform factor; per-monitor toolkit UI scale, nonuniform transforms and arbitrary rotation are not fully modeled. Placement uses X11 pixel coordinates. On GNOME/Wayland, the extension supplies connector names, logical monitor rectangles and compositor scale. Placement uses the same logical coordinates without multiplying scale again; mirrored connectors share a logical monitor. Disconnected monitors cause a visible error instead of silently redirecting a zone.

## Development checks

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
node scripts/gnome-controller.test.mjs
python3 scripts/package-gnome-extension.py
```

CI runs these checks on Ubuntu and uploads the compiled binary and extension bundle as workflow artifacts. Node.js is needed only for the compositor-independent extension tests. CI does not publish releases.

The window icon is embedded from `assets/icon.png`; `assets/icon.svg` is its editable source. To regenerate the 256-pixel PNG with the optional `librsvg2-bin` development tool:

```sh
rsvg-convert -w 256 -h 256 assets/icon.svg -o assets/icon.png
```

See [VALIDATION.md](VALIDATION.md) for measured results and remaining PRD acceptance work.

## X11 acceptance checks

On an Ubuntu X11 desktop, install the test tools in addition to the runtime dependencies:

```sh
sudo apt-get install xterm xdotool
cargo test --locked x11_ -- --ignored --test-threads=1 --nocapture
cargo build --release --locked
python3 scripts/check-x11.py --output /tmp/herdctrl-x11-acceptance.json
```

The opt-in Rust tests launch temporary terminals on each active monitor, compare their geometry within a 32-pixel tolerance for decorations and character sizing, check that an unrelated window stays put, verify the no-window timeout, and reject a disconnected target before executing its command. Test terminals close after the test or expire automatically. Do not interact with the test windows while the checks run.

The Python check uses a temporary configuration, starts and stops its own HerdCtrl instances, triggers `Ctrl+Alt+Shift+F10` while another application is focused, and records warm startup to a visible window plus RSS samples. Keep that shortcut free for the test. Visibility is a startup proxy; the PRD first-frame target still requires a separate measurement. Your saved layouts are not changed.

CI also runs these checks under Xvfb and Openbox and uploads a JSON measurement artifact. Virtual-display results supplement the physical multi-monitor, scaling and editor checks in `VALIDATION.md`.
