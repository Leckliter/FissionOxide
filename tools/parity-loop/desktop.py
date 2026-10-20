"""Desktop control for the UI probe: screenshots, mouse and keyboard on Windows, served to the agent
as an in-process MCP server named `desktop`.

It moves the real mouse and types into the real windows, so the driver only uses it when the user
passes --ui, and runs one UI probe at a time. Needs Pillow (screenshots).
"""

import asyncio
import base64
import ctypes
import io
import time
from ctypes import wintypes

from claude_agent_sdk import create_sdk_mcp_server, tool

user32 = ctypes.windll.user32
try:
  ctypes.windll.shcore.SetProcessDpiAwareness(2)  # physical pixels, so screenshots and clicks agree
except Exception:
  user32.SetProcessDPIAware()

MAX_WIDTH = 1400
_scale = {"f": 1.0}

INPUT_MOUSE, INPUT_KEYBOARD = 0, 1
KEYEVENTF_KEYUP, KEYEVENTF_UNICODE = 0x2, 0x4
MOUSE = {"left": (0x2, 0x4), "right": (0x8, 0x10), "middle": (0x20, 0x40)}
MOUSEEVENTF_WHEEL = 0x800


class KEYBDINPUT(ctypes.Structure):
  _fields_ = [("wVk", wintypes.WORD), ("wScan", wintypes.WORD), ("dwFlags", wintypes.DWORD),
              ("time", wintypes.DWORD), ("dwExtraInfo", ctypes.POINTER(ctypes.c_ulong))]


class MOUSEINPUT(ctypes.Structure):
  _fields_ = [("dx", wintypes.LONG), ("dy", wintypes.LONG), ("mouseData", wintypes.DWORD),
              ("dwFlags", wintypes.DWORD), ("time", wintypes.DWORD), ("dwExtraInfo", ctypes.POINTER(ctypes.c_ulong))]


class _U(ctypes.Union):
  _fields_ = [("ki", KEYBDINPUT), ("mi", MOUSEINPUT), ("pad", ctypes.c_byte * 32)]


class INPUT(ctypes.Structure):
  _fields_ = [("type", wintypes.DWORD), ("u", _U)]


def _send(*inputs):
  arr = (INPUT * len(inputs))(*inputs)
  user32.SendInput(len(inputs), arr, ctypes.sizeof(INPUT))


def _key(vk=0, scan=0, flags=0):
  i = INPUT(type=INPUT_KEYBOARD)
  i.u.ki = KEYBDINPUT(vk, scan, flags, 0, None)
  return i


def _mouse(flags, data=0):
  i = INPUT(type=INPUT_MOUSE)
  i.u.mi = MOUSEINPUT(0, 0, data, flags, 0, None)
  return i


VK = {"enter": 0x0D, "return": 0x0D, "escape": 0x1B, "esc": 0x1B, "tab": 0x09, "backspace": 0x08,
      "delete": 0x2E, "space": 0x20, "up": 0x26, "down": 0x28, "left": 0x25, "right": 0x27, "home": 0x24,
      "end": 0x23, "pageup": 0x21, "pagedown": 0x22, "ctrl": 0x11, "control": 0x11, "shift": 0x10,
      "alt": 0x12, "win": 0x5B, **{f"f{n}": 0x6F + n for n in range(1, 13)}}


def _vk(name):
  name = name.lower()
  if name in VK:
    return VK[name]
  if len(name) == 1:
    return user32.VkKeyScanW(ord(name)) & 0xFF
  raise ValueError(f"unknown key {name!r}")


def _point(x, y):
  f = _scale["f"]
  return int(round(x / f)), int(round(y / f))


def _text(s):
  return {"content": [{"type": "text", "text": s}]}


@tool("screenshot", "Capture the primary screen. Coordinates for the other tools are in this image's pixels.",
      {"type": "object", "properties": {}})
async def screenshot(args):
  from PIL import ImageGrab

  await asyncio.sleep(0.3)
  img = ImageGrab.grab()
  f = min(1.0, MAX_WIDTH / img.width)
  _scale["f"] = f
  if f < 1.0:
    img = img.resize((int(img.width * f), int(img.height * f)))
  buf = io.BytesIO()
  img.save(buf, "PNG")
  return {"content": [{"type": "image", "data": base64.b64encode(buf.getvalue()).decode(), "mimeType": "image/png"},
                      {"type": "text", "text": f"{img.width}x{img.height}"}]}


