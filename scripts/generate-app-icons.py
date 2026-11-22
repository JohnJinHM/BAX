"""Generate application icon sizes from the approved raster master (requires Pillow)."""
from pathlib import Path

from PIL import Image

icons = Path(__file__).resolve().parents[1] / 'src-tauri' / 'icons'
with Image.open(icons / 'icon.png') as source:
    master = source.convert('RGBA')
for name, size in [('32x32.png', 32), ('64x64.png', 64),
                   ('128x128.png', 128), ('128x128@2x.png', 256)]:
    master.resize((size, size), Image.Resampling.LANCZOS).save(icons / name)
master.save(icons / 'icon.png')
master.save(icons / 'icon.ico', sizes=[(n, n) for n in (16, 20, 24, 32, 40, 48, 64, 128, 256)])
