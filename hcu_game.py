"""Microsoft Hybrid Computer Use adapter for the Blue Archive GUI.

All desktop observations and input go through the official HCU Python SDK and
its paired native CLI/provider. Windows UIA is empty for game cards, so HCU GUI
coordinates are derived from the current screenshot's client geometry. Zoom
captures both halves separately for readable OCR at 1280x960.
"""
from __future__ import annotations

import os
from pathlib import Path

from PIL import Image
import win32api
import win32con
import win32gui

import hcu_sdk as hcu


class HcuGame:
    def __init__(self, binary: Path):
        if not binary.is_file() or not binary.with_name("computer-use-windows-provider.exe").is_file():
            raise RuntimeError("HCU Windows CLI/provider missing; build vendor/hcu first")
        os.environ["HCU_BIN"] = str(binary.resolve())
        self.desktop = hcu.connect()
        windows = [w for w in self.desktop.apps().windows if w.title == "Blue Archive"]
        if len(windows) != 1:
            raise RuntimeError(f"Expected one Blue Archive HCU window, found {len(windows)}")
        self.window_ref = windows[0].ref
        self.hwnd = win32gui.FindWindow(None, "Blue Archive")
        if not self.hwnd:
            raise RuntimeError("Blue Archive Win32 client geometry unavailable")
        self.frame = None
        self.geometry = None

    def activate(self) -> None:
        self.desktop.activate(self.window_ref)

    def _client_frame_box(self, frame):
        left, top = win32gui.ClientToScreen(self.hwnd, (0, 0))
        _, _, width, height = win32gui.GetClientRect(self.hwnd)
        vx = win32api.GetSystemMetrics(win32con.SM_XVIRTUALSCREEN)
        vy = win32api.GetSystemMetrics(win32con.SM_YVIRTUALSCREEN)
        vw = win32api.GetSystemMetrics(win32con.SM_CXVIRTUALSCREEN)
        vh = win32api.GetSystemMetrics(win32con.SM_CYVIRTUALSCREEN)
        sx, sy = frame.screen.width / vw, frame.screen.height / vh
        return ((left-vx)*sx, (top-vy)*sy, (left+width-vx)*sx,
                (top+height-vy)*sy, width, height)

    def capture(self) -> Image.Image:
        frame = self.desktop.screenshot()
        x0, y0, x1, y1, width, height = self._client_frame_box(frame)
        self.frame = frame
        self.geometry = (x0, y0, x1, y1, width, height)
        halves = []
        for a, b in ((y0, (y0+y1)/2), ((y0+y1)/2, y1)):
            zoom = self.desktop.zoom([int(x0)-2, int(a)-2, int(x1)+2, int(b)+2])
            region = zoom.evidence.zoom.region
            sx, sy = zoom.evidence.zoom.scale_x, zoom.evidence.zoom.scale_y
            with Image.open(zoom.images[0]) as image:
                rect = tuple(round(v) for v in ((x0-region[0])*sx, (a-region[1])*sy,
                                                 (x1-region[0])*sx, (b-region[1])*sy))
                halves.append(image.crop(rect).convert("RGB").resize((width, height//2)))
        result = Image.new("RGB", (width, height))
        result.paste(halves[0], (0, 0))
        result.paste(halves[1], (0, height//2))
        return result

    def _gui_point(self, x: float, y: float) -> tuple[float, float]:
        if self.frame is None or self.geometry is None:
            raise RuntimeError("Capture a fresh HCU frame before any input")
        x0, y0, x1, y1, width, height = self.geometry
        return x0 + (x1-x0)*x/1280, y0 + (y1-y0)*y/960

    def _input(self, action: dict) -> None:
        # HCU checks the current frame's window layout before dispatch. An
        # uncertain-effect exception is *not* retried by this adapter.
        result = self.desktop.gui([action], settle_ms=1100)
        if not result.input_emitted or result.completed != 1:
            raise RuntimeError(f"HCU did not confirm input dispatch: {result}")
        self.frame = None

    def click(self, x: int, y: int) -> None:
        px, py = self._gui_point(x, y)
        self._input({"type": "click", "x": px, "y": py})

    def scroll(self, delta: int = 420) -> None:
        px, py = self._gui_point(1170, 700)
        self._input({"type": "scroll", "x": px, "y": py, "scroll_y": delta})

    def back(self) -> None:
        self.click(57, 36)
