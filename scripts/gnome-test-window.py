#!/usr/bin/env python3
"""Finite-lived GTK window for the opt-in live GNOME launch-engine test."""
import argparse
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--backend", choices=("wayland", "x11"), required=True)
parser.add_argument("--report", type=Path, required=True)
args = parser.parse_args()
os.environ["GDK_BACKEND"] = args.backend

import gi  # noqa: E402
gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk  # noqa: E402

window = Gtk.Window(title="HerdCtrl temporary acceptance window")
window.set_decorated(False)
window.set_default_size(320, 200)
window.set_child(Gtk.Label(label="HerdCtrl placement test — closes automatically"))
loop = GLib.MainLoop()
window.connect("close-request", lambda *_: loop.quit())


def sample():
    if args.report.with_suffix(".stop").exists():
        loop.quit()
        return GLib.SOURCE_REMOVE
    surface = window.get_surface()
    if surface is None:
        return GLib.SOURCE_CONTINUE
    monitor = surface.get_display().get_monitor_at_surface(surface)
    report = {"pid": os.getpid(), "backend": args.backend,
              "width": window.get_width(), "height": window.get_height(),
              "monitor": monitor.get_connector() if monitor else None}
    if args.backend == "x11":
        gi.require_version("GdkX11", "4.0")
        from gi.repository import GdkX11
        xid = GdkX11.X11Surface.get_xid(surface)
        report["xwininfo"] = subprocess.check_output(
            ["xwininfo", "-id", str(xid)], text=True, timeout=2)
    temporary = args.report.with_suffix(".tmp")
    temporary.write_text(json.dumps(report))
    temporary.replace(args.report)
    return GLib.SOURCE_CONTINUE


window.present()
GLib.timeout_add(100, sample)
GLib.timeout_add_seconds(20, lambda: (loop.quit(), GLib.SOURCE_REMOVE)[1])
loop.run()
window.destroy()
