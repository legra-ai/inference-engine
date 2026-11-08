"""Test an installed npm tarball, including native loading and license inventory."""
import json
from pathlib import Path
import os
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
NPM = shutil.which('npm.cmd' if os.name == 'nt' else 'npm')
if NPM is None:
    raise RuntimeError('npm is required')
result = json.loads(subprocess.check_output([NPM, 'pack', '--json', '--ignore-scripts'], cwd=ROOT, text=True))
if len(result) != 1:
    raise ValueError('Expected exactly one npm archive')
archive = ROOT / result[0]['filename']
with tarfile.open(archive) as bundle:
    for name in ['LICENSE-APACHE', 'LICENSE-MIT']:
        if bundle.extractfile('package/' + name).read() != (ROOT / name).read_bytes():
            raise ValueError(f'Packaged license differs: {name}')
    if not any(name.endswith('.node') for name in bundle.getnames()):
        raise ValueError('npm archive omits native bindings')
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    (root / 'package.json').write_text('{"name":"inference-engine-install-test","private":true}')
    subprocess.run([NPM, 'install', '--ignore-scripts', '--no-audit', '--no-fund', str(archive)], cwd=root, check=True)
    shutil.copyfile(ROOT / 'scripts/test-node.cjs', root / 'test-native.cjs')
    env = dict(os.environ, INFERENCE_ENGINE_PACKAGE=str(root / 'node_modules/inference-engine'))
    subprocess.run(['node', '--test', 'test-native.cjs'], cwd=root, env=env, check=True)
