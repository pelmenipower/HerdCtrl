#!/usr/bin/env python3
"""Read-only connectivity/geometry diagnostic for the live GNOME Shell bridge."""
import ast
import json
import subprocess
import sys

request = json.dumps({"version": 1, "operation": "state"})
try:
    raw = subprocess.check_output([
        "gdbus", "call", "--session", "--timeout", "2", "--dest", "org.gnome.Shell",
        "--object-path", "/org/gnome/Shell/Extensions/HerdCtrl", "--method",
        "org.gnome.Shell.Extensions.HerdCtrl.Call", request,
    ], text=True, stderr=subprocess.STDOUT, timeout=4)
    response = json.loads(ast.literal_eval(raw)[0])
    if not response["ok"]:
        raise RuntimeError(response.get("error", "Bridge error"))
    state = response["result"]
    if not state["monitors"]:
        raise RuntimeError("No logical monitors returned")
    print(json.dumps({"bridge": "connected", "monitors": state["monitors"],
                      "eligible_window_count": len(state["windows"])}, indent=2))
except (OSError, subprocess.SubprocessError, ValueError, KeyError, IndexError, RuntimeError) as error:
    detail = getattr(error, "output", None) or str(error)
    print(f"GNOME bridge unavailable: {detail}\n"
          "Run bash scripts/install-gnome-extension.sh. Log out/in if GNOME has not loaded it,\n"
          "then run gnome-extensions enable herdctrl@herdctrl.local.", file=sys.stderr)
    sys.exit(1)
