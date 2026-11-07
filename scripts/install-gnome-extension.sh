#!/usr/bin/env bash
set -euo pipefail

project_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
extension_uuid=herdctrl@herdctrl.local
for tool in python3 gnome-extensions gnome-shell; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Missing required tool: $tool" >&2
        exit 1
    fi
done
shell_version=$(gnome-shell --version)
if [[ ! $shell_version =~ GNOME\ Shell\ 50([.[:space:]]|$) ]]; then
    echo "This extension targets GNOME Shell 50. Found: $shell_version" >&2
    exit 1
fi
bundle=$(python3 "$project_dir/scripts/package-gnome-extension.py")
gnome-extensions install --force "$bundle"
echo "Installed HerdCtrl for your user."
if gnome-extensions enable "$extension_uuid"; then
    echo "Enable requested. If replacing a loaded version, log out and back in to load the update."
else
    echo "GNOME has not loaded the new extension yet. Log out and back in, then run:"
    echo "  gnome-extensions enable $extension_uuid"
fi
echo "Then start HerdCtrl and select Refresh displays."
