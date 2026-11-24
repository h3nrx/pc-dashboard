"""Hands for Jarvis - the things he is allowed to do on this PC.

Everything here runs through Windows' own user32 API (no pip install needed) and
only ever does one of the named actions below. There is deliberately no
"run any command" tool: the model picks from this list, it cannot invent new
powers, and nothing here deletes or overwrites files.

Add a tool:  write the function, then describe it in TOOLS at the bottom.
"""
import ctypes
import difflib
import glob
import os
import subprocess
import time
import webbrowser
from ctypes import wintypes

import config
import texts

# What the tools report back - the model reads it and the dashboard shows it,
# so it follows the language chosen in setup.
MSG = texts.pick(texts.TOOL, config.LANG)


def t(key, **kw):
    return MSG[key].format(**kw)


user32 = ctypes.WinDLL("user32", use_last_error=True)
try:    # so cursor coordinates match what you see on a scaled display
    ctypes.WinDLL("shcore").SetProcessDpiAwareness(2)
except Exception:
    pass

NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)


# ------------------------------------------------------------ input plumbing
class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wintypes.LONG), ("dy", wintypes.LONG),
                ("mouseData", wintypes.DWORD), ("dwFlags", wintypes.DWORD),
                ("time", wintypes.DWORD), ("extra", ctypes.POINTER(ctypes.c_ulong))]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wintypes.WORD), ("wScan", wintypes.WORD),
                ("dwFlags", wintypes.DWORD), ("time", wintypes.DWORD),
                ("extra", ctypes.POINTER(ctypes.c_ulong))]


class _UNION(ctypes.Union):
    _fields_ = [("mi", MOUSEINPUT), ("ki", KEYBDINPUT)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wintypes.DWORD), ("u", _UNION)]


KEYUP, UNICODE = 0x0002, 0x0004
M_LEFT = (0x0002, 0x0004)
M_RIGHT = (0x0008, 0x0010)
M_MIDDLE = (0x0020, 0x0040)
M_WHEEL = 0x0800

VK = {
    "ctrl": 0x11, "control": 0x11, "strg": 0x11, "alt": 0x12, "shift": 0x10,
    "win": 0x5B, "windows": 0x5B,
    "enter": 0x0D, "return": 0x0D, "tab": 0x09, "esc": 0x1B, "escape": 0x1B,
    "space": 0x20, "leertaste": 0x20, "backspace": 0x08, "delete": 0x2E, "entf": 0x2E,
    "up": 0x26, "down": 0x28, "left": 0x25, "right": 0x27,
    "home": 0x24, "end": 0x23, "pageup": 0x21, "pagedown": 0x22,
    "volup": 0xAF, "voldown": 0xAE, "mute": 0xAD,
    "playpause": 0xB3, "next": 0xB0, "prev": 0xB1, "stop": 0xB2,
    "printscreen": 0x2C,
}
for _i in range(10):
    VK[str(_i)] = 0x30 + _i
for _i in range(26):
    VK[chr(97 + _i)] = 0x41 + _i
for _i in range(1, 13):
    VK[f"f{_i}"] = 0x6F + _i


def _send(*inputs):
    arr = (INPUT * len(inputs))(*inputs)
    user32.SendInput(len(inputs), ctypes.byref(arr), ctypes.sizeof(INPUT))


def _key(vk, up=False):
    return INPUT(type=1, u=_UNION(ki=KEYBDINPUT(wVk=vk, dwFlags=KEYUP if up else 0)))


def _char(ch, up=False):
    return INPUT(type=1, u=_UNION(ki=KEYBDINPUT(
        wScan=ord(ch), dwFlags=UNICODE | (KEYUP if up else 0))))


def _tap(vk, times=1):
    for _ in range(max(1, min(times, 50))):
        _send(_key(vk), _key(vk, True))
        time.sleep(0.03)


# ------------------------------------------------------------------- screen
def screen_info():
    """Resolution and where the cursor sits - the model needs this to aim."""
    pt = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(pt))
    w, h = user32.GetSystemMetrics(0), user32.GetSystemMetrics(1)
    return {"ok": True, "result": t("screen", w=w, h=h, x=pt.x, y=pt.y),
            "width": w, "height": h, "x": pt.x, "y": pt.y}


# -------------------------------------------------------------------- mouse
def mouse_move(x=None, y=None, dx=0, dy=0):
    """Absolute position with x/y, or a nudge from here with dx/dy."""
    pt = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(pt))
    tx = pt.x + int(dx or 0) if x is None else int(x)
    ty = pt.y + int(dy or 0) if y is None else int(y)
    tx = max(0, min(tx, user32.GetSystemMetrics(0) - 1))
    ty = max(0, min(ty, user32.GetSystemMetrics(1) - 1))
    user32.SetCursorPos(tx, ty)
    return {"ok": True, "result": t("moved", x=tx, y=ty)}


