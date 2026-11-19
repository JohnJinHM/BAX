"""Offline native OCR regression checks. Pillow is a test-only dependency."""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--runtime', type=Path, default=ROOT / '.local/ocr-dist/bax-ocr')
parser.add_argument('--screenshot', type=Path)
args = parser.parse_args()
runtime = args.runtime.resolve()
exe = runtime / 'bax-ocr.exe'
models = runtime / 'models'
output = ROOT / '.local/native-validation'
output.mkdir(parents=True, exist_ok=True)


def run(image, label, repeats=1):
    path = output / (label + '.png')
    image.save(path)
    result = subprocess.run([str(exe), str(models), '--image', str(path), str(repeats)],
                            capture_output=True, encoding='utf8', check=True, timeout=60)
    # Real observations stay under .local and never enter the public test report.
    (output / (label + '.jsonl')).write_text(result.stdout, encoding='utf8')
    data = [json.loads(line) for line in result.stdout.splitlines()]
    assert data[0]['ready'] is True
    return data[0], data[1:]


names = ['AlphaTest', 'ТестИгрок', 'テスト太郎', '星河玩家', 'Player_07',
         'Борис', 'サクラ', '银河舰队', 'Mixed123', 'T-T']
levels = [93, 57, 59, 57, 100, 73, 71, 93, 70, 18]
canvas = Image.new('RGB', (1920, 1080), (25, 30, 23))
draw = ImageDraw.Draw(canvas)
font = ImageFont.truetype('C:/Windows/Fonts/msyhbd.ttc', 18)
small = ImageFont.truetype('C:/Windows/Fonts/arialbd.ttf', 15)
for i, (name, level) in enumerate(zip(names, levels)):
    side, row = divmod(i, 5)
    x = round((.787 if side else .1201) * 1920)
    y = round((.2917 + row * .14255) * 1080)
    draw.text((x+1, y+1), name, font=font, fill=(230, 240, 225), anchor='lt')
    x = round((.8039 if side else .1375) * 1920)
    y = round((.3121 + row * .14255) * 1080)
    draw.text((x, y), 'lvl', font=small, fill=(240, 205, 30), anchor='lt')
    draw.text((x+20, y), str(level), font=small, fill=(235, 235, 220), anchor='lt')
_, results = run(canvas, 'synthetic-four-languages')
players = results[0]['players']
assert len(players) == 10, 'synthetic roster incomplete'
for i, player in enumerate(players):
    assert player['level'] == levels[i], 'incorrect synthetic level'
    assert names[i] in [player['name'], *player['alternatives']], 'expected name not in OCR hypotheses'
    assert (player['side'], player['row']) == ('right' if i >= 5 else 'left', i % 5)

blank = Image.new('RGB', (1920, 1080), (30, 30, 30))
one_side = canvas.copy()
ImageDraw.Draw(one_side).rectangle((960, 0, 1919, 1079), fill=(30, 30, 30))
no_label = canvas.copy()
draw = ImageDraw.Draw(no_label)
for i in range(10):
    side, row = divmod(i, 5)
    x = round((.8039 if side else .1375) * 1920)
    y = round((.3121 + row * .14255) * 1080)
    draw.rectangle((x, y, x+18, y+16), fill=(30, 30, 30))
for label, image in [('blank', blank), ('one-side', one_side), ('missing-label', no_label),
                     ('unsupported-ultrawide', canvas.resize((2560, 1080)))]:
    _, results = run(image, label)
    assert not results[0]['players'], 'non-roster must be rejected: ' + label
with tempfile.TemporaryDirectory() as empty:
    result = subprocess.run([str(exe), empty, '--self-test'], capture_output=True, encoding='utf8', timeout=10)
    assert result.returncode != 0 and json.loads(result.stdout)['ready'] is False

summary = {'synthetic': '10/10 names in hypotheses and levels correct', 'negativeCases': 5,
           'realScreenshots': []}
if args.screenshot:
    source = Image.open(args.screenshot).convert('RGB')
    for w, h in [source.size, (2560, 1440), (1920, 1080), (1280, 720)]:
        ready, runs = run(source.resize((w, h), Image.Resampling.LANCZOS), f'real-{w}', 11)
        assert all(r['players'] == runs[0]['players'] for r in runs), 'unstable repeated result'
        summary['realScreenshots'].append({'width': w, 'height': h, 'players': len(runs[0]['players']),
            'initializationMs': ready['initializationMs'], 'warmSamples': len(runs)-1,
            'warmMedianMs': round(statistics.median(r['elapsedMs'] for r in runs[1:]), 2),
            'warmMaxMs': round(max(r['elapsedMs'] for r in runs[1:]), 2)})
(output / 'report.json').write_text(json.dumps(summary, indent=2), encoding='utf8')
print(json.dumps(summary, indent=2))
