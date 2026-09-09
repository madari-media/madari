#!/usr/bin/env python3
"""Fetch checksum-pinned MSYS2 libraries into target/windows-deps (Linux host)."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
DEST = ROOT / "target/windows-deps"
PACKAGES = DEST / "packages"


def fetch(package):
    archive = PACKAGES / package["filename"]
    if not archive.exists() or hashlib.sha256(archive.read_bytes()).hexdigest() != package["sha256"]:
        subprocess.run([
            "curl", "-fsSL", "--retry", "3", "--max-time", "180",
            "https://repo.msys2.org/mingw/mingw64/" + package["filename"],
            "-o", str(archive),
        ], check=True)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != package["sha256"]:
        raise RuntimeError(f"Checksum mismatch: {archive}")
    return archive


def main():
    PACKAGES.mkdir(parents=True, exist_ok=True)
    packages = json.loads((ROOT / "scripts/windows-dependencies.json").read_text())
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        for index, archive in enumerate(pool.map(fetch, packages), 1):
            subprocess.run([
                "tar", "-xf", str(archive), "-C", str(DEST),
                "--wildcards", "mingw64/*",
            ], check=True)
            print(f"[{index}/{len(packages)}] {archive.name}", flush=True)
    (DEST / ".complete").write_text("Dependencies extracted successfully.\n")


if __name__ == "__main__":
    main()