def mouse_click(button="left", double=False):
    down, up = {"left": M_LEFT, "right": M_RIGHT, "middle": M_MIDDLE}.get(button, M_LEFT)
    mk = lambda f: INPUT(type=0, u=_UNION(mi=MOUSEINPUT(dwFlags=f)))
    for _ in range(2 if double else 1):
        _send(mk(down), mk(up))
        time.sleep(0.05)
    return {"ok": True, "result": t("dclick" if double else "click", b=button)}


def mouse_scroll(amount=3):
    notches = max(-20, min(int(amount), 20))
    _send(INPUT(type=0, u=_UNION(mi=MOUSEINPUT(mouseData=notches * 120, dwFlags=M_WHEEL))))
    return {"ok": True, "result": t("scrolled", n=notches)}


# ----------------------------------------------------------------- keyboard
def type_text(text=""):
    text = str(text)[:800]
    for ch in text:
        _send(_char(ch), _char(ch, True))
        time.sleep(0.004)
    return {"ok": True, "result": t("typed", n=len(text))}


def press_keys(keys=""):
    """A shortcut like 'win+e', 'alt+tab', 'ctrl+shift+esc' or just 'enter'."""
    parts = [p.strip().lower() for p in str(keys).replace(" ", "+").split("+") if p.strip()]
    codes = [VK[p] for p in parts if p in VK]
    if not codes:
        return {"ok": False, "result": t("nokey", k=keys)}
    _send(*[_key(c) for c in codes], *[_key(c, True) for c in reversed(codes)])
    return {"ok": True, "result": t("pressed", k="+".join(parts))}


def volume(action="up", steps=4):
    vk = {"up": 0xAF, "down": 0xAE, "mute": 0xAD}.get(action, 0xAF)
    _tap(vk, 1 if action == "mute" else int(steps or 4))
    return {"ok": True,
            "result": t("vol_" + (action if action in ("up", "down", "mute") else "up"))}


def media(action="playpause"):
    vk = {"playpause": 0xB3, "next": 0xB0, "prev": 0xB1, "stop": 0xB2}.get(action, 0xB3)
    _tap(vk)
    return {"ok": True, "result": t("media", a=action)}


# --------------------------------------------------------------------- apps
BUILTIN = {
    "einstellungen": "ms-settings:", "settings": "ms-settings:",
    "taskmanager": "taskmgr", "task manager": "taskmgr",
    "rechner": "calc", "calculator": "calc",
    "explorer": "explorer", "dateien": "explorer", "papierkorb": "explorer shell:RecycleBinFolder",
    "editor": "notepad", "notepad": "notepad", "notizblock": "notepad",
    "paint": "mspaint", "systemsteuerung": "control", "control panel": "control",
    "bluetooth": "ms-settings:bluetooth", "wlan": "ms-settings:network-wifi",
    "sound": "ms-settings:sound", "lautstaerke": "ms-settings:sound",
    "kamera": "microsoft.windows.camera:", "terminal": "wt",
}
_index, _index_t = {}, 0.0


def app_index():
    """Name -> .lnk path for everything in both Start Menus, refreshed now and then."""
    global _index, _index_t
    if _index and time.time() - _index_t < 120:
        return _index
    found = {}
    for root in (os.path.join(os.environ.get("ProgramData", ""), "Microsoft", "Windows",
                              "Start Menu", "Programs"),
                 os.path.join(os.environ.get("APPDATA", ""), "Microsoft", "Windows",
                              "Start Menu", "Programs")):
        for p in glob.glob(os.path.join(root, "**", "*.lnk"), recursive=True):
            found.setdefault(os.path.splitext(os.path.basename(p))[0].lower(), p)
    _index, _index_t = found, time.time()
    return found


def list_apps(query=""):
    """What is installed - lets the model check before it guesses a name."""
    names = sorted(app_index())
    q = str(query or "").lower().strip()
    if q:
        names = [n for n in names if q in n] or difflib.get_close_matches(q, names, 8, 0.5)
    return {"ok": True, "result": ", ".join(names[:40]) or t("nothing"),
            "count": len(names)}


def open_app(name=""):
    q = str(name or "").lower().strip()
    if not q:
        return {"ok": False, "result": t("noapp")}
    for key, cmd in BUILTIN.items():
        if q == key or q in key:
            subprocess.Popen(cmd, shell=True, creationflags=NO_WINDOW)
            return {"ok": True, "result": t("opened", n=name)}
    idx = app_index()
    hit = (idx.get(q)
           or next((p for n, p in sorted(idx.items()) if n.startswith(q)), None)
           or next((p for n, p in sorted(idx.items()) if q in n), None))
    if not hit:
        close = difflib.get_close_matches(q, list(idx), 3, 0.55)
        hit = idx[close[0]] if close else None
        if not hit:
            return {"ok": False, "result": t("notinst", n=name)}
    os.startfile(hit)
    return {"ok": True, "result": t("started", n=os.path.splitext(os.path.basename(hit))[0])}


def open_website(url=""):
    u = str(url or "").strip()
    if not u:
        return {"ok": False, "result": t("nourl")}
    if not u.startswith(("http://", "https://")):
        u = "https://" + u
    webbrowser.open(u)
    return {"ok": True, "result": t("opened", n=u)}


