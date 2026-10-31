# Product Requirement Document (PRD): HerdCtrl

**Project Name:** HerdCtrl  
**Version:** 1.0 (Specification)  
**Target OS (Initial):** Ubuntu (Linux / X11)  
**Tech Stack:** Rust (`egui` / `eframe`, `serde_json`, `wmctrl` / X11 API bindings)

---

## 1. Executive Summary

**HerdCtrl** is a native, minimalist, cross-platform desktop application designed to define screen layouts visually, assign applications or terminal commands to designated grid zones, and orchestrate their launch and positioning with a single click or global hotkey.

Derived from the concept of a sheepdog herding its pack, **HerdCtrl** keeps application windows, custom scripts, and terminal sessions strictly organized in their designated places across multi-monitor setups.

---

## 2. Core Objectives

1. **Zero-Bloat Performance:** Built entirely in Rust using `egui` (Immediate Mode GUI), compiling to a lightweight, fast, single executable without heavy web runtimes (No Electron / Node.js).
2. **Multi-Monitor Awareness:** Full detection of multiple physical displays, taking into account native resolution, offsets, and scaling.
3. **Visual Grid Editor:** Interactive canvas to draw, drag, and resize application zones across monitor representations.
4. **Automated Window Herding:** Execute terminal commands or binaries, poll for window handles, and position them accurately via absolute pixel translation derived from percentage-based zones.
5. **Global System Hotkeys:** Trigger complete workspace layouts seamlessly via custom system-wide keyboard shortcuts or an in-app dashboard.

---

## 3. Target Audience

Power users, developers, and sysadmins who frequently open complex setups (e.g., specific terminal sessions running scripts alongside browsers and editors) and need an instant, reliable way to organize them into exact display regions.

---

## 4. Functional Requirements

### 4.1. Display & Multi-Monitor Support
* **Hardware Detection:** Automatically query connected display devices, capturing width, height, scale factor, and position coordinates $(X, Y)$ relative to the virtual screen space.
* **Canvas Preview:** Render an interactive canvas representing the physical monitor setup accurately.

### 4.2. Visual Zone Canvas (The Herd Editor)
* **Drag-and-Drop Drawing:** Draw rectangular zones per screen using mouse input.
* **Relative Percentage Coordinates:** Store zone position and bounds in relative float percentages ($0.0$ to $1.0$). If resolution or monitor scale changes, zones rescale dynamically.
* **Zone Properties:** Each zone contains:
  * **Zone Name:** Friendly label (e.g., *"Terminal Top-Right"*).
  * **Command / Executable:** Executable path or inline command (e.g., `alacritty -e herdr` or `zen-browser`).
  * **Launch Delay (ms):** Optional delay buffer to allow heavy applications time to spawn before applying movement vectors.

### 4.3. View Management (Workspace Profiles)
* **Named Views:** Save combinations of zones as a distinct "View" (e.g., *"Dev Setup"*, *"Writing Mode"*).
* **Configuration File:** Persist all views, zones, monitor rules, and hotkey bindings in a clean, human-readable JSON file stored at `~/.config/herdctrl/layouts.json`.

### 4.4. Triggering Mechanisms
* **Dashboard Launcher:** Visual buttons inside the HerdCtrl GUI to launch any View instantly.
* **Global Hotkeys:** Assign custom key combinations (e.g., `Super + Shift + C`) to trigger specific views globally, even when HerdCtrl is running in the background.

### 4.5. The Launch & Move Engine (Linux/Ubuntu X11 Execution Pipeline)
Upon triggering a View:
1. **Geometry Calculation:** Query active screen geometries; calculate target pixel dimensions $(X, Y, W, H)$ from the saved relative percentage bounds.
2. **Process Spawning:** Launch each zone's defined shell command asynchronously via `std::process::Command`, recording the target Process ID (PID).
3. **Window Polling:** Execute an asynchronous polling loop to wait until a valid window handle associated with the process appears in the window manager window stack.
4. **Window Manipulation:** Apply position and resize attributes to the window handle using native system calls / `wmctrl` (`wmctrl -r <WINDOW> -e 0,x,y,w,h`).

---

## 5. System Architecture & File Structure

```text
herdctrl/
├── Cargo.toml
├── assets/
│   └── icon.png
└── src/
    ├── main.rs           # Application entrypoint & event loop
    ├── ui/
    │   ├── mod.rs        # UI module routing
    │   ├── canvas.rs     # Interactive monitor canvas & zone drawing logic
    │   └── dashboard.rs  # Main View manager & configuration panel
    ├── core/
    │   ├── mod.rs
    │   ├── config.rs     # JSON load/save mechanisms
    │   ├── monitor.rs    # Monitor discovery & coordinate translation
    │   └── launcher.rs   # Async process execution & handle polling
    └── os/
        ├── mod.rs        # Platform abstraction layer
        └── linux.rs      # Ubuntu / X11 window positioning & wmctrl integration
```

---

## 6. Non-Functional Requirements

* **Binary Size:** $< 15 \text{ MB}$ compiled binary.
* **Startup Time:** $< 100 \text{ ms}$ warm start.
* **Memory Footprint:** $< 30 \text{ MB}$ RAM footprint when active.
* **Reliability:** Timeout mechanisms on window handle polling (default $5 \text{ seconds}$) to prevent non-blocking UI freezes if an app fails to start.

---

## 7. Future Scope (v1.x+)

* macOS support via Accessibility API (`AXUIElement`).
* Windows support via Win32 API (`SetWindowPos`).
* Native Wayland Compositor IPC integrations (e.g., Hyprland / KDE Plasma IPC).
* Tray Icon / Minimization to system notification area.