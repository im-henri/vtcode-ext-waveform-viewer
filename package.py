#!/usr/bin/env python3
"""Create a reproducible runtime-only vtcode-extension.zip for a version tag."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import stat
import tomllib
import zipfile


def runtime_files(root):
    root = Path(root).resolve()
    manifest = json.loads((root / "extension.json").read_text(encoding="utf-8"))
    if (root / "Cargo.toml").exists():
        cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["package"]
        locked = [item for item in tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"] if item["name"] == cargo["name"]]
        if cargo["version"] != manifest["version"] or len(locked) != 1 or locked[0]["version"] != manifest["version"]:
            raise ValueError("extension.json, Cargo.toml, and Cargo.lock versions must match")
    names = json.loads((root / "package-files.json").read_text(encoding="utf-8"))
    if not isinstance(names, list) or not names or len(names) > 4096:
        raise ValueError("package-files.json must list the runtime files explicitly")
    seen = set()
    for name in names:
        path = PurePosixPath(name) if isinstance(name, str) else None
        if (not path or path.is_absolute() or ".." in path.parts or str(path) != name
                or any(ch in name for ch in "\\:\0") or any(ord(ch) < 32 for ch in name)
                or any(part.casefold() in (".git", ".vtcode-source.json") for part in path.parts)):
            raise ValueError(f"Invalid runtime package path: {name!r}")
        if name.casefold() in seen:
            raise ValueError(f"Duplicate package path: {name}")
        seen.add(name.casefold())
        source = root / name
        if not source.resolve().is_relative_to(root) or any((root.joinpath(*path.parts[:i])).is_symlink() for i in range(1, len(path.parts) + 1)):
            raise ValueError(f"Package files must not be symlinks: {name}")
        if not source.is_file():
            raise ValueError(f"Missing runtime file: {name}")
    required = {"extension.json", "LICENSE"}
    if manifest.get("main"):
        required.add(manifest["main"])
    if not required.issubset(names):
        raise ValueError(f"Missing required package entries: {sorted(required - set(names))}")
    if sum((root / name).stat().st_size for name in names) > 256 * 1024 * 1024:
        raise ValueError("Unpacked extension package exceeds 256 MiB")
    return manifest, names


def build_package(root, output, tag=None):
    root = Path(root).resolve()
    manifest, names = runtime_files(root)
    if tag is not None and tag.removeprefix("v") != manifest["version"]:
        raise ValueError(f"Tag {tag} does not match extension version {manifest['version']}")
    output = Path(output).resolve()
    if output.is_relative_to(root) and output.relative_to(root).as_posix() in names:
        raise ValueError("The output archive must not overwrite a runtime input")
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = output.with_name(output.name + ".tmp")
    try:
        with zipfile.ZipFile(temporary, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for name in names:
                source = root / name
                info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                info.create_system = 3
                info.compress_type = zipfile.ZIP_DEFLATED
                mode = 0o755 if source.stat().st_mode & 0o111 else 0o644
                info.external_attr = (stat.S_IFREG | mode) << 16
                with source.open("rb") as stream, archive.open(info, "w") as target:
                    shutil.copyfileobj(stream, target, length=64 * 1024)
        temporary.replace(output)
    finally:
        temporary.unlink(missing_ok=True)
    with output.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    output.with_name(output.name + ".sha256").write_text(f"{digest}  {output.name}\n", encoding="ascii", newline="\n")
    print(f"{manifest['id']} {manifest['version']}: {len(names)} runtime files, {output.stat().st_size:,} ZIP bytes\n{output}\nSHA-256 {digest}")
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("dist/vtcode-extension.zip"))
    parser.add_argument("--tag", help="Require the package version to match this Git tag")
    args = parser.parse_args()
    try:
        build_package(Path(__file__).resolve().parent, args.output, args.tag)
    except (ValueError, OSError, KeyError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
