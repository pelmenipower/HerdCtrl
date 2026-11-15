#!/usr/bin/env python3
"""Opt-in GUI/hotkey check and measurements; uses an isolated configuration.

Run on Ubuntu X11, or inside Xvfb with an EWMH window manager for CI.
Window visibility is a startup proxy, not first-rendered-frame acceptance.
"""
import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
import time


def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT, timeout=3)


def wait_for(predicate, description, timeout=8):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.01)
    raise RuntimeError(f"Timed out waiting for {description}")


def visible_window(process):
    if process.poll() is not None:
        raise RuntimeError(f"Process exited with {process.returncode}")
    result = subprocess.run(
        ["xdotool", "search", "--onlyvisible", "--pid", str(process.pid)],
        capture_output=True, text=True, timeout=3,
    )
    if result.returncode not in (0, 1):
        raise RuntimeError(result.stderr)
    return result.stdout.splitlines()[0] if result.stdout.strip() else None


def stop(process):
    if process and process.poll() is None:
        # The editor starts a tray child for the temporary configuration; stop it too.
        trays = subprocess.run(["pgrep", "-P", str(process.pid), "-f", "--", "--tray"],
                               capture_output=True, text=True).stdout.split()
        for pid in trays:
            subprocess.run(["kill", pid], check=False)
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=3)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/herdctrl"))
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output", type=Path, help="Save JSON measurements to this path")
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    if os.environ.get("XDG_SESSION_TYPE") != "x11" or not os.environ.get("DISPLAY"):
        parser.error("An accessible X11 session is required (XDG_SESSION_TYPE=x11, DISPLAY set)")
    for tool in ("xdotool", "wmctrl", "xrandr", "xterm"):
        if not shutil.which(tool):
            parser.error(f"Missing tool: {tool}")
    binary = args.binary.resolve(strict=True)
    displays = run("xrandr", "--verbose")
    monitor = next(
        (line.split()[0] for line in displays.splitlines()
         if " connected " in line and any(
             "x" in field and ("+" in field or "-" in field)
             for field in line.split()[2:])), None,
    )
    if monitor is None:
        parser.error("No active display found")
    wm = run("wmctrl", "-m")
    samples = []
    with tempfile.TemporaryDirectory(prefix="herdctrl-x11-") as directory:
        root = Path(directory)
        marker = root / "hotkey-triggered"
        config = root / "herdctrl" / "layouts.json"
        config.parent.mkdir()
        config.write_text(json.dumps({"views": [{
            "name": "X11 acceptance", "hotkey": "Control+Alt+Shift+F10",
            "zones": [{"name": "Hotkey marker", "monitor": monitor,
                       "command": "touch " + shlex.quote(str(marker)), "delay_ms": 0,
                       "bounds": {"x": 0, "y": 0, "width": 0.5, "height": 0.5}}],
        }]}))
        env = dict(os.environ, XDG_CONFIG_HOME=directory)
        helper = app = None
        try:
            helper = subprocess.Popen(["xterm", "-T", "HerdCtrl acceptance focus", "-e", "sleep", "120"])
            helper_window = wait_for(lambda: visible_window(helper), "focus window")
            # Discard one warm-up, then sample already cached executable startup.
            for index in range(args.runs + 1):
                with (root / "stderr.log").open("w+") as log:
                    started = time.monotonic()
                    app = subprocess.Popen([str(binary)], env=env, stderr=log, stdout=log)
                    try:
                        wait_for(lambda: visible_window(app), "visible HerdCtrl window")
                        elapsed = (time.monotonic() - started) * 1000
                        time.sleep(1)
                        status = Path(f"/proc/{app.pid}/status").read_text()
                        rss = int(next(line.split()[1] for line in status.splitlines() if line.startswith("VmRSS:")))
                        if index:
                            samples.append({"visible_window_ms": round(elapsed, 2), "rss_kib": rss})
                        if index == args.runs:
                            # Trigger globally with another application focused.
                            run("xdotool", "windowactivate", "--sync", helper_window)
                            run("xdotool", "key", "--clearmodifiers", "ctrl+alt+shift+F10")
                            wait_for(marker.exists, "global hotkey command")
                            time.sleep(6)
                            assert app.poll() is None, "GUI exited during no-window timeout"
                    except Exception:
                        log.flush()
                        log.seek(0)
                        print(log.read(), file=sys.stderr)
                        raise
                    finally:
                        stop(app)
        finally:
            stop(app)
            stop(helper)
    report = {
        "binary": str(binary), "binary_bytes": binary.stat().st_size,
        "session": os.environ["XDG_SESSION_TYPE"], "window_manager": wm.strip(),
        "display_geometry": displays, "samples": samples,
        "median_visible_window_ms": statistics.median(s["visible_window_ms"] for s in samples),
        "max_rss_kib": max(s["rss_kib"] for s in samples),
        "global_hotkey_with_other_app_focused": "passed",
        "gui_survived_no_window_command": "passed",
        "limitations": "Visibility is not first rendered frame. RSS sampled after 1 s; not peak RSS. Virtual displays do not verify physical multi-monitor/scaling behavior.",
    }
    rendered = json.dumps(report, indent=2)
    if args.output:
        args.output.write_text(rendered + "\n")
    print(rendered)


if __name__ == "__main__":
    main()
