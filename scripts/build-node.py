"""Build the native Node addon with Cargo; no install-time download or compilation."""
from pathlib import Path
import shutil
import json
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
target = subprocess.check_output(['rustc', '-vV'], text=True).split('host: ')[1].splitlines()[0]
subprocess.run(['cargo', 'build', '--locked', '--release', '-p', 'inference-engine-node'], cwd=ROOT, check=True)
name = ('inference_engine_node.dll' if 'windows' in target else
        'libinference_engine_node.dylib' if 'apple' in target else 'libinference_engine_node.so')
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=ROOT, text=True))
shutil.copyfile(Path(metadata['target_directory']) / 'release' / name, ROOT / f'inference-engine.{target}.node')
