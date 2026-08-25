#!/usr/bin/env python3
"""Repository-specific tests for the PipeWire target guard."""

import copy, importlib.util, subprocess, sys, tempfile, tomllib, unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT, MANIFEST, LINUX = ROOT / "scripts" / "check-target-dependencies.py", ROOT / "src-tauri" / "Cargo.toml", 'cfg(target_os = "linux")'
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
sys.dont_write_bytecode = True


def load_checker():
    spec = importlib.util.spec_from_file_location("target_guard", SCRIPT)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def metadata(*, direct: bool, alias: str = "pipewire", version: str = "0.10.0",
             source: str | None = CRATES_IO, unrelated_source: str | None | bool = False,
             transitive: bool = False) -> dict:
    root, pipewire, helper = "root", "pipewire-package", "helper-package"
    package = lambda identity, name, package_version, package_source: {
        "id": identity, "name": name, "version": package_version, "source": package_source,
        "manifest_path": str(MANIFEST if name == "pipe-deck" else "/crate/Cargo.toml")}
    edges = [{"name": alias, "pkg": pipewire}] if direct else []
    packages = [package(root, "pipe-deck", "0.2.1", None),
                package(pipewire, "pipewire", version, source)]
    nodes = [{"id": pipewire, "deps": []}]
    if transitive:
        edges.append({"name": "helper", "pkg": helper})
        packages.append(package(helper, "helper", "1.0.0", CRATES_IO))
        nodes.append({"id": helper, "deps": [{"name": "pipewire", "pkg": pipewire}]})
    if unrelated_source is not False:
        unrelated = "unrelated-pipewire"
        edges.append({"name": "forked-pw", "pkg": unrelated})
        packages.append(package(unrelated, "pipewire", "0.10.0", unrelated_source))
        nodes.append({"id": unrelated, "deps": []})
    return {
        "packages": packages,
        "resolve": {"root": root, "nodes": [{"id": root, "deps": edges}, *nodes]},
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

    def test_wrong_source_does_not_satisfy_linux(self) -> None:
        with self.assertRaises(AssertionError):
            self.target_check()(
                metadata(direct=True, source="git+https://example.invalid/pipewire"),
                "fixture-linux", True,
            )

    def test_wrong_version_does_not_satisfy_linux(self) -> None:
        with self.assertRaises(AssertionError):
            self.target_check()(metadata(direct=True, version="1.0.0"), "fixture-linux", True)

    def test_unrelated_direct_git_and_path_resolutions_are_ignored(self) -> None:
        check = self.target_check()
        for source in ("git+https://example.invalid/pipewire", None):
            with self.subTest(source=source):
                edge, package = check(metadata(direct=True, unrelated_source=source), "fixture-linux", True)
                self.assertEqual((edge["pkg"], package["source"]), ("pipewire-package", CRATES_IO))

    def test_linux_requires_one_intended_dependency(self) -> None:
        with self.assertRaises(AssertionError):
            self.target_check()(
                metadata(direct=False, unrelated_source="git+https://example.invalid/pipewire"),
                "fixture-linux", True,
            )

    def test_windows_excludes_direct_but_ignores_real_transitive_pipewire(self) -> None:
        check = self.target_check()
        with self.assertRaises(AssertionError):
            check(metadata(direct=True), "fixture-target", False)
        self.assertIsNone(check(metadata(direct=False, transitive=True), "fixture-target", False))

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
