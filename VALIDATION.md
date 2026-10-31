# PRD acceptance record

`PRD.md` is the unchanged product specification. This record distinguishes implemented functionality from verified acceptance.

| PRD requirement | Implementation / verification |
| --- | --- |
| 4.1 Multi-monitor geometry | XRandR discovery and canvas with offsets; parser tests cover negative coordinates and active outputs. Horizontal transform factor shown; full scaling/rotation validation remains open. |
| 4.2 Visual zone editor | Drawing, moving, corner resizing, command/name/delay properties, and normalized bounds implemented. Interactive desktop verification remains open. |
| 4.3 Profiles and persistence | Named views, JSON serialization, validation, private atomic saves, and malformed-config rejection. Persistence tests cover round-trip and error handling. |
| 4.4 Dashboard and global shortcuts | View launch button and global hotkey registration/event handling implemented. Conflicts appear in Activity. Desktop shortcut verification remains open. |
| 4.5 Launch and move engine | Background workers, shell commands, process/descendant matching, bounded polling and wmctrl movement. Real-window launch/resize acceptance remains open. |
| 5 Structure | Separate core, UI and Linux OS modules. Application icon is not yet provided. |
| 6 Binary <15 MB | 4,481,312 bytes (4.48 MB), optimized and stripped x86_64 Linux build; meets size target. |
| 6 Warm startup <100 ms | Not measured; no acceptance claim. |
| 6 Active RSS <30 MB | Not measured; no acceptance claim. |
| 6 Reliability | UI does not perform helper-process polling. Five-second polling budget after launch delay; helper-process timeouts can extend elapsed completion time. |

## Desktop acceptance procedure

On Ubuntu X11 with `wmctrl`, `xrandr`, and two connected displays:

1. Draw, drag and resize zones on both displays, including one with a negative virtual offset; save, restart and check preservation.
2. Launch `xterm` into each zone and inspect geometry with `wmctrl -lG`. Check resizing after a resolution change.
3. Assign a free global hotkey, save, focus another app, then trigger the view. Confirm conflicting shortcuts are reported.
4. Launch a command that creates no window and verify a timeout message while the editor remains responsive.
5. Disconnect a target display and confirm an error without moving an unrelated window.
6. Check malformed JSON, interrupted saves, and unsaved-close behavior.
7. Measure warm startup to first visible frame and active RSS; record desktop, GPU, monitor scales, executable size and measurements before claiming PRD performance acceptance.

## Local verification — 2026-10-31

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --locked -- -D warnings`: passed.
- `cargo test --locked`: 7 passed, 0 failed.
- `cargo build --release --locked`: passed with Rust 1.99.1.
- GUI startup smoke: remained running for eight seconds on XWayland, no stderr output; terminated by test timeout. Temporary copies of `libxkbcommon-x11-0` and `libxcb-xkb1` were needed on this host. This checks process startup, not visual or interaction acceptance.
- Host session is Wayland. Full X11 window herding and hotkey integration remain unverified.
- Rust toolchain used for local verification is in `/tmp/herdctrl-cargo` and `/tmp/herdctrl-rustup`; it was not installed system-wide.