# ------------------------------------------------------------------ windows
def _windows():
    out = []
    cb = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

    def each(hwnd, _):
        if user32.IsWindowVisible(hwnd):
            n = user32.GetWindowTextLengthW(hwnd)
            if n:
                buf = ctypes.create_unicode_buffer(n + 1)
                user32.GetWindowTextW(hwnd, buf, n + 1)
                out.append((hwnd, buf.value))
        return True

    user32.EnumWindows(cb(each), 0)
    return out


def list_windows():
    titles = [wt for _, wt in _windows()]
    return {"ok": True, "result": " | ".join(titles[:20]) or t("nowin")}


def focus_window(title=""):
    q = str(title or "").lower().strip()
    for hwnd, wt in _windows():             # not t - that is the translator
        if q and q in wt.lower():
            user32.ShowWindow(hwnd, 9)          # restore if minimised
            user32.SetForegroundWindow(hwnd)
            return {"ok": True, "result": t("focused", t=wt)}
    return {"ok": False, "result": t("nosuchwin", t=title)}


def close_window(title=""):
    """Asks the window to close (WM_CLOSE) - it may still prompt to save."""
    if title:
        r = focus_window(title)
        if not r["ok"]:
            return r
        time.sleep(0.25)
    hwnd = user32.GetForegroundWindow()
    user32.PostMessageW(hwnd, 0x0010, 0, 0)
    return {"ok": True, "result": t("closed")}


def minimize_all():
    press_keys("win+d")
    return {"ok": True, "result": t("desktop")}


def lock_pc():
    ctypes.WinDLL("user32").LockWorkStation()
    return {"ok": True, "result": t("locked")}


# -------------------------------------------------------------- the catalog
def _t(name, desc, props=None, req=None):
    return {"type": "function", "function": {
        "name": name, "description": desc,
        "parameters": {"type": "object", "properties": props or {}, "required": req or []}}}


NUM = {"type": "number"}
STR = {"type": "string"}

# Descriptions stay English whatever the spoken language: the model reads them,
# the user never sees them, and one wording then serves both.
TOOLS = [
    _t("screen_info", "Screen resolution and current mouse position. Always call this "
                      "before moving the mouse to an absolute position."),
    _t("mouse_move", "Move the mouse pointer: absolutely with x and y, or relative to "
                     "where it is now with dx and dy.",
       {"x": NUM, "y": NUM, "dx": NUM, "dy": NUM}),
    _t("mouse_click", "Click at the current pointer position.",
       {"button": {"type": "string", "enum": ["left", "right", "middle"]},
        "double": {"type": "boolean"}}),
    _t("mouse_scroll", "Turn the scroll wheel. Positive scrolls up, negative down.",
       {"amount": NUM}),
    _t("type_text", "Type text as if it came from the keyboard.", {"text": STR}, ["text"]),
    _t("press_keys", "Press a key or shortcut, e.g. 'enter', 'alt+tab', 'win+e', "
                     "'ctrl+shift+esc'.", {"keys": STR}, ["keys"]),
    _t("volume", "Change the system volume.",
       {"action": {"type": "string", "enum": ["up", "down", "mute"]}, "steps": NUM}),
    _t("media", "Control playback - works for Spotify, YouTube and anything else.",
       {"action": {"type": "string", "enum": ["playpause", "next", "prev", "stop"]}}),
    _t("open_app", "Start an installed program, e.g. 'Discord', 'Chrome', 'Settings', "
                   "'Task Manager'.", {"name": STR}, ["name"]),
    _t("list_apps", "See which programs are installed. Use this when a name was not "
                    "found instead of guessing.", {"query": STR}),
    _t("open_website", "Open a website in the default browser.", {"url": STR}, ["url"]),
    _t("list_windows", "Titles of all open windows."),
    _t("focus_window", "Bring an open window to the front (part of the title is enough).",
       {"title": STR}, ["title"]),
    _t("close_window", "Close a window. Without a title, the active one.", {"title": STR}),
    _t("minimize_all", "Minimise every window and show the desktop."),
    _t("lock_pc", "Lock the PC. Only when clearly asked for."),
]

RUN = {
    "screen_info": screen_info, "mouse_move": mouse_move, "mouse_click": mouse_click,
    "mouse_scroll": mouse_scroll, "type_text": type_text, "press_keys": press_keys,
    "volume": volume, "media": media, "open_app": open_app, "list_apps": list_apps,
    "open_website": open_website, "list_windows": list_windows,
    "focus_window": focus_window, "close_window": close_window,
    "minimize_all": minimize_all, "lock_pc": lock_pc,
}


def run(name, args):
    """Call one tool by name. Never raises - the model gets the error as text."""
    fn = RUN.get(name)
    if not fn:
        return {"ok": False, "result": t("notool", n=name)}
    try:
        return fn(**(args or {}))
    except TypeError as e:
        return {"ok": False, "result": t("badargs", n=name, e=e)}
    except Exception as e:
        return {"ok": False, "result": t("failed", n=name, e=e)}
