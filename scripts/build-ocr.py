"""Build the Windows x64 OCR runtime. Run with the dedicated build venv."""
from pathlib import Path
import hashlib
import importlib.metadata as metadata
import json
import shutil
import subprocess
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
LOCAL = ROOT / '.local'
DIST = LOCAL / 'ocr-dist' / 'bax-ocr'
CACHE = LOCAL / 'ocr-models'
CACHE.mkdir(parents=True, exist_ok=True)
models = json.loads((ROOT / 'ocr/models.json').read_text(encoding='utf-8'))
for model in models:
    target = CACHE / model['file']
    if not target.is_file() or hashlib.sha256(target.read_bytes()).hexdigest() != model['sha256']:
        urllib.request.urlretrieve(model['url'], target)
    if hashlib.sha256(target.read_bytes()).hexdigest() != model['sha256']:
        raise RuntimeError('Model checksum mismatch: ' + model['file'])

subprocess.run([sys.executable, '-m', 'PyInstaller', '--noconfirm', '--clean', '--onedir', '--console',
    '--noupx', '--name', 'bax-ocr', '--distpath', str(LOCAL / 'ocr-dist'),
    '--workpath', str(LOCAL / 'ocr-pyinstaller'), '--specpath', str(LOCAL),
    '--collect-all', 'rapidocr', '--collect-all', 'onnxruntime',
    '--exclude-module', 'tkinter', '--exclude-module', 'matplotlib',
    str(ROOT / 'ocr/worker.py')], check=True, cwd=ROOT)
shutil.copytree(CACHE, DIST / 'models', dirs_exist_ok=True)
# Explicit model paths above the runtime replace RapidOCR's bundled defaults.
for model in models:
    duplicate = DIST / '_internal' / 'rapidocr' / 'models' / model['file']
    if duplicate.is_file():
        duplicate.unlink()
licenses = DIST / 'licenses'
licenses.mkdir(exist_ok=True)
packages = []
for dist in metadata.distributions():
    name = dist.metadata['Name']
    packages.append({'name': name, 'version': dist.version})
    for file in dist.files or []:
        if any(part.lower().startswith(('license', 'copying', 'notice')) for part in file.parts):
            source = Path(dist.locate_file(file))
            if source.is_file():
                destination = licenses / name / str(file).replace('../', '_parent_/')
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, destination)
python_license = Path(sys.base_prefix) / 'LICENSE.txt'
if python_license.is_file():
    shutil.copy2(python_license, licenses / 'Python-LICENSE.txt')
else:
    tag = '.'.join(map(str, sys.version_info[:3]))
    urllib.request.urlretrieve(f'https://raw.githubusercontent.com/python/cpython/v{tag}/LICENSE', licenses / 'Python-LICENSE.txt')
for project, url in {
    'PaddleOCR': 'https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/main/LICENSE',
}.items():
    urllib.request.urlretrieve(url, licenses / (project + '.txt'))
(DIST / 'manifest.json').write_text(json.dumps({'python':sys.version, 'packages':packages,'models':models},indent=2),encoding='utf8')
# The worker blocks network connections itself. Validate the packaged executable, not Python.
completed = subprocess.run([str(DIST / 'bax-ocr.exe'), str(DIST / 'models'), '--self-test'],
    capture_output=True, text=True, timeout=60, creationflags=0x08000000)
if completed.returncode != 0 or not any(json.loads(line).get('ready') is True for line in completed.stdout.splitlines() if line.startswith('{')):
    raise RuntimeError('Packaged OCR self-test failed: ' + completed.stderr[-3000:] + completed.stdout[-1000:])
size = sum(p.stat().st_size for p in DIST.rglob('*') if p.is_file())
print(json.dumps({'runtime':str(DIST), 'bytes':size, 'modelsBytes':sum((DIST/'models'/m['file']).stat().st_size for m in models)}))
