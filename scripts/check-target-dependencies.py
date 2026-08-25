#!/usr/bin/env python3
"""Check target-filtered Cargo dependency invariants."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = REPOSITORY_ROOT / "src-tauri" / "Cargo.toml"
LINUX_TARGET = "x86_64-unknown-linux-gnu"
WINDOWS_TARGET = "x86_64-pc-windows-msvc"


def resolved_package_names(target: str) -> set[str]:
    cargo = os.environ.get("CARGO", "cargo")
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
    metadata = json.loads(result.stdout)
    package_names = {
        package["id"]: package["name"] for package in metadata["packages"]
    }
    return {
        package_names[node["id"]]
        for node in metadata["resolve"]["nodes"]
    }


def main() -> None:
    print(f"check script: {Path(__file__).resolve()}")
    print(f"manifest: {MANIFEST_PATH}")

    windows_packages = resolved_package_names(WINDOWS_TARGET)
    linux_packages = resolved_package_names(LINUX_TARGET)

    assert "pipewire" not in windows_packages, (
        f"{WINDOWS_TARGET} resolved graph must exclude pipewire"
    )
    assert "pipewire" in linux_packages, (
        f"{LINUX_TARGET} resolved graph must include pipewire"
    )

    print(f"PASS: {WINDOWS_TARGET} excludes pipewire")
    print(f"PASS: {LINUX_TARGET} includes pipewire")


if __name__ == "__main__":
    main()
