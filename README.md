# HerdCtrl

A native Rust desktop application for arranging application windows into named workspaces. The product specification is [PRD.md](PRD.md); contributors must follow [AGENTS.md](AGENTS.md).

## Build and run

Initial platform: Ubuntu with an **X11** desktop session and an EWMH-compatible window manager. Wayland window control is outside the initial scope.

```sh
sudo apt-get install build-essential pkg-config libx11-dev libxi-dev libxcursor-dev libxrandr-dev libxkbcommon-dev libxkbcommon-x11-0 libgl1-mesa-dev wmctrl x11-xserver-utils
cargo build --release --locked
./target/release/herdctrl
```

Install a stable Rust toolchain using rustup before building. Runtime tools are `xrandr`, `wmctrl`, and GNU `timeout`; the GUI also uses the system X11/OpenGL libraries. This application has no Electron or Node.js runtime.

## Use

1. Create a named view.
2. Select **Draw a zone**, then drag a rectangle on a display.
3. Select a zone to edit its name, command, monitor and launch delay. Move zones by dragging; resize with the bottom-right handle. Numeric bounds use fractions from 0 to 1.
4. Optionally assign a global hotkey such as `Super+Shift+KeyD`.
5. Save layouts to activate the hotkey bindings, then launch the selected view. Keep HerdCtrl running for global shortcuts to work. The desktop may already reserve a shortcut; registration failures appear in Activity.

Views are stored in `$XDG_CONFIG_HOME/herdctrl/layouts.json`, falling back to `~/.config/herdctrl/layouts.json`. Saves use a private temporary file and atomic rename. Invalid existing configuration causes a startup error and is not silently overwritten. A stale `layouts.json.tmp` after interruption must be inspected and removed before saving again. Run one HerdCtrl instance per configuration directory.

Commands run through `sh -c` with the current user's privileges. Only launch trusted commands. For terminal sessions, include the terminal executable, for example `xterm -e htop`.

## Window matching and limitations

Launch workers run outside the GUI thread. Each records existing windows, tracks the launched process and observed descendants, waits for a new window belonging to those processes, removes maximization, and asks `wmctrl` to move and resize it. Polling expires after five seconds following the configured launch delay; individual X11 helper calls have a two-second timeout. Launched applications remain running after a positioning timeout.

Single-instance applications can delegate window creation to an existing process. Use their new-instance option when available; HerdCtrl deliberately avoids guessing by window title or moving unrelated windows. Very short-lived intermediaries may exit before descendant discovery. Window managers and applications can enforce minimum sizes or adjust decorations, so pixel-perfect outer-window placement needs integration verification on the target desktop.

Monitor geometry comes from `xrandr --verbose`, including negative offsets. The canvas displays the XRandR horizontal transform factor; per-monitor toolkit UI scale, nonuniform transforms and arbitrary rotation are not fully modeled. Zone placement uses X11 pixel coordinates, avoiding double application of desktop scaling. Disconnected monitors cause a visible error instead of silently redirecting a zone.

## Development checks

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

CI runs these checks on Ubuntu and uploads the compiled binary as a workflow artifact. It does not publish releases.

See [VALIDATION.md](VALIDATION.md) for measured results and remaining PRD acceptance work.
