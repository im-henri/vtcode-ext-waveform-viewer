#!/usr/bin/env python3
"""Build a standalone package; optionally install it in a VTCode workspace."""
import argparse
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def run(*args):
    subprocess.run(args, cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--skip-tests", action="store_true")
    parser.add_argument("--install-local", type=Path, metavar="WORKSPACE")
    parser.add_argument("--host-fixtures", type=Path, metavar="VTCODE_SOURCE")
    args = parser.parse_args()
    common = ["--locked"] + (["--offline"] if args.offline else [])
    run("cargo", "fmt", "--check")
    if not args.skip_tests:
        run("cargo", "test", *common)
    run("cargo", "build", *common, "--release", "--target", "wasm32-unknown-unknown", "--lib")
    wasm = ROOT / "waveform-viewer.wasm"
    shutil.copy2(ROOT / "target/wasm32-unknown-unknown/release/vtcode_waveform_viewer.wasm", wasm)
    if args.host_fixtures:
        host = args.host_fixtures.resolve()
        if not (host / "src/extension_waveform_tests.rs").is_file():
            parser.error("--host-fixtures must be the VTCode source containing waveform integration tests")
        target = host / "tests/fixtures/waveform-viewer"
        target.mkdir(parents=True, exist_ok=True)
        for name in ["waveform-viewer.wasm", "extension.json"]:
            shutil.copy2(ROOT / name, target / name)
        shutil.copy2(ROOT / "examples/demo.vcd", target / "demo.vcd")
        shutil.copy2(ROOT / "tests/fixtures/gcd.fst", target / "gcd.fst")
        shutil.copy2(ROOT / "tests/fixtures/NOTICE.md", target / "NOTICE.md")
        shutil.copy2(ROOT / "third-party/fst-reader-0.17.0/LICENSE", target / "FST-LICENSE")
        shutil.copy2(ROOT / "THIRD_PARTY_NOTICES.md", target / "THIRD_PARTY_NOTICES.md")
        shutil.copytree(ROOT / "third-party", target / "third-party", dirs_exist_ok=True)
    if args.install_local:
        workspace = args.install_local.resolve()
        if not workspace.is_dir():
            parser.error("--install-local must name an existing workspace directory")
        target = workspace / ".vtcode/extensions/waveform-viewer"
        target.mkdir(parents=True, exist_ok=True)
        for name in ["waveform-viewer.wasm", "extension.json", "README.md", "LICENSE", "THIRD_PARTY_NOTICES.md"]:
            shutil.copy2(ROOT / name, target / name)
        shutil.copytree(ROOT / "third-party", target / "third-party", dirs_exist_ok=True)
        if (ROOT / "docs").is_dir():
            shutil.copytree(ROOT / "docs", target / "docs", dirs_exist_ok=True)
        print(f"Installed {target}. Run Extensions: Reload in VTCode.")
    print(f"Built {wasm} ({wasm.stat().st_size:,} bytes)")


if __name__ == "__main__":
    main()
