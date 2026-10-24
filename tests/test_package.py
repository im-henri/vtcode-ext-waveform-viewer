import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("release_package", Path(__file__).resolve().parents[1] / "package.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class PackageTests(unittest.TestCase):
    def fixture(self, root):
        (root / "extension.json").write_text(json.dumps({"id": "example", "version": "1.2.3", "main": "main.wasm"}), encoding="utf-8")
        (root / "main.wasm").write_bytes(b"\0asm fixture")
        (root / "LICENSE").write_text("license", encoding="utf-8")
        (root / "package-files.json").write_text(json.dumps(["extension.json", "main.wasm", "LICENSE"]), encoding="utf-8")

    def test_runtime_only_and_reproducible(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            (root / "development.rs").write_text("not needed at runtime", encoding="utf-8")
            first = package.build_package(root, root / "a.zip", "v1.2.3")
            second = package.build_package(root, root / "b.zip", "v1.2.3")
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with zipfile.ZipFile(first) as archive:
                self.assertEqual(archive.namelist(), ["extension.json", "main.wasm", "LICENSE"])
            with self.assertRaises(ValueError):
                package.build_package(root, root / "a.zip", "v9.0.0")

    def test_missing_unsafe_duplicate_and_source_overwrite_paths_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            for names in [["../escape"], ["main.wasm", "MAIN.WASM"], ["extension.json", "LICENSE"], [".git/config"]]:
                (root / "package-files.json").write_text(json.dumps(names), encoding="utf-8")
                with self.assertRaises(ValueError):
                    package.build_package(root, root / "a.zip", "v1.2.3")
            self.fixture(root)
            with self.assertRaises(ValueError):
                package.build_package(root, root / "main.wasm", "v1.2.3")


if __name__ == "__main__":
    unittest.main()
