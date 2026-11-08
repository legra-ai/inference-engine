"""Synchronize native binding metadata before the shared Cargo version writer."""
import json
from pathlib import Path
import re
import runpy
import sys

root = Path(__file__).resolve().parents[1]
version = sys.argv[1]
runpy.run_path(str(root / 'scripts/release-version.py'))['python_version'](version)
manifest = root / 'Cargo.toml'
text, count = re.subn(r'(\[workspace.package\]\nversion = ")[^"]+("[\n])',
                      lambda match: match[1] + version + match[2], manifest.read_text())
if count != 1:
    raise ValueError('Expected one workspace version authority')
manifest.write_text(text)
for name in ['package.json', 'package-lock.json']:
    path = root / name
    data = json.loads(path.read_text())
    data['version'] = version
    if name == 'package-lock.json':
        data['packages']['']['version'] = version
    path.write_text(json.dumps(data, indent=2) + '\n')
