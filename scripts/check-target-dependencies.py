#!/usr/bin/env python3
"""Repository-specific Linux dependency guard; unusual layouts must update it."""

import json, os, subprocess, tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST, LINUX = ROOT / "src-tauri" / "Cargo.toml", 'cfg(target_os = "linux")'
PIPEWIRE_SOURCE, PIPEWIRE_VERSION = "registry+https://github.com/rust-lang/crates.io-index", "0.10."
SD_NOTIFY_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
GROUPS = ("dependencies", "build-dependencies", "dev-dependencies")
TARGETS = (("x86_64-unknown-linux-gnu", True), ("x86_64-pc-windows-msvc", False))


def is_default_pipewire(alias: str, specification: object) -> bool:
    if isinstance(specification, str):
        return alias == "pipewire"
    if not isinstance(specification, dict) or specification.get("package", alias) != "pipewire":
        return False
    if "git" in specification or "path" in specification:
        return False
    assert not ({"workspace", "registry"} & specification.keys()), (
        "workspace/registry PipeWire layouts must update this repository-specific check"
    )
    return True


def check_manifest(manifest: dict) -> None:
    locations, sd_notify_locations = [], []

    def scan(target: str | None, group: str, dependencies: dict) -> None:
        for alias, specification in dependencies.items():
            if is_default_pipewire(alias, specification):
                locations.append((target, group, alias))
            if alias == "sd-notify":
                sd_notify_locations.append((target, group, alias, specification))

    for group in GROUPS:
        scan(None, group, manifest.get(group, {}))
    for target, tables in manifest.get("target", {}).items():
        for group in GROUPS:
            scan(target, group, tables.get(group, {}))
    assert locations == [(LINUX, "dependencies", "pipewire")], (
        f"default-registry pipewire must appear exactly once in Linux dependencies; found {locations}"
    )
    assert sd_notify_locations == [(LINUX, "dependencies", "sd-notify", {"version": "0.5"})], (
        "default-registry sd-notify 0.5 must appear exactly once in Linux dependencies; "
        f"found {sd_notify_locations}"
    )


def check_target(metadata: dict, target: str, expected: bool):
    packages = {package["id"]: package for package in metadata["packages"]}
    resolve = metadata["resolve"]
    nodes = {node["id"]: node for node in resolve["nodes"]}
    assert resolve["root"] in nodes, f"{target}: Cargo metadata has no root node"
    matches = []
    for edge in nodes[resolve["root"]]["deps"]:
        package = packages[edge["pkg"]]
        if (package["name"] == "pipewire" and package["source"] == PIPEWIRE_SOURCE
                and package["version"].startswith(PIPEWIRE_VERSION)):
            matches.append((edge, package))
    assert len(matches) == int(expected), (
        f"{target}: expected {int(expected)} root-direct crates.io pipewire 0.10 edge, "
        f"found {len(matches)}"
    )
    return matches[0] if matches else None


def check_sd_notify_target(metadata: dict, target: str, expected: bool):
    packages = {package["id"]: package for package in metadata["packages"]}
    resolve = metadata["resolve"]
    nodes = {node["id"]: node for node in resolve["nodes"]}
    assert resolve["root"] in nodes, f"{target}: Cargo metadata has no root node"
    matches = []
    for edge in nodes[resolve["root"]]["deps"]:
        package = packages[edge["pkg"]]
        if package["name"] == "sd-notify" and package["source"] == SD_NOTIFY_SOURCE:
            matches.append((edge, package))
    assert len(matches) == int(expected), (
        f"{target}: expected {int(expected)} root-direct crates.io sd-notify edge, "
        f"found {len(matches)}"
    )
    return matches[0] if matches else None


def cargo_metadata(target: str) -> dict:
    result = subprocess.run(
        [os.environ.get("CARGO", "cargo"), "metadata", "--locked", "--manifest-path", str(MANIFEST),
         "--filter-platform", target, "--format-version", "1"],
        check=True, capture_output=True, text=True,
    )
    return json.loads(result.stdout)


def main() -> None:
    with MANIFEST.open("rb") as source:
        check_manifest(tomllib.load(source))
    print(f"PASS: {MANIFEST} declares default-registry pipewire and sd-notify only for target_os = linux")
    for target, expected in TARGETS:
        metadata = cargo_metadata(target)
        pipewire = check_target(metadata, target, expected)
        sd_notify = check_sd_notify_target(metadata, target, expected)
        if pipewire:
            edge, package = pipewire
            print(f"PASS: {target}: root-direct {edge['name']} -> {package['name']} "
                  f"{package['version']} from {package['source']}")
        else:
            print(f"PASS: {target}: no root-direct pipewire edge")
        if sd_notify:
            edge, package = sd_notify
            print(f"PASS: {target}: root-direct {edge['name']} -> {package['name']} "
                  f"{package['version']} from {package['source']}")
        else:
            print(f"PASS: {target}: no root-direct sd-notify edge")


if __name__ == "__main__":
    main()
