#!/usr/bin/env python3
"""Build the self-contained GNOME extension bundle without GNOME/Node tooling."""
import argparse
import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "gnome-extension" / "herdctrl@herdctrl.local"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "target" / "herdctrl@herdctrl.local.shell-extension.zip")
    args = parser.parse_args()
    metadata = json.loads((SOURCE / "metadata.json").read_text())
    if metadata["uuid"] != SOURCE.name:
        parser.error("Extension directory and metadata UUID differ")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(args.output, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        for name in ("metadata.json", "extension.js", "controller.mjs"):
            entry = zipfile.ZipInfo(name, date_time=(2026, 10, 31, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            bundle.writestr(entry, (SOURCE / name).read_bytes())
    print(args.output)


if __name__ == "__main__":
    main()
