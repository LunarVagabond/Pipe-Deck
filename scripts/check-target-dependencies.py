#!/usr/bin/env python3
"""Check target-filtered Cargo dependency invariants."""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import subprocess
import tomllib
from typing import NamedTuple


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = (REPOSITORY_ROOT / "src-tauri" / "Cargo.toml").resolve()
TARGET_EXPECTATIONS = (
    ("x86_64-unknown-linux-gnu", True),
    ("x86_64-pc-windows-msvc", False),
)
LINUX_TARGET_EXPRESSION = 'cfg(target_os = "linux")'
DEPENDENCY_TABLE_NAMES = ("dependencies", "build-dependencies", "dev-dependencies")
CRATES_IO_SOURCES = frozenset(
    {
        "registry+https://github.com/rust-lang/crates.io-index",
        "registry+https://index.crates.io/",
        "sparse+https://index.crates.io/",
    }
)


class DependencyIdentity(NamedTuple):
    edge_name: str
    package_id: str
    package_name: str
    version: str
    source: str


def fail(message: str) -> None:
    raise AssertionError(message)


def require_mapping(value: object, label: str) -> dict[str, object]:
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    return value


def require_list(value: object, label: str) -> list[object]:
    if not isinstance(value, list):
        fail(f"{label} must be a JSON array")
    return value


def require_string(value: object, label: str) -> str:
    if not isinstance(value, str) or not value:
        fail(f"{label} must be a nonempty string")
    return value


def dependency_package_name(alias: str, specification: object, label: str) -> str:
    if isinstance(specification, str):
        return alias
    if not isinstance(specification, dict):
        fail(f"{label} must be a version string or dependency table")
    package_name = specification.get("package", alias)
    return require_string(package_name, f"{label}.package")


def validate_manifest_declaration(manifest_value: object) -> str:
    manifest = require_mapping(manifest_value, "manifest")
    declarations: list[tuple[str | None, str, str]] = []

    def inspect_table(
        target_expression: str | None,
        table_name: str,
        table_value: object,
    ) -> None:
        table = require_mapping(table_value, f"manifest {table_name}")
        for alias, specification in table.items():
            package_name = dependency_package_name(
                alias,
                specification,
                f"manifest dependency {alias}",
            )
            if package_name == "pipewire":
                declarations.append((target_expression, table_name, alias))

    for table_name in DEPENDENCY_TABLE_NAMES:
        if table_name in manifest:
            inspect_table(None, table_name, manifest[table_name])

    targets = require_mapping(manifest.get("target", {}), "manifest target")
    for target_expression, target_value in targets.items():
        target = require_mapping(
            target_value,
            f"manifest target {target_expression}",
        )
        for table_name in DEPENDENCY_TABLE_NAMES:
            if table_name in target:
                inspect_table(target_expression, table_name, target[table_name])

    expected_location = (LINUX_TARGET_EXPRESSION, "dependencies")
    matching = [
        alias
        for target_expression, table_name, alias in declarations
        if (target_expression, table_name) == expected_location
    ]
    if len(declarations) != 1 or len(matching) != 1:
        fail(
            "pipewire must be declared exactly once in "
            f"target.{LINUX_TARGET_EXPRESSION!r}.dependencies; found {declarations!r}"
        )
    return matching[0]


def load_manifest() -> dict[str, object]:
    try:
        with MANIFEST_PATH.open("rb") as manifest_file:
            value = tomllib.load(manifest_file)
    except (OSError, tomllib.TOMLDecodeError) as error:
        fail(f"manifest {MANIFEST_PATH} could not be parsed: {error}")
    return require_mapping(value, f"manifest {MANIFEST_PATH}")


def package_index(metadata: dict[str, object], target: str) -> dict[str, dict[str, object]]:
    packages = require_list(metadata.get("packages"), f"{target}: packages")
    indexed: dict[str, dict[str, object]] = {}
    for position, value in enumerate(packages):
        package = require_mapping(value, f"{target}: packages[{position}]")
        package_id = require_string(
            package.get("id"),
            f"{target}: packages[{position}].id",
        )
        require_string(package.get("name"), f"{target}: package {package_id} name")
        require_string(package.get("version"), f"{target}: package {package_id} version")
        if "source" not in package:
            fail(f"{target}: package {package_id} source identity is missing")
        source = package["source"]
        if source is not None and (not isinstance(source, str) or not source):
            fail(f"{target}: package {package_id} source identity is malformed")
        require_string(
            package.get("manifest_path"),
            f"{target}: package {package_id} manifest_path",
        )
        if package_id in indexed:
            fail(f"{target}: duplicate package identity {package_id}")
        indexed[package_id] = package
    return indexed


