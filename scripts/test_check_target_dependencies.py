#!/usr/bin/env python3
"""Repository-specific tests for the PipeWire target guard."""

import copy, importlib.util, subprocess, sys, tempfile, tomllib, unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT, MANIFEST, LINUX = ROOT / "scripts" / "check-target-dependencies.py", ROOT / "src-tauri" / "Cargo.toml", 'cfg(target_os = "linux")'
sys.dont_write_bytecode = True


def load_checker():
    spec = importlib.util.spec_from_file_location("target_guard", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def metadata(*, direct: bool, alias: str = "pipewire") -> dict:
    root, pipewire = "root", "pipewire-package"
    package = lambda identity, name, source: {
        "id": identity, "name": name, "version": "0.10.0", "source": source,
        "manifest_path": str(MANIFEST if name == "pipe-deck" else "/crate/Cargo.toml")}
    edges = [{"name": alias, "pkg": pipewire}] if direct else []
    return {
        "packages": [package(root, "pipe-deck", None), package(pipewire, "pipewire", "registry")],
        "resolve": {"root": root, "nodes": [{"id": root, "deps": edges}, {"id": pipewire, "deps": []}]},
    }


class GuardTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.guard = load_checker()
        with MANIFEST.open("rb") as source:
            cls.manifest = tomllib.load(source)

    def manifest_check(self):
        check = getattr(self.guard, "check_manifest", None)
        self.assertTrue(callable(check), "repository-specific check_manifest is required")
        return check

    def target_check(self):
        check = getattr(self.guard, "check_target", None)
        self.assertTrue(callable(check), "repository-specific check_target is required")
        return check

    def test_checked_in_manifest_is_linux_only(self) -> None:
        self.manifest_check()(self.manifest)

    def test_widened_target_is_rejected(self) -> None:
        manifest = copy.deepcopy(self.manifest)
        table = manifest["target"].pop(LINUX)
        manifest["target"]['cfg(any(target_os = "linux", target_os = "freebsd"))'] = table
        check = self.manifest_check()
        with self.assertRaises(AssertionError):
            check(manifest)

    def test_root_placement_is_rejected(self) -> None:
        manifest = copy.deepcopy(self.manifest)
        dependency = manifest["target"][LINUX]["dependencies"].pop("pipewire")
        manifest["dependencies"]["pipewire"] = dependency
        check = self.manifest_check()
        with self.assertRaises(AssertionError):
            check(manifest)

    def test_unrelated_git_and_path_same_name_are_ignored(self) -> None:
        check = self.manifest_check()
        for source, value in (("git", "https://example.invalid/pw"), ("path", "../pw")):
            with self.subTest(source=source):
                manifest = copy.deepcopy(self.manifest)
                manifest.setdefault("dev-dependencies", {})["forked-pw"] = {
                    "package": "pipewire", source: value,
                }
                check(manifest)

    def test_alias_edge_follows_resolved_package_identity(self) -> None:
        edge, package = self.target_check()(metadata(direct=True, alias="pw"), "fixture-target", True)
        self.assertEqual((edge["name"], package["name"]), ("pw", "pipewire"))

    def test_windows_excludes_direct_but_ignores_transitive_pipewire(self) -> None:
        check = self.target_check()
        with self.assertRaises(AssertionError):
            check(metadata(direct=True), "fixture-target", False)
        self.assertIsNone(check(metadata(direct=False), "fixture-target", False))

    def test_make_target_is_cwd_independent_and_ordered(self) -> None:
        expected = [f"python3 {ROOT}/scripts/test_check_target_dependencies.py", f"python3 {SCRIPT}"]
        with tempfile.TemporaryDirectory(prefix="pipe-deck-make-") as other_cwd:
            for cwd in (ROOT, Path(other_cwd)):
                result = subprocess.run(
                    ["make", "--no-print-directory", "-n", "-f", str(ROOT / "Makefile"), "check-target-dependencies"],
                    cwd=cwd, check=True, capture_output=True, text=True,
                )
                self.assertEqual(result.stdout.splitlines(), expected)


if __name__ == "__main__":
    unittest.main(verbosity=2)
