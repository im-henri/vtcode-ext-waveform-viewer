#!/usr/bin/env python3
"""Collect locked dependency licenses for redistribution of the Wasm package."""
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--format-version", "1", "--locked", "--offline",
     "--filter-platform", "wasm32-unknown-unknown"], cwd=ROOT, text=True))
lines = ["# Third-party notices", "",
         "The viewer is MIT licensed. Dependencies retain their respective licenses.",
         "This inventory includes development dependencies; see Cargo.lock for exact versions.", "",
         "The vendored VTCode extension SDK is covered by vendor/vtcode-extension-api/LICENSE.", "",
         "| Crate | Version | License | License files |", "| --- | --- | --- | --- |"]
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if not package.get("source"):
        continue
    name = f'{package["name"]}-{package["version"]}'
    source = Path(package["manifest_path"]).parent
    destination = ROOT / "third-party" / name
    files = [p for p in source.iterdir() if p.is_file() and p.name.upper().startswith(("LICENSE", "COPYING", "COPYRIGHT"))]
    if not files:
        raise RuntimeError(f"No license file found for {name}")
    destination.mkdir(parents=True, exist_ok=True)
    for path in files:
        shutil.copy2(path, destination / path.name)
    links = ", ".join(f'[{p.name}](third-party/{name}/{p.name})' for p in files)
    lines.append(f'| {package["name"]} | {package["version"]} | {package.get("license", "See files")} | {links} |')
lines += ["", "The GCD FST test fixture originates from ekiwi/fst-reader (BSD 3-Clause).",
          "See tests/fixtures/NOTICE.md and third-party/fst-reader-0.17.0/LICENSE.", "",
          "VaporView, Vivado, and Verdi informed the interaction design. No code or assets",
          "from those viewers are included.", ""]
(ROOT / "THIRD_PARTY_NOTICES.md").write_text("\n".join(lines), encoding="utf-8")
print("Updated THIRD_PARTY_NOTICES.md and third-party licenses")
