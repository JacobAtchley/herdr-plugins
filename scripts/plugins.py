#!/usr/bin/env python3
"""Build or link the herdr plugins under plugins/.

Each plugin is built by running the [[build]] commands from its own
herdr-plugin.toml, so local builds match what `herdr plugin install` runs and
plugins in any language work without changes here.

Usage: plugins.py build|link [plugin-dir-name ...]
"""

import os
import pathlib
import platform
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
PLATFORM = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}[platform.system()]


def manifests(names):
    if names:
        paths = [ROOT / "plugins" / name / "herdr-plugin.toml" for name in names]
        missing = [str(p) for p in paths if not p.is_file()]
        if missing:
            sys.exit(f"no manifest: {', '.join(missing)}")
        return paths
    return sorted((ROOT / "plugins").glob("*/herdr-plugin.toml"))


def supported(entry, default_platforms):
    platforms = entry.get("platforms", default_platforms)
    return platforms is None or PLATFORM in platforms


def build(manifest):
    data = tomllib.loads(manifest.read_text())
    for step in data.get("build", []):
        if not supported(step, data.get("platforms")):
            continue
        print(f"[{data['id']}] {' '.join(step['command'])}", flush=True)
        subprocess.run(step["command"], cwd=manifest.parent, check=True)


def link(manifest):
    data = tomllib.loads(manifest.read_text())
    herdr = os.environ.get("HERDR_BIN_PATH", "herdr")
    subprocess.run(
        [herdr, "plugin", "link", str(manifest.parent)],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    print(f"[{data['id']}] linked {manifest.parent.relative_to(ROOT)}", flush=True)


def main():
    if len(sys.argv) < 2 or sys.argv[1] not in ("build", "link"):
        sys.exit(__doc__)
    action = build if sys.argv[1] == "build" else link
    found = manifests(sys.argv[2:])
    if not found:
        sys.exit("no plugins found under plugins/")
    for manifest in found:
        try:
            action(manifest)
        except subprocess.CalledProcessError as err:
            sys.exit(f"{manifest.parent.name}: {' '.join(err.cmd)} exited with {err.returncode}")


if __name__ == "__main__":
    main()
