#!/usr/bin/env python3
"""Deterministic regression tests for check-target-dependencies.py."""

from __future__ import annotations

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
import unittest
from unittest import mock


SCRIPT_PATH = Path(__file__).with_name("check-target-dependencies.py")
ROOT_ID = "path+file:///fixture#pipe-deck@0.2.0"
PIPEWIRE_ID = "registry:opaque-pipewire-node"
CRATES_IO_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
EXPECTED_TARGETS = (
    ("x86_64-unknown-linux-gnu", True),
    ("aarch64-unknown-linux-gnu", True),
    ("x86_64-pc-windows-msvc", False),
    ("x86_64-pc-windows-gnu", False),
    ("aarch64-apple-darwin", False),
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
    dependency_source: str | None = CRATES_IO_SOURCE,
    dependency_id: str = PIPEWIRE_ID,
    direct_edge_name: str | None = "pipewire",
    include_dependency_package: bool = True,
    include_dependency_node: bool = True,
    include_root_node: bool = True,
    include_resolve: bool = True,
    include_source_field: bool = True,
) -> dict[str, object]:
    root_package = package(
        ROOT_ID,
        "pipe-deck",
        "0.2.0",
        None,
        str(manifest_path),
    )
    packages = [root_package]
    nodes: list[dict[str, object]] = []
    deps: list[dict[str, object]] = []

    if dependency_name is not None:
        dependency = package(
            dependency_id,
            dependency_name,
            dependency_version,
            dependency_source,
            "/fixture/dependency/Cargo.toml",
        )
        if not include_source_field:
            dependency.pop("source")
        if include_dependency_package:
            packages.append(dependency)
        if include_dependency_node:
            nodes.append({"id": dependency_id, "deps": [], "dependencies": []})
        if direct_edge_name is not None:
            deps.append(
                {
                    "name": direct_edge_name,
                    "pkg": dependency_id,
                    "dep_kinds": [{"kind": None, "target": None}],
                }
            )

    if include_root_node:
        nodes.insert(0, {"id": ROOT_ID, "deps": deps, "dependencies": []})

    metadata: dict[str, object] = {
        "packages": packages,
        "workspace_members": [ROOT_ID],
        "workspace_root": str(manifest_path.parent.parent),
    }
    if include_resolve:
        metadata["resolve"] = {"root": ROOT_ID, "nodes": nodes}
    return metadata


def valid_matrix(manifest_path: Path) -> dict[str, dict[str, object]]:
    return {
        target: metadata_fixture(
            manifest_path,
            dependency_name="pipewire" if expects_pipewire else None,
        )
        for target, expects_pipewire in EXPECTED_TARGETS
    }


def assert_checker_contract(checker) -> None:
    actual_targets = getattr(checker, "TARGET_EXPECTATIONS", None)
    if actual_targets != EXPECTED_TARGETS:
        raise AssertionError(
            "checker target matrix must cover two Linux and three non-Linux targets"
        )

    validator = getattr(checker, "validate_target_metadata", None)
    if not callable(validator):
        raise AssertionError("checker must expose validate_target_metadata")

    manifest_path = checker.MANIFEST_PATH
    for target, expects_pipewire in EXPECTED_TARGETS:
        validator(
            valid_matrix(manifest_path)[target],
            target,
            expects_pipewire,
        )

    invalid_cases = (
        (
            "inventory-only package",
            metadata_fixture(
                manifest_path,
                dependency_name="pipewire",
                include_dependency_node=False,
            ),
        ),
        (
            "transitive-only dependency",
            metadata_fixture(
                manifest_path,
                dependency_name="pipewire",
                direct_edge_name=None,
            ),
        ),
        (
            "unrelated same-name dependency",
            metadata_fixture(
                manifest_path,
                dependency_name="pipewire",
                dependency_version="99.0.0",
                dependency_source="git+https://example.invalid/unrelated",
            ),
        ),
        (
            "wrong version",
            metadata_fixture(
                manifest_path,
                dependency_name="pipewire",
                dependency_version="0.11.0",
            ),
        ),
        (
            "wrong source",
            metadata_fixture(
                manifest_path,
                dependency_name="pipewire",
                dependency_source="git+https://example.invalid/pipewire",
            ),
        ),
        (
            "nonempty package inventory",
            metadata_fixture(
                manifest_path,
                dependency_name="unrelated",
                direct_edge_name="unrelated",
            ),
        ),
        (
            "missing dependency source identity",
            metadata_fixture(
                manifest_path,
                dependency_name="pipewire",
                include_source_field=False,
            ),
        ),
    )
    for label, metadata in invalid_cases:
        try:
            validator(metadata, "x86_64-unknown-linux-gnu", True)
        except AssertionError:
            continue
        raise AssertionError(f"checker accepted {label} mutant")

    opaque_id_metadata = metadata_fixture(
        manifest_path,
        dependency_name="pipewire",
        dependency_id="urn:opaque:dependency:42",
        direct_edge_name="renamed_pipewire",
    )
    try:
        validator(opaque_id_metadata, "x86_64-pc-windows-msvc", False)
    except AssertionError:
        pass
    else:
        raise AssertionError("checker accepted renamed crates.io PipeWire on Windows")

    unrelated_negative = metadata_fixture(
        manifest_path,
        dependency_name="pipewire",
        dependency_version="99.0.0",
        dependency_source="git+https://example.invalid/unrelated",
        direct_edge_name=None,
    )
    validator(unrelated_negative, "aarch64-apple-darwin", False)


class CheckerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.checker = load_checker()

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
                "if mode == 'nonzero':\n"
                "    raise SystemExit(42)\n"
                "if mode == 'malformed':\n"
                "    print('{not json')\n"
                "    raise SystemExit(0)\n"
                "target = args[args.index('--filter-platform') + 1]\n"
                "metadata = json.loads(pathlib.Path(os.environ['FIXTURE_METADATA']).read_text())\n"
                "print(json.dumps(metadata[target]))\n"
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
            arguments = [
                json.loads(line)
                for line in args_path.read_text().splitlines()
                if line
            ]
            return output.getvalue(), arguments

    def assert_rejected(
        self,
        metadata_by_target: dict[str, dict[str, object]],
        message: str,
    ) -> None:
        with self.assertRaisesRegex(AssertionError, message):
            self.run_main(metadata_by_target)

    def test_target_matrix_covers_linux_widely_and_three_non_linux_targets(self) -> None:
        _, calls = self.run_main(valid_matrix(self.checker.MANIFEST_PATH))
        called_targets = [
            args[args.index("--filter-platform") + 1]
            for args in calls
        ]
        self.assertEqual(called_targets, [target for target, _ in EXPECTED_TARGETS])

    def test_positive_target_requires_root_direct_edge(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-unknown-linux-gnu"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            direct_edge_name=None,
        )
        self.assert_rejected(matrix, "direct.*pipewire")

    def test_positive_target_rejects_unrelated_same_name_package(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-unknown-linux-gnu"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            dependency_version="99.0.0",
            dependency_source="git+https://example.invalid/unrelated",
        )
        self.assert_rejected(matrix, "source|version")

    def test_positive_target_rejects_wrong_version(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-unknown-linux-gnu"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            dependency_version="0.11.0",
        )
        self.assert_rejected(matrix, "version")

    def test_positive_target_rejects_wrong_source(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-unknown-linux-gnu"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            dependency_source="git+https://example.invalid/pipewire",
        )
        self.assert_rejected(matrix, "source")

    def test_inventory_only_pipewire_does_not_satisfy_linux(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-unknown-linux-gnu"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            include_dependency_node=False,
        )
        self.assert_rejected(matrix, "pipewire")

    def test_nonempty_linux_packages_do_not_satisfy_dependency(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-unknown-linux-gnu"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="unrelated",
            direct_edge_name="unrelated",
        )
        self.assert_rejected(matrix, "pipewire")

    def test_negative_target_rejects_renamed_crates_io_pipewire(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-pc-windows-msvc"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            dependency_id="urn:opaque:dependency:42",
            direct_edge_name="renamed_pipewire",
        )
        self.assert_rejected(matrix, "pipewire")

    def test_negative_target_allows_unrelated_same_name_package(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-pc-windows-msvc"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            dependency_version="99.0.0",
            dependency_source="git+https://example.invalid/unrelated",
            direct_edge_name=None,
        )
        try:
            self.run_main(matrix)
        except AssertionError as error:
            self.fail(f"unrelated same-name package was a false positive: {error}")

    def test_command_is_locked_and_uses_exact_manifest(self) -> None:
        _, calls = self.run_main(valid_matrix(self.checker.MANIFEST_PATH))
        for args in calls:
            self.assertIn("--locked", args)
            manifest_index = args.index("--manifest-path") + 1
            self.assertEqual(args[manifest_index], str(self.checker.MANIFEST_PATH))

    def test_valid_output_names_every_target_and_direct_identity(self) -> None:
        output, _ = self.run_main(valid_matrix(self.checker.MANIFEST_PATH))
        for target, expects_pipewire in EXPECTED_TARGETS:
            self.assertIn(target, output)
            if expects_pipewire:
                self.assertIn("direct pipewire -> pipewire 0.10.0", output)

    def test_wrong_root_manifest_is_rejected(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        wrong_manifest = Path("/fixture/wrong/Cargo.toml")
        matrix["x86_64-unknown-linux-gnu"] = metadata_fixture(
            wrong_manifest,
            dependency_name="pipewire",
        )
        self.assert_rejected(matrix, "manifest")

    def test_missing_resolve_is_fail_closed(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-unknown-linux-gnu"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            include_resolve=False,
        )
        with self.assertRaises((KeyError, AssertionError)):
            self.run_main(matrix)

    def test_missing_root_node_is_rejected(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-unknown-linux-gnu"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            include_root_node=False,
        )
        self.assert_rejected(matrix, "root")

    def test_missing_dependency_source_identity_is_rejected(self) -> None:
        matrix = valid_matrix(self.checker.MANIFEST_PATH)
        matrix["x86_64-unknown-linux-gnu"] = metadata_fixture(
            self.checker.MANIFEST_PATH,
            dependency_name="pipewire",
            include_source_field=False,
        )
        self.assert_rejected(matrix, "source")

    def test_cargo_nonzero_is_fail_closed(self) -> None:
        with self.assertRaises((subprocess.CalledProcessError, AssertionError)):
            self.run_main(valid_matrix(self.checker.MANIFEST_PATH), mode="nonzero")

    def test_malformed_json_is_fail_closed(self) -> None:
        with self.assertRaises((json.JSONDecodeError, AssertionError)):
            self.run_main(valid_matrix(self.checker.MANIFEST_PATH), mode="malformed")

    def test_embedded_contract_kills_named_mutants(self) -> None:
        assert_checker_contract(self.checker)


if __name__ == "__main__":
    unittest.main(verbosity=2)
