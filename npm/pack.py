"""Makes the npm packages of a release from its archives, and packs each with npm pack.

    python npm/pack.py <archives> <out>

<archives> holds the archives release.yml made (one per platform). Into <out> go the packed packages: one per
platform in PLATFORMS, holding what that platform's archive holds (the binary and the licenses), and the package
rotproof, which holds the script that runs the binary and depends on every platform package as optional, so npm
installs only the one for the os and cpu it runs on. Every package takes the version of Rotproof from Cargo.toml, as
the wheels do, and the description from pyproject.toml, so neither is written a second time.
"""

import json
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NPM = ROOT / "npm"

# The target of an archive -> the directory of its package in npm/, named as Node names the platform
# (process.platform-process.arch), which is how bin/rotproof.js finds it
PLATFORMS = {
    "x86_64-unknown-linux-gnu": "linux-x64",
    "x86_64-pc-windows-msvc": "win32-x64",
}


def main(archives: Path, out: Path) -> None:
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    description = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))["project"]["description"]
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as work:
        make(archives, Path(work), out, version, description)


def make(archives: Path, work: Path, out: Path, version: str, description: str) -> None:
    optional = {}
    for target, platform in PLATFORMS.items():
        found = [a for a in archives.iterdir() if a.name.endswith((f"-{target}.tar.gz", f"-{target}.zip"))]
        if len(found) != 1:
            sys.exit(f"one archive for {target} in {archives}, but {len(found)}: {[a.name for a in found]}")
        package = work / platform
        unpack(found[0], package)
        manifest = versioned(read(NPM / platform / "package.json"), version)
        write(package / "package.json", manifest)
        optional[manifest["name"]] = version
    unknown = [a.name for a in archives.iterdir() if not any(t in a.name for t in PLATFORMS)]
    if unknown:
        sys.exit(f"archives of no platform in PLATFORMS: {unknown}")

    package = work / "rotproof"
    shutil.copytree(NPM / "rotproof", package)
    for name in ["README.md", "LICENSE-MIT", "LICENSE-APACHE"]:
        shutil.copy(ROOT / name, package / name)
    manifest = versioned(read(package / "package.json"), version, description)
    manifest["optionalDependencies"] = optional
    write(package / "package.json", manifest)

    npm = shutil.which("npm")
    if npm is None:
        sys.exit("npm is not on the PATH")
    for package in sorted(work.iterdir()):
        subprocess.run([npm, "pack", "--pack-destination", str(out.resolve())], cwd=package, check=True)


def unpack(archive: Path, into: Path) -> None:
    """The files of the one directory at the top of the archive, into `into`, with their modes (the binary's x bit)"""
    staging = into.with_name(into.name + "-archive")
    if archive.name.endswith(".tar.gz"):
        with tarfile.open(archive) as tar:
            # The filter is in Python 3.12 and in 3.11 from 3.11.4; without it, 3.12 and 3.13 warn
            safe = {"filter": "data"} if hasattr(tarfile, "data_filter") else {}
            tar.extractall(staging, **safe)
    else:
        with zipfile.ZipFile(archive) as zip:
            zip.extractall(staging)
    tops = list(staging.iterdir())
    if len(tops) != 1 or not tops[0].is_dir():
        sys.exit(f"{archive.name} does not hold one directory at its top: {[t.name for t in tops]}")
    tops[0].rename(into)
    staging.rmdir()


def versioned(manifest: dict, version: str, description: str | None = None) -> dict:
    """The manifest with its version, and its description when given, after its name, where npm writes them"""
    head = {"name": manifest.pop("name"), "version": version}
    if description is not None:
        head["description"] = description
    return head | manifest


def read(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def write(path: Path, manifest: dict) -> None:
    path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(Path(sys.argv[1]), Path(sys.argv[2]))
