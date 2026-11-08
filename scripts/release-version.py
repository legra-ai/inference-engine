"""Cargo owns alpha identity; Python uses the equivalent PEP 440 spelling."""
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]

def python_version(version):
    match = re.fullmatch(r'(0\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*))-alpha\.([1-9]\d*)', version)
    if not match:
        raise ValueError('Only explicit zero-major alpha candidates may be published')
    return match[1] + 'a' + match[2]

def version():
    return tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']

def check():
    current = version()
    native = python_version(current)
    package = json.loads((ROOT / 'package.json').read_text())
    if package['version'] != current or package['publishConfig']['tag'] != 'alpha':
        raise ValueError('npm version/channel differs from Cargo alpha identity')
    if package['name'] != 'inference-engine':
        raise ValueError('Unexpected npm package name')
    print(f'Cargo/npm: {current}; Python: {native}; npm tag: alpha')

if __name__ == '__main__':
    check()
