"""Install the built wheel into a clean environment and test outside the checkout."""
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile
import venv

ROOT = Path(__file__).resolve().parents[1]
wheels = list((ROOT / 'dist').glob('*.whl'))
if len(wheels) != 1:
    raise ValueError('Expected exactly one local platform wheel')
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    venv.create(root / 'venv', with_pip=True)
    python = root / 'venv' / ('Scripts/python.exe' if os.name == 'nt' else 'bin/python')
    subprocess.run([str(python), '-m', 'pip', '--isolated', 'install', '--no-index', str(wheels[0])], check=True, cwd=root)
    shutil.copyfile(ROOT / 'scripts/test-python.py', root / 'test_native.py')
    env = dict(os.environ)
    env.pop('PYTHONPATH', None)
    subprocess.run([str(python), '-I', 'test_native.py'], check=True, cwd=root, env=env)
