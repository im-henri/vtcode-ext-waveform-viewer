#!/usr/bin/env python3
"""Regenerate documentation PNGs from actual native viewer frames (requires Pillow)."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    subprocess.run(
        ["cargo", "build", "--locked", *(["--offline"] if args.offline else []), "--example", "preview"],
        cwd=ROOT, check=True,
    )
    executable = ROOT / "target/debug/examples" / ("preview.exe" if os.name == "nt" else "preview")
    common = {"WIDTH": "180", "HEIGHT": "28", "START": "200", "SPAN": "600"}
    cases = [
        ("preview", "demo.vcd", {}),
        ("drag-preview", "demo.vcd", {"DRAG_SIGNAL": "wdata", "DRAG_Y": "8"}),
        ("hover", "bus_transfers.vcd", {"HOVER_X": "120", "HOVER_Y": "14"}),
        ("normal", "bus_transfers.vcd", {"SIZE": "normal", "HEIGHT": "28"}),
        ("range-preview", "bus_transfers.vcd", {"RANGE_START": "360", "RANGE_END": "620"}),
        ("signal-details", "demo.vcd", {"HOVER_X": "44", "HOVER_Y": "10"}),
        ("help", "demo.vcd", {"HELP": "1", "WIDTH": "100"}),
        ("help-narrow", "demo.vcd", {"HELP": "1", "WIDTH": "80"}),
        ("signal-menu", "demo.vcd", {"MENU_SIGNAL": "wdata", "WIDTH": "150", "HEIGHT": "32"}),
        ("selection", "demo.vcd", {"SELECT": "4,5,6", "HEIGHT": "32"}),
        ("selection-menu", "demo.vcd", {"SELECT": "4,5,6", "MENU_SIGNAL": "wdata", "HEIGHT": "38"}),
        ("column-menu", "demo.vcd", {"COLUMN_MENU": "1", "VALUE_WIDTH": "22", "HEIGHT": "32"}),
        ("cursor-handles", "demo.vcd", {"CURSOR": "420", "CURSOR_B": "420", "HOVER_X": "110", "HOVER_Y": "3"}),
        ("menu-narrow", "demo.vcd", {"MENU_SIGNAL": "address", "WIDTH": "90", "HEIGHT": "18"}),
        ("marker-dialog", "demo.vcd", {"QUERY": "marker", "QUERY_TEXT": "Handshake"}),
        ("query-validation", "demo.vcd", {"QUERY": "group", "QUERY_SUBMIT": "1"}),
        ("drag-invalid", "demo.vcd", {"DRAG_SIGNAL": "wdata", "DRAG_X": "8", "DRAG_Y": "3"}),
    ]
    (ROOT / "screenshots").mkdir(exist_ok=True)
    for name, source, options in cases:
        env = {key: value for key, value in os.environ.items() if not key.startswith("WAVE_")}
        env.update({f"WAVE_{key}": value for key, value in (common | options).items()})
        output = subprocess.run(
            [str(executable), f"examples/{source}", "--json"],
            cwd=ROOT, env=env, check=True, capture_output=True, encoding="utf-8",
        ).stdout
        frame = json.loads(output)
        assert (frame["width"], frame["height"]) == (int(env["WAVE_WIDTH"]), int(env["WAVE_HEIGHT"]))
        target = ROOT / f"screenshots/{name}.json"
        target.write_text(output, encoding="utf-8")
        subprocess.run(
            [sys.executable, "examples/render_preview.py", str(target), f"docs/{name}.png"],
            cwd=ROOT, check=True,
        )


if __name__ == "__main__":
    main()
