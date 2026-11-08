"""Verify distributable content and alpha metadata before registry publication."""
from pathlib import Path
import sys
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
    for license_name in ['LICENSE-MIT', 'LICENSE-APACHE']:
        if not any(name.endswith('/' + license_name) for name in names):
            raise ValueError(f'Missing {license_name}')
    print(f'Checked {path.name}')

if __name__ == '__main__':
    for argument in sys.argv[1:]:
        check(Path(argument))
