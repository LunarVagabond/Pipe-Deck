#!/usr/bin/env python3
"""Deterministic regression tests for check-target-dependencies.py."""

from __future__ import annotations

import builtins
from contextlib import redirect_stdout
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest import mock


SCRIPT_PATH = Path(__file__).with_name("check-target-dependencies.py")
REPOSITORY_ROOT = SCRIPT_PATH.parent.parent
ROOT_ID = "path+file:///fixture#pipe-deck@0.2.0"
PIPEWIRE_ID = "registry:opaque-pipewire-node"
LEGACY_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
REGISTRY_SOURCE = "registry+https://index.crates.io/"
SPARSE_SOURCE = "sparse+https://index.crates.io/"
REPRESENTATIVE_TARGETS = (
    ("x86_64-unknown-linux-gnu", True),
    ("x86_64-pc-windows-msvc", False),
)
sys.dont_write_bytecode = True


def load_checker():
    spec = importlib.util.spec_from_file_location("target_dependency_checker", SCRIPT_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError(f"unable to load checker from {SCRIPT_PATH}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def package(
    package_id: str,
    name: str,
    version: str,
    source: str | None,
    manifest_path: str,
) -> dict[str, object]:
    return {
        "id": package_id,
        "name": name,
        "version": version,
        "source": source,
        "manifest_path": manifest_path,
    }


def metadata_fixture(
    manifest_path: Path,
    *,
    dependency_name: str | None = None,
    dependency_version: str = "0.10.0",
    dependency_source: str | None = LEGACY_SOURCE,
    dependency_id: str = PIPEWIRE_ID,
    direct_edge_name: str | None = "pipewire",
    dependency_is_resolved: bool = True,
    include_root_node: bool = True,
    include_resolve: bool = True,
    include_source: bool = True,
) -> dict[str, object]:
    packages = [package(ROOT_ID, "pipe-deck", "0.2.0", None, str(manifest_path))]
    nodes: list[dict[str, object]] = []
    edges: list[dict[str, object]] = []
    if dependency_name is not None:
        dependency = package(
            dependency_id,
            dependency_name,
            dependency_version,
            dependency_source,
            "/fixture/dependency/Cargo.toml",
        )
        if not include_source:
            dependency.pop("source")
        packages.append(dependency)
        if dependency_is_resolved:
            nodes.append({"id": dependency_id, "deps": []})
        if direct_edge_name is not None:
            edges.append({"name": direct_edge_name, "pkg": dependency_id})
    if include_root_node:
        nodes.insert(0, {"id": ROOT_ID, "deps": edges})
    metadata: dict[str, object] = {"packages": packages}
    if include_resolve:
        metadata["resolve"] = {"root": ROOT_ID, "nodes": nodes}
    return metadata


def valid_matrix(manifest_path: Path) -> dict[str, dict[str, object]]:
    return {
        target: metadata_fixture(
            manifest_path,
            dependency_name="pipewire" if expects_pipewire else None,
        )
        for target, expects_pipewire in REPRESENTATIVE_TARGETS
    }


class CheckerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.checker = load_checker()

    def validate_positive(self, **fixture_options) -> object:
        metadata = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            **fixture_options,
        )
        try:
            return self.checker.validate_target_metadata(metadata, "linux-fixture", True)
        except AssertionError as error:
            self.fail(f"valid direct PipeWire dependency was rejected: {error}")

    def assert_positive_rejected(self, message: str, **fixture_options) -> None:
        metadata = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            **fixture_options,
        )
        with self.assertRaisesRegex(AssertionError, message):
            self.checker.validate_target_metadata(metadata, "linux-fixture", True)

    def run_main(
        self,
        metadata_by_target: dict[str, dict[str, object]],
        *,
        mode: str = "metadata",
    ) -> tuple[str, list[list[str]]]:
        with tempfile.TemporaryDirectory(prefix="pipe-deck-355-fixture-") as temp_dir:
            fixture_dir = Path(temp_dir)
            metadata_path = fixture_dir / "metadata.json"
            args_path = fixture_dir / "args.jsonl"
            cargo_path = fixture_dir / "cargo-fixture.py"
            metadata_path.write_text(json.dumps(metadata_by_target))
            cargo_path.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, pathlib, sys\n"
                "args = sys.argv[1:]\n"
                "with pathlib.Path(os.environ['FIXTURE_ARGS_LOG']).open('a') as log:\n"
                "    log.write(json.dumps(args) + '\\n')\n"
                "mode = os.environ['FIXTURE_MODE']\n"
                "if mode == 'nonzero': raise SystemExit(42)\n"
                "if mode == 'malformed': print('{not json'); raise SystemExit(0)\n"
                "target = args[args.index('--filter-platform') + 1]\n"
                "data = json.loads(pathlib.Path(os.environ['FIXTURE_METADATA']).read_text())\n"
                "print(json.dumps(data[target]))\n"
            )
            cargo_path.chmod(cargo_path.stat().st_mode | stat.S_IXUSR)
            environment = {
                "CARGO": str(cargo_path),
                "FIXTURE_ARGS_LOG": str(args_path),
                "FIXTURE_METADATA": str(metadata_path),
                "FIXTURE_MODE": mode,
            }
            output = io.StringIO()
            with mock.patch.dict(os.environ, environment, clear=False), redirect_stdout(output):
                self.checker.main()
            calls = [json.loads(line) for line in args_path.read_text().splitlines()]
            return output.getvalue(), calls

    def test_actual_manifest_declares_pipewire_only_for_linux(self) -> None:
        with self.checker.MANIFEST_PATH.open("rb") as manifest_file:
            manifest = tomllib.load(manifest_file)
        self.assertEqual(self.checker.validate_manifest_declaration(manifest), "pipewire")

    def test_manifest_rejects_freebsd_widening(self) -> None:
        manifest_text = self.checker.MANIFEST_PATH.read_text()
        widened = manifest_text.replace(
            "[target.'cfg(target_os = \"linux\")'.dependencies]",
            "[target.'cfg(any(target_os = \"linux\", target_os = \"freebsd\"))'.dependencies]",
        )
        self.assertNotEqual(widened, manifest_text)
        with self.assertRaisesRegex(AssertionError, "exactly once"):
            self.checker.validate_manifest_declaration(tomllib.loads(widened))

    def test_graph_matrix_is_representative_not_exhaustive(self) -> None:
        self.assertEqual(self.checker.TARGET_EXPECTATIONS, REPRESENTATIVE_TARGETS)

    def test_accepts_renamed_direct_dependency(self) -> None:
        identity = self.validate_positive(direct_edge_name="pw")
        self.assertEqual(identity.edge_name, "pw")

    def test_accepts_semver_build_metadata(self) -> None:
        identity = self.validate_positive(dependency_version="0.10.0+build.1")
        self.assertEqual(identity.version, "0.10.0+build.1")

    def test_accepts_legacy_crates_io_source(self) -> None:
        self.validate_positive(dependency_source=LEGACY_SOURCE)

    def test_accepts_registry_index_source(self) -> None:
        self.validate_positive(dependency_source=REGISTRY_SOURCE)

    def test_accepts_sparse_source(self) -> None:
        self.validate_positive(dependency_source=SPARSE_SOURCE)

    def test_rejects_prerelease_and_malformed_versions(self) -> None:
        malformed = (
            "0.10.0-alpha.1",
            "0.10.0+",
            "0.10.0+build..1",
            "00.10.0",
            "0.10",
            "0.11.0",
        )
        for version in malformed:
            with self.subTest(version=version):
                self.assertFalse(self.checker.is_expected_pipewire_version(version))

    def test_requires_root_direct_edge(self) -> None:
        self.assert_positive_rejected("direct dependency", direct_edge_name=None)

    def test_inventory_only_package_is_rejected(self) -> None:
        self.assert_positive_rejected("unresolved", dependency_is_resolved=False)

    def test_wrong_package_is_rejected(self) -> None:
        metadata = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="unrelated",
            direct_edge_name="pipewire",
        )
        with self.assertRaisesRegex(AssertionError, "resolving to pipewire"):
            self.checker.validate_target_metadata(metadata, "linux-fixture", True)

    def test_wrong_version_is_rejected(self) -> None:
        self.assert_positive_rejected("version", dependency_version="0.11.0")

    def test_wrong_source_is_rejected(self) -> None:
        self.assert_positive_rejected(
            "source",
            dependency_source="git+https://example.invalid/pipewire",
        )

    def test_negative_graph_rejects_renamed_crates_io_pipewire(self) -> None:
        metadata = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            dependency_id="urn:opaque:dependency:42",
            direct_edge_name="pw",
        )
        with self.assertRaisesRegex(AssertionError, "resolved graph"):
            self.checker.validate_target_metadata(metadata, "windows-fixture", False)

    def test_negative_graph_allows_unrelated_same_name_package(self) -> None:
        metadata = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            dependency_version="99.0.0",
            dependency_source="git+https://example.invalid/unrelated",
            direct_edge_name=None,
        )
        self.checker.validate_target_metadata(metadata, "windows-fixture", False)

    def test_missing_resolve_is_rejected(self) -> None:
        self.assert_positive_rejected("resolve", include_resolve=False)

    def test_missing_root_node_is_rejected(self) -> None:
        self.assert_positive_rejected("root node", include_root_node=False)

    def test_missing_source_identity_is_rejected(self) -> None:
        self.assert_positive_rejected("source", include_source=False)

    def test_malformed_dependency_identity_is_rejected(self) -> None:
        for options in (
            {"dependency_id": ""},
            {"dependency_source": ""},
        ):
            with self.subTest(options=options):
                self.assert_positive_rejected("nonempty|malformed", **options)

    def test_wrong_root_manifest_is_rejected(self) -> None:
        metadata = metadata_fixture(Path("/fixture/wrong/Cargo.toml"), dependency_name="pipewire")
        with self.assertRaisesRegex(AssertionError, "manifest"):
            self.checker.validate_target_metadata(metadata, "linux-fixture", True)

    def test_main_uses_locked_exact_manifest_for_representative_targets(self) -> None:
        output, calls = self.run_main(valid_matrix(self.checker.MANIFEST_PATH))
        called_targets = []
        for arguments in calls:
            self.assertIn("--locked", arguments)
            manifest_index = arguments.index("--manifest-path") + 1
            self.assertEqual(arguments[manifest_index], str(self.checker.MANIFEST_PATH))
            called_targets.append(arguments[arguments.index("--filter-platform") + 1])
        self.assertEqual(called_targets, [target for target, _ in REPRESENTATIVE_TARGETS])
        self.assertIn("direct pipewire -> pipewire 0.10.0", output)

    def test_main_fails_closed_for_cargo_nonzero(self) -> None:
        with self.assertRaisesRegex(AssertionError, "exit 42"):
            self.run_main(valid_matrix(self.checker.MANIFEST_PATH), mode="nonzero")

    def test_main_fails_closed_for_malformed_json(self) -> None:
        with self.assertRaisesRegex(AssertionError, "malformed JSON"):
            self.run_main(valid_matrix(self.checker.MANIFEST_PATH), mode="malformed")

    def test_checker_does_not_import_test_support(self) -> None:
        original_import = builtins.__import__

        def reject_test_import(name, *args, **kwargs):
            if name == "test_check_target_dependencies":
                raise AssertionError("production checker imported test support")
            return original_import(name, *args, **kwargs)

        with mock.patch("builtins.__import__", side_effect=reject_test_import):
            self.run_main(valid_matrix(self.checker.MANIFEST_PATH))

    def test_make_target_runs_tests_then_checker_once_each(self) -> None:
        result = subprocess.run(
            ["make", "-n", "check-target-dependencies"],
            cwd=REPOSITORY_ROOT,
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(
            result.stdout.splitlines(),
            [
                "python3 scripts/test_check_target_dependencies.py",
                "python3 scripts/check-target-dependencies.py",
            ],
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
