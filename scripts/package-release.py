#!/usr/bin/env python3
"""Package native desktop/CLI builds using only the Python standard library."""

import argparse
import hashlib
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile


ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "x86_64-pc-windows-msvc": "windows-x86_64",
    "x86_64-unknown-linux-gnu": "linux-x86_64",
    "x86_64-apple-darwin": "macos-x86_64",
    "aarch64-apple-darwin": "macos-aarch64",
}


def validate_tag(tag):
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))[
        "workspace"
    ]["package"]["version"]
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", tag):
        raise ValueError("Release tag must use vMAJOR.MINOR.PATCH, optionally with a prerelease suffix")
    if tag != f"v{version}":
        raise ValueError(f"Tag {tag} does not match workspace version v{version} in Cargo.toml")
    return version


def copy_executable(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)
    destination.chmod(0o755)


def package(tag, target, binary_dir, output_dir):
    version = validate_tag(tag)
    platform = TARGETS[target]
    windows = platform.startswith("windows")
    macos = platform.startswith("macos")
    suffix = ".exe" if windows else ""
    desktop = binary_dir / f"desktop{suffix}"
    cli = binary_dir / f"cli{suffix}"
    for binary in (desktop, cli):
        if not binary.is_file():
            raise FileNotFoundError(f"Missing release binary: {binary}")
    name = f"gamdl-rs-{tag}-{platform}"
    output_dir.mkdir(parents=True, exist_ok=True)
    extension = ".tar.gz" if platform.startswith("linux") else ".zip"
    archive = output_dir.resolve() / f"{name}{extension}"

    with tempfile.TemporaryDirectory(prefix="gamdl-release-") as temporary:
        payload = Path(temporary) / name
        payload.mkdir()
        copy_executable(cli, payload / f"gamdl-rs-cli{suffix}")
        if macos:
            bundle = payload / "Music Downloader.app"
            contents = bundle / "Contents"
            copy_executable(desktop, contents / "MacOS" / "gamdl-rs")
            with (contents / "Info.plist").open("wb") as stream:
                plistlib.dump(
                    {
                        "CFBundleName": "Music Downloader",
                        "CFBundleDisplayName": "Music Downloader",
                        "CFBundleIdentifier": "com.gamdl.rust",
                        "CFBundleExecutable": "gamdl-rs",
                        "CFBundlePackageType": "APPL",
                        "CFBundleShortVersionString": version.split("-")[0].split("+")[0],
                        "CFBundleVersion": version.split("-")[0].split("+")[0],
                        "NSHighResolutionCapable": True,
                    },
                    stream,
                )
            # Ad-hoc signing also seals the bundle for Apple Silicon execution.
            # Developer ID signing and Apple notarization require separate credentials.
            subprocess.run(["codesign", "--force", "--sign", "-", str(bundle)], check=True)
        else:
            copy_executable(desktop, payload / f"gamdl-rs{suffix}")

        for source, destination in (
            (ROOT / "README.md", "README.md"),
            (ROOT / "CHANGELOG.md", "CHANGELOG.md"),
            (ROOT / "LICENSE", "LICENSE"),
            (ROOT / "crates/media/assets/NOTICE.md", "NOTICE.md"),
            (ROOT / "crates/media/assets/LICENSE.gamdl", "LICENSE.gamdl"),
            (ROOT / "scripts/release-running.md", "RUNNING.md"),
        ):
            shutil.copy2(source, payload / destination)

        if extension == ".tar.gz":
            with tarfile.open(archive, "w:gz") as stream:
                stream.add(payload, arcname=name)
        else:
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as stream:
                for path in sorted(payload.rglob("*")):
                    if path.is_file():
                        stream.write(path, path.relative_to(payload.parent).as_posix())

    with archive.open("rb") as stream:
        checksum = hashlib.file_digest(stream, "sha256").hexdigest()
    archive.with_name(f"{archive.name}.sha256").write_text(
        f"{checksum}  {archive.name}\n", encoding="utf-8"
    )
    print(archive)
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--validate-tag", help="Only validate the release tag against Cargo.toml")
    parser.add_argument("--tag")
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--binary-dir", type=Path)
    parser.add_argument("--output-dir", type=Path, default=Path("dist"))
    args = parser.parse_args()
    try:
        if args.validate_tag:
            print(validate_tag(args.validate_tag))
        else:
            if not all((args.tag, args.target, args.binary_dir)):
                parser.error("--tag, --target and --binary-dir are required when packaging")
            package(args.tag, args.target, args.binary_dir, args.output_dir)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Release packaging failed: {error}\n")


if __name__ == "__main__":
    main()
