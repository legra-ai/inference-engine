"""Verify distributable content and alpha metadata before registry publication."""
from pathlib import Path
import sys
import subprocess
import tempfile
import tarfile
import zipfile


def check(path):
    if path.suffix == '.whl':
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            metadata = archive.read(next(name for name in names if name.endswith('.dist-info/METADATA'))).decode()
        if 'Development Status :: 3 - Alpha' not in metadata or '\nVersion: 0.' not in metadata:
            raise ValueError('Missing Python alpha metadata')
        if not any(name.endswith(('.so', '.pyd')) for name in names):
            raise ValueError('Missing native Python extension')
    else:
        with tarfile.open(path) as archive:
            names = archive.getnames()
        if not any(name.endswith('/tests/public_api.rs') for name in names):
            raise ValueError('Source distribution omits public API regression tests')
        if not any(name.endswith('/src/lib.rs') for name in names):
            raise ValueError('Source distribution omits Rust implementation')
        if not any(name.endswith('/build.rs') for name in names):
            raise ValueError('Source distribution omits required Cargo build script')
        with tempfile.TemporaryDirectory(prefix='inference-sdist-contract-') as directory:
            with tarfile.open(path) as archive:
                archive.extractall(directory, filter='data')
            roots = list(Path(directory).iterdir())
            if len(roots) != 1:
                raise ValueError('Expected one source archive root')
            subprocess.run(['cargo', 'metadata', '--locked', '--format-version', '1',
                            '--manifest-path', 'bindings/python/Cargo.toml',
                            '--features', 'pyo3/extension-module'], cwd=roots[0],
                           stdout=subprocess.DEVNULL, check=True)
    for license_name in ['LICENSE-MIT', 'LICENSE-APACHE']:
        if not any(name.endswith('/' + license_name) for name in names):
            raise ValueError(f'Missing {license_name}')
    print(f'Checked {path.name}')

if __name__ == '__main__':
    for argument in sys.argv[1:]:
        check(Path(argument))