def resolved_nodes(
    metadata: dict[str, object],
    packages: dict[str, dict[str, object]],
    target: str,
) -> tuple[str, dict[str, dict[str, object]]]:
    resolve = require_mapping(metadata.get("resolve"), f"{target}: resolve")
    root_id = require_string(resolve.get("root"), f"{target}: resolve.root")
    nodes = require_list(resolve.get("nodes"), f"{target}: resolve.nodes")
    indexed: dict[str, dict[str, object]] = {}
    for position, value in enumerate(nodes):
        node = require_mapping(value, f"{target}: resolve.nodes[{position}]")
        node_id = require_string(
            node.get("id"),
            f"{target}: resolve.nodes[{position}].id",
        )
        if node_id not in packages:
            fail(f"{target}: resolved node {node_id} has no package identity")
        if node_id in indexed:
            fail(f"{target}: duplicate resolved node {node_id}")
        require_list(node.get("deps"), f"{target}: node {node_id} deps")
        indexed[node_id] = node
    if root_id not in indexed:
        fail(f"{target}: root node {root_id} is missing from resolve.nodes")
    return root_id, indexed


def dependency_edges(
    node: dict[str, object],
    resolved: dict[str, dict[str, object]],
    packages: dict[str, dict[str, object]],
    target: str,
) -> list[dict[str, object]]:
    raw_edges = require_list(node.get("deps"), f"{target}: root dependency edges")
    edges: list[dict[str, object]] = []
    for position, value in enumerate(raw_edges):
        edge = require_mapping(value, f"{target}: root dependency edge {position}")
        require_string(edge.get("name"), f"{target}: dependency edge {position} name")
        package_id = require_string(
            edge.get("pkg"),
            f"{target}: dependency edge {position} package identity",
        )
        if package_id not in resolved or package_id not in packages:
            fail(
                f"{target}: dependency edge {position} references unresolved package {package_id}"
            )
        edges.append(edge)
    return edges


def is_expected_pipewire_version(version: str) -> bool:
    match = re.fullmatch(
        r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
        r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?",
        version,
    )
    if match is None:
        return False
    parsed = tuple(int(component) for component in match.groups())
    return (0, 10, 0) <= parsed < (0, 11, 0)


def validate_target_metadata(
    metadata_value: object,
    target: str,
    expects_pipewire: bool,
) -> DependencyIdentity | None:
    metadata = require_mapping(metadata_value, f"{target}: metadata")
    packages = package_index(metadata, target)
    root_id, resolved = resolved_nodes(metadata, packages, target)
    root_package = packages[root_id]
    if root_package["name"] != "pipe-deck":
        fail(f"{target}: resolve root must be the pipe-deck package")
    root_manifest = Path(str(root_package["manifest_path"])).resolve()
    if root_manifest != MANIFEST_PATH:
        fail(
            f"{target}: root manifest {root_manifest} does not match {MANIFEST_PATH}"
        )
    edges = dependency_edges(resolved[root_id], resolved, packages, target)

    if expects_pipewire:
        pipewire_edges = [
            edge
            for edge in edges
            if packages[str(edge["pkg"])]["name"] == "pipewire"
        ]
        if len(pipewire_edges) != 1:
            fail(
                f"{target}: root must have exactly one direct dependency resolving "
                "to pipewire"
            )
        edge = pipewire_edges[0]
        package_id = str(edge["pkg"])
        package = packages[package_id]
        source = package["source"]
        if source not in CRATES_IO_SOURCES:
            fail(f"{target}: direct pipewire package has unexpected source {source!r}")
        version = str(package["version"])
        if not is_expected_pipewire_version(version):
            fail(f"{target}: direct pipewire package has unexpected version {version!r}")
        return DependencyIdentity(
            edge_name=str(edge["name"]),
            package_id=package_id,
            package_name="pipewire",
            version=version,
            source=str(source),
        )

    for package_id in resolved:
        package = packages[package_id]
        if package["name"] == "pipewire" and package["source"] in CRATES_IO_SOURCES:
            fail(
                f"{target}: resolved graph contains crates.io pipewire via {package_id}"
            )
    return None


def cargo_metadata(target: str) -> dict[str, object]:
    cargo = os.environ.get("CARGO", "cargo")
    try:
        result = subprocess.run(
            [
                cargo,
                "metadata",
                "--locked",
                "--manifest-path",
                str(MANIFEST_PATH),
                "--filter-platform",
                target,
                "--format-version",
                "1",
            ],
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        )
    except subprocess.CalledProcessError as error:
        fail(f"{target}: cargo metadata failed with exit {error.returncode}")
    try:
        value = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        fail(f"{target}: cargo metadata returned malformed JSON: {error}")
    return require_mapping(value, f"{target}: cargo metadata")


def main() -> None:
    print(f"check script: {Path(__file__).resolve()}")
    print(f"manifest: {MANIFEST_PATH}")
    dependency_alias = validate_manifest_declaration(load_manifest())
    print(
        "PASS: manifest declares crates.io pipewire only in "
        f"target.{LINUX_TARGET_EXPRESSION!r}.dependencies as {dependency_alias!r}"
    )

    for target, expects_pipewire in TARGET_EXPECTATIONS:
        identity = validate_target_metadata(
            cargo_metadata(target),
            target,
            expects_pipewire,
        )
        if identity is None:
            print(
                f"PASS: {target}: no direct pipewire edge and no resolved "
                "crates.io pipewire package"
            )
        else:
            print(
                f"PASS: {target}: direct {identity.edge_name} -> "
                f"{identity.package_name} {identity.version} from {identity.source} "
                f"({identity.package_id})"
            )


if __name__ == "__main__":
    main()
