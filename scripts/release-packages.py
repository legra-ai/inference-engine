"""Persist original release archives and verify registry identity on every retry."""

import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from urllib.error import HTTPError
from urllib.request import urlopen
import zipfile
import runpy

python_version = runpy.run_path(str(Path(__file__).with_name("release-version.py")))["python_version"]
TARGETS = ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "x86_64-apple-darwin", "aarch64-apple-darwin", "x86_64-pc-windows-msvc")


def digest(path, algorithm="sha256"):
    with path.open("rb") as source:
        return hashlib.file_digest(source, algorithm).digest()


def registry_json(url):
    try:
        with urlopen(url, timeout=60) as response:
            return json.load(response)
    except HTTPError as error:
        if error.code == 404:
            return None
        raise


def npm_exists(version, archive):
    metadata = registry_json(f"https://registry.npmjs.org/inference-engine/{version}")
    if metadata is None:
        return False
    expected = "sha512-" + base64.b64encode(digest(archive, "sha512")).decode()
    if metadata["dist"]["integrity"] != expected:
        raise ValueError("npm package identity differs from the original release archive")
    return True


def missing_python(version, wheels):
    metadata = registry_json(f"https://pypi.org/pypi/inference-engine/{python_version(version)}/json")
    published = {entry["filename"]: entry["digests"]["sha256"]
                 for entry in metadata["urls"]} if metadata else {}
    missing = []
    for wheel in wheels:
        if wheel.name not in published:
            missing.append(wheel)
        elif published[wheel.name] != digest(wheel).hex():
            raise ValueError(f"PyPI wheel identity differs: {wheel.name}")
    return missing


def write_bundle(bundle, version, revision, packages):
    manifest = {"version": version, "revision": revision, "files": {
        package.name: digest(package).hex() for package in packages
    }}
    if len(manifest["files"]) != len(packages):
        raise ValueError("Duplicate package filenames")
    with zipfile.ZipFile(bundle, "x", compression=zipfile.ZIP_STORED) as archive:
        archive.writestr("manifest.json", json.dumps(manifest, sort_keys=True))
        for package in packages:
            archive.write(package, package.name)


def read_bundle(bundle, version, revision, output):
    with zipfile.ZipFile(bundle) as archive:
        manifest = json.loads(archive.read("manifest.json"))
        if (manifest["version"], manifest["revision"]) != (version, revision):
            raise ValueError("Release bundle version/revision mismatch")
        names = archive.namelist()
        if len(names) != len(set(names)) or set(names) != {"manifest.json", *manifest["files"]}:
            raise ValueError("Release bundle contains unexpected files")
        output.mkdir()
        for name, expected in manifest["files"].items():
            if Path(name).name != name or name in (".", ".."):
                raise ValueError("Invalid release package filename")
            target = output / name
            with archive.open(name) as source, target.open("xb") as destination:
                shutil.copyfileobj(source, destination)
            if digest(target).hex() != expected:
                raise ValueError(f"Release bundle package identity mismatch: {name}")


def inventory(directory, version):
    """Require one source archive and five distinct native-wheel platforms."""
    py = python_version(version)
    wheels = sorted(directory.glob(f"inference_engine-{py}-*.whl"))
    valid = (len(wheels) == 5
             and sum("manylinux_2_28_x86_64" in p.name for p in wheels) == 1
             and sum("manylinux_2_28_aarch64" in p.name for p in wheels) == 1
             and sum("macosx_" in p.name and p.name.endswith("x86_64.whl") for p in wheels) == 1
             and sum("macosx_" in p.name and p.name.endswith("arm64.whl") for p in wheels) == 1
             and sum(p.name.endswith("win_amd64.whl") for p in wheels) == 1)
    source = directory / f"inference_engine-{py}.tar.gz"
    if not valid or not source.is_file():
        raise ValueError("Incomplete or duplicate Python release inventory")
    return [source, *wheels]


def stage(version, revision):
    """Restore immutable original archives when any publication is interrupted."""
    tag = f"v{version}"
    bundle = Path("release-packages.zip")
    assets = json.loads(subprocess.check_output(
        ["gh", "release", "view", tag, "--json", "assets"], text=True
    ))["assets"]
    if not any(asset["name"] == bundle.name for asset in assets):
        required = {f"inference-engine.{target}.node" for target in TARGETS}
        actual = {path.name for path in Path("dist").glob("*.node")}
        if actual != required:
            raise ValueError("Incomplete or unexpected npm native binary inventory")
        for name in required:
            shutil.copyfile(Path("dist") / name, name)
        result = json.loads(subprocess.check_output(
            ["npm", "pack", "--json", "--ignore-scripts"], text=True
        ))
        if len(result) != 1:
            raise ValueError("Expected one npm archive")
        packages = [Path(result[0]["filename"]), *inventory(Path("dist"), version)]
        write_bundle(bundle, version, revision, packages)
        subprocess.run(["gh", "release", "upload", tag, str(bundle)], check=True)
    else:
        subprocess.run(["gh", "release", "download", tag, "--pattern", bundle.name], check=True)
    read_bundle(bundle, version, revision, Path("release-packages"))
    inventory(Path("release-packages"), version)


def output(name, value):
    with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
        stream.write(f"{name}={value}\n")


def main():
    command = sys.argv[1]
    version = os.environ["RELEASE_VERSION"]
    if command == "stage":
        stage(version, os.environ["RELEASE_REVISION"])
        return
    directory = Path("release-packages")
    if command == "check-npm":
        archives = list(directory.glob("*.tgz"))
        if len(archives) != 1:
            raise ValueError("Expected exactly one npm archive")
        output("npm_missing", str(not npm_exists(version, archives[0])).lower())
        output("npm_archive", str(archives[0]))
    elif command == "filter-python":
        wheels = inventory(directory, version)
        missing = missing_python(version, wheels)
        pending = Path("pending-packages")
        pending.mkdir()
        for wheel in missing:
            shutil.copyfile(wheel, pending / wheel.name)
        output("python_missing", str(bool(missing)).lower())
    elif command == "verify-python":
        missing = missing_python(version, inventory(directory, version))
        if missing:
            raise ValueError(f"Missing Python release archives: {missing}")
    elif command == "check-build":
        inventory(Path("dist"), version)
    else:
        raise ValueError(f"Unknown release operation: {command}")


if __name__ == "__main__":
    main()
