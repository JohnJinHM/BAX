"""BAX local OCR worker. JSON lines only; no screenshot or name logging."""
import contextlib
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import re
import sys
import time

import cv2
import numpy as np
from PIL import ImageGrab
from rapidocr import RapidOCR, LangRec, ModelType, OCRVersion

LEVEL = re.compile(r"[iIlL1d]?v[lI1i]\s*(\d{1,3})(?!\d)", re.IGNORECASE)


def extract(engine, image, russian=None):
    height, width = image.shape[:2]
    if width < 960 or height < 540:
        return []
    rows = []
    for side, left, right in [("left", 0.08, 0.25), ("right", 0.75, 0.92)]:
        strip = image[:, int(width * left):int(width * right)]
        result = engine(strip, use_det=True, use_cls=False)
        if result.txts is None or result.boxes is None:
            continue
        lines = []
        for text, score, box in zip(result.txts, result.scores, result.boxes):
            x1, y1 = box.min(axis=0)
            x2, y2 = box.max(axis=0)
            lines.append((text, float(score), float(x1), float(y1), float(x2), float(y2)))
        found = []
        for text, score, x1, y1, x2, y2 in lines:
            match = LEVEL.search(text)
            if not match or score < 0.80 or not 0 <= int(match[1]) <= 100:
                continue
            h = max(y2 - y1, 1)
            # The nickname is one line above the level, left-aligned with the card text.
            names = [line for line in lines if line[1] >= 0.85 and
                     0 < y1 - line[3] < h * 2.8 and line[5] <= y1 + h * 0.25 and
                     x1 - strip.shape[1] * 0.23 <= line[2] <= x1 + h * 0.5 and
                     not LEVEL.search(line[0]) and len(line[0]) <= 80]
            if not names:
                continue
            name = min(names, key=lambda line: abs(y1 - line[5]))
            if not name[0].strip():
                continue
            alternatives = []
            if russian is not None and not re.search(r"[\u3040-\u30ff\u3400-\u9fff]", name[0]):
                crop = strip[max(0, int(name[3])-2):int(name[5])+2, max(0, int(name[2])-2):int(name[4])+2]
                alt = russian(crop, use_det=False, use_cls=False)
                if alt.txts and alt.scores[0] >= 0.8 and re.search(r"[\u0400-\u052f]", alt.txts[0]) and alt.txts[0] != name[0]:
                    alternatives.append(alt.txts[0])
            found.append((name[3], name[0].strip(), int(match[1]), alternatives))
        found.sort()
        # A different screen / duplicate detection must not masquerade as a roster.
        if len(found) > 5:
            return []
        for i, (_, name, level, alternatives) in enumerate(found):
            rows.append({"side": side, "row": i, "name": name, "level": level, "alternatives": alternatives})
    # This layout is a two-sided roster. One-sided menus are not accepted.
    if not any(r["side"] == "left" for r in rows) or not any(r["side"] == "right" for r in rows):
        return []
    return rows


def capture_game():
    if sys.platform != "win32":
        return None
    user = ctypes.WinDLL("user32", use_last_error=True)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    user.GetForegroundWindow.restype = wintypes.HWND
    user.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
    user.GetClientRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
    user.ClientToScreen.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.POINT)]
    user.IsIconic.argtypes = [wintypes.HWND]
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    hwnd = user.GetForegroundWindow()
    if not hwnd or user.IsIconic(hwnd):
        return None
    pid = wintypes.DWORD()
    user.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    handle = kernel.OpenProcess(0x1000, False, pid.value)
    if not handle:
        return None
    try:
        length = wintypes.DWORD(32768)
        path = ctypes.create_unicode_buffer(length.value)
        if not kernel.QueryFullProcessImageNameW(handle, 0, path, ctypes.byref(length)):
            return None
        if Path(path.value).name.lower() not in {"brokenarrow.exe", "broken_arrow.exe", "broken arrow.exe"}:
            return None
    finally:
        kernel.CloseHandle(handle)
    rect = wintypes.RECT()
    if not user.GetClientRect(hwnd, ctypes.byref(rect)):
        return None
    origin = wintypes.POINT(0, 0)
    if not user.ClientToScreen(hwnd, ctypes.byref(origin)):
        return None
    if rect.right < 960 or rect.bottom < 540:
        return None
    if user.GetForegroundWindow() != hwnd:
        return None
    image = ImageGrab.grab(bbox=(origin.x, origin.y, origin.x + rect.right, origin.y + rect.bottom), all_screens=True)
    if user.GetForegroundWindow() != hwnd:
        return None
    return cv2.cvtColor(np.asarray(image), cv2.COLOR_RGB2BGR)


def main():
    model_root = Path(sys.argv[1])
    if sys.platform == "win32":
        # Physical pixels across displays with different scaling.
        try:
            ctypes.windll.shcore.SetProcessDpiAwareness(2)
        except (AttributeError, OSError):
            pass
    with contextlib.redirect_stdout(sys.stderr):
        engine = RapidOCR(params={
            "Global.model_root_dir": str(model_root), "Global.log_level": "error",
            "EngineConfig.onnxruntime.intra_op_num_threads": 2,
            "EngineConfig.onnxruntime.inter_op_num_threads": 1,
        })
        # Load models and warm them once, before the ready response.
        engine(np.zeros((96, 320, 3), dtype=np.uint8), use_cls=False)
        engine(np.zeros((48, 160, 3), dtype=np.uint8), use_det=False, use_cls=False)
        russian = RapidOCR(params={
            "Global.model_root_dir": str(model_root), "Global.log_level": "error",
            "Rec.lang_type": LangRec.ESLAV, "Rec.model_type": ModelType.MOBILE,
            "Rec.ocr_version": OCRVersion.PPOCRV5,
            "EngineConfig.onnxruntime.intra_op_num_threads": 2,
            "EngineConfig.onnxruntime.inter_op_num_threads": 1,
        })
        russian(np.zeros((48, 160, 3), dtype=np.uint8), use_det=False, use_cls=False)
    print(json.dumps({"ready": True}), flush=True)
    for line in sys.stdin:
        try:
            json.loads(line)
            start = time.perf_counter()
            with contextlib.redirect_stdout(sys.stderr):
                image = capture_game()
                players = extract(engine, image, russian) if image is not None else []
            status = "等待游戏前台画面" if image is None else ("已识别卡片" if players else "等待双侧玩家卡片画面")
            print(json.dumps({"players": players, "status": status, "elapsedMs": round((time.perf_counter()-start)*1000)}, ensure_ascii=True), flush=True)
        except Exception:
            print(json.dumps({"players": [], "status": "画面识别失败，将重试"}), flush=True)


if __name__ == "__main__":
    main()
