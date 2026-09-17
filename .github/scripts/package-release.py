#!/usr/bin/env python3
"""Package a built Turtle executable into a distributable archive.

Uses only the Python standard library so no extra install step is needed
on any of the GitHub-hosted or containerized runners.
"""

import argparse
import hashlib
import os
import shutil
import sys
import tarfile
import zipfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

# Root-level files that exist in this repository and are safe to include
# if present. Missing files are skipped rather than causing a failure.
OPTIONAL_EXTRA_FILES = [
    "README.md",
    "LICENSE",
    "AGENTS.md",
    "cpp-checks.json",
    "python-checks.json",
    "web-checks.json",
]


def parse_args():
    parser = argparse.ArgumentParser(description="Package a Turtle release archive.")
    parser.add_argument("--target", required=True, help="Rust target triple, e.g. aarch64-apple-darwin")
    parser.add_argument("--name", required=True, help="Human-readable build name, e.g. macos-apple-silicon")
    parser.add_argument("--binary", required=True, help="Path to the built executable")
    parser.add_argument("--archive-ext", required=True, choices=["tar.gz", "zip"], help="Archive format to produce")
    return parser.parse_args()


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> int:
    args = parse_args()

    binary_path = REPO_ROOT / args.binary
    if not binary_path.is_file():
        print(f"ERROR: built executable not found at {binary_path}", file=sys.stderr)
        return 1

    dist_dir = REPO_ROOT / "dist"
    dist_dir.mkdir(parents=True, exist_ok=True)

    stage_name = f"turtle-{args.name}"
    stage_dir = dist_dir / stage_name
    if stage_dir.exists():
        shutil.rmtree(stage_dir)
    stage_dir.mkdir(parents=True)

    staged_binary = stage_dir / binary_path.name
    shutil.copy2(binary_path, staged_binary)
    if os.name != "nt":
        os.chmod(staged_binary, 0o755)

    for filename in OPTIONAL_EXTRA_FILES:
        source = REPO_ROOT / filename
        if source.is_file():
            shutil.copy2(source, stage_dir / filename)

    metadata_lines = [
        f"target = {args.target}",
        f"build_name = {args.name}",
        f"binary = {binary_path.name}",
    ]
    (stage_dir / "BUILD_INFO.txt").write_text("\n".join(metadata_lines) + "\n", encoding="utf-8")

    archive_base = dist_dir / stage_name

    if args.archive_ext == "tar.gz":
        archive_path = Path(f"{archive_base}.tar.gz")
        with tarfile.open(archive_path, "w:gz") as tar:
            tar.add(stage_dir, arcname=stage_name)
    else:
        archive_path = Path(f"{archive_base}.zip")
        with zipfile.ZipFile(archive_path, "w", zipfile.ZIP_DEFLATED) as zf:
            for file_path in stage_dir.rglob("*"):
                if file_path.is_file():
                    zf.write(file_path, arcname=file_path.relative_to(stage_dir.parent))

    checksum = sha256_of(archive_path)
    checksum_path = Path(f"{archive_path}.sha256")
    checksum_path.write_text(f"{checksum}  {archive_path.name}\n", encoding="utf-8")

    shutil.rmtree(stage_dir)

    print(f"Packaged: {archive_path}")
    print(f"Checksum: {checksum}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