@tool("click", "Click at (x, y) in screenshot pixels. button: left|right|middle. count: 1 or 2.",
      {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"},
                                        "button": {"type": "string"}, "count": {"type": "integer"}},
       "required": ["x", "y"]})
async def click(args):
  user32.SetCursorPos(*_point(args["x"], args["y"]))
  down, up = MOUSE[args.get("button", "left")]
  for _ in range(int(args.get("count", 1))):
    _send(_mouse(down), _mouse(up))
    time.sleep(0.05)
  return _text("clicked")


@tool("drag", "Press the left button at (x1, y1), move to (x2, y2) and release.",
      {"type": "object", "properties": {k: {"type": "number"} for k in ("x1", "y1", "x2", "y2")},
       "required": ["x1", "y1", "x2", "y2"]})
async def drag(args):
  user32.SetCursorPos(*_point(args["x1"], args["y1"]))
  _send(_mouse(MOUSE["left"][0]))
  x1, y1 = _point(args["x1"], args["y1"])
  x2, y2 = _point(args["x2"], args["y2"])
  for i in range(1, 21):
    user32.SetCursorPos(x1 + (x2 - x1) * i // 20, y1 + (y2 - y1) * i // 20)
    time.sleep(0.01)
  _send(_mouse(MOUSE["left"][1]))
  return _text("dragged")


@tool("move", "Move the mouse to (x, y) without clicking (hover).",
      {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}}, "required": ["x", "y"]})
async def move(args):
  user32.SetCursorPos(*_point(args["x"], args["y"]))
  return _text("moved")


@tool("scroll", "Scroll the wheel at (x, y); clicks > 0 scrolls up, < 0 down.",
      {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"},
                                        "clicks": {"type": "integer"}}, "required": ["x", "y", "clicks"]})
async def scroll(args):
  user32.SetCursorPos(*_point(args["x"], args["y"]))
  _send(_mouse(MOUSEEVENTF_WHEEL, ctypes.c_ulong(120 * int(args["clicks"])).value))
  return _text("scrolled")


@tool("type", "Type text into the focused window.",
      {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]})
async def type_text(args):
  for ch in args["text"]:
    _send(_key(scan=ord(ch), flags=KEYEVENTF_UNICODE), _key(scan=ord(ch), flags=KEYEVENTF_UNICODE | KEYEVENTF_KEYUP))
    time.sleep(0.01)
  return _text("typed")


@tool("key", "Press a key or chord, e.g. 'escape', 'enter', 'ctrl+z', 'shift+s', 'e'.",
      {"type": "object", "properties": {"keys": {"type": "string"}}, "required": ["keys"]})
async def key(args):
  vks = [_vk(k) for k in args["keys"].split("+")]
  _send(*[_key(vk) for vk in vks])
  _send(*[_key(vk, flags=KEYEVENTF_KEYUP) for vk in reversed(vks)])
  return _text("pressed " + args["keys"])


@tool("focus_window", "Bring the first top-level window whose title contains `title` to the front.",
      {"type": "object", "properties": {"title": {"type": "string"}}, "required": ["title"]})
async def focus_window(args):
  want = args["title"].lower()
  found = []

  @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
  def each(hwnd, _):
    if user32.IsWindowVisible(hwnd):
      n = user32.GetWindowTextLengthW(hwnd)
      buf = ctypes.create_unicode_buffer(n + 1)
      user32.GetWindowTextW(hwnd, buf, n + 1)
      if want in buf.value.lower():
        found.append((hwnd, buf.value))
    return True

  user32.EnumWindows(each, 0)
  if not found:
    return _text(f"no window titled like {args['title']!r}")
  hwnd, title = found[0]
  user32.ShowWindow(hwnd, 9)  # SW_RESTORE
  _send(_key(VK["alt"]), _key(VK["alt"], flags=KEYEVENTF_KEYUP))  # lets SetForegroundWindow work
  user32.SetForegroundWindow(hwnd)
  return _text(f"focused {title!r}")


@tool("wait", "Wait for `seconds` (at most 10).",
      {"type": "object", "properties": {"seconds": {"type": "number"}}, "required": ["seconds"]})
async def wait(args):
  await asyncio.sleep(min(10.0, float(args["seconds"])))
  return _text("waited")


def server():
  return create_sdk_mcp_server("desktop", tools=[screenshot, click, drag, move, scroll, type_text, key,
                                                 focus_window, wait])
