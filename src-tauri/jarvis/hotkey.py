"""The talk button - a global mouse hook so it works in every program.

The browser only sees mouse buttons while the dashboard has focus, which is no
good for a push-to-talk button you want to use while gaming or browsing. So we
hook the mouse low-level through Windows itself (no extra software) and count
the presses; the dashboard waits on them through monitor.py.

Which button:  config.TALK_BUTTON - "x2" is the upper thumb button, "x1" the
lower one, "middle" the wheel. config.TALK_SWALLOW decides whether the press is
also passed on to whatever program is in front; it is off by default, so the
button no longer does its usual back/forward job anywhere.
"""
import ctypes
import threading
import time
from ctypes import wintypes

import config

user32 = ctypes.WinDLL("user32", use_last_error=True)

WH_MOUSE_LL = 14
WM_MBUTTONDOWN, WM_MBUTTONUP = 0x0207, 0x0208
WM_XBUTTONDOWN, WM_XBUTTONUP = 0x020B, 0x020C
XBUTTON = {"x1": 1, "x2": 2}

presses = 0            # bumped on every press of the talk button
events = 0             # bumped on every press AND every release
down = False           # is the button held down right now?
hooked = False         # False means the hook could not be installed
_wake = threading.Event()


class MSLLHOOKSTRUCT(ctypes.Structure):
    _fields_ = [("pt", wintypes.POINT), ("mouseData", wintypes.DWORD),
                ("flags", wintypes.DWORD), ("time", wintypes.DWORD),
                ("extra", ctypes.POINTER(ctypes.c_ulong))]


HOOKPROC = ctypes.WINFUNCTYPE(ctypes.c_ssize_t, ctypes.c_int,
                              wintypes.WPARAM, wintypes.LPARAM)

# without these the 64-bit handles get truncated and the hook silently dies
user32.SetWindowsHookExW.restype = wintypes.HHOOK
user32.SetWindowsHookExW.argtypes = [ctypes.c_int, HOOKPROC,
                                     wintypes.HINSTANCE, wintypes.DWORD]
user32.CallNextHookEx.restype = ctypes.c_ssize_t
user32.CallNextHookEx.argtypes = [wintypes.HHOOK, ctypes.c_int,
                                  wintypes.WPARAM, wintypes.LPARAM]


def _want():
    return str(getattr(config, "TALK_BUTTON", "x2")).lower()


def _is_talk(msg, data):
    """Is this message our button going down (True) or up (False)? Else None."""
    want = _want()
    if want == "middle":
        if msg == WM_MBUTTONDOWN:
            return True
        if msg == WM_MBUTTONUP:
            return False
        return None
    if msg in (WM_XBUTTONDOWN, WM_XBUTTONUP):
        if ((data >> 16) & 0xFFFF) == XBUTTON.get(want, 2):
            return msg == WM_XBUTTONDOWN
    return None


def _proc(code, wparam, lparam):
    if code == 0:
        info = ctypes.cast(lparam, ctypes.POINTER(MSLLHOOKSTRUCT)).contents
        edge = _is_talk(wparam, info.mouseData)
        if edge is not None:
            global presses, events, down
            if edge and not down:         # ignore the key repeat of a long hold
                presses += 1
            down = edge
            events += 1
            _wake.set()
            if getattr(config, "TALK_SWALLOW", True):
                return 1          # do not pass it on - this is Jarvis' button now
    return user32.CallNextHookEx(None, code, wparam, lparam)


_cb = HOOKPROC(_proc)             # must stay referenced or Windows calls into freed memory


def _loop():
    global hooked
    handle = user32.SetWindowsHookExW(WH_MOUSE_LL, _cb, None, 0)
    if not handle:
        hooked = False
        return
    hooked = True
    msg = wintypes.MSG()
    while user32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
        user32.TranslateMessage(ctypes.byref(msg))
        user32.DispatchMessageW(ctypes.byref(msg))


def start():
    threading.Thread(target=_loop, daemon=True).start()
    time.sleep(0.2)               # give it a moment so hooked is meaningful
    return hooked


def wait(since, timeout=25.0):
    """Block until the button is pressed or released, or the timeout runs out.

    The dashboard hangs on this instead of polling, so an edge reaches it at
    once and an idle minute costs nothing.
    """
    end = time.time() + timeout
    while events == since and time.time() < end:
        _wake.wait(0.25)
        _wake.clear()
    return events


if __name__ == "__main__":
    print("Taste:", _want(), "| schluckt den Klick:", getattr(config, "TALK_SWALLOW", True))
    print("Hook aktiv:", start())
    print("Halte die Taste - Strg+C beendet.")
    seen = events
    while True:
        seen = wait(seen, 60)
        print(f"  {'gedrueckt' if down else 'losgelassen':<12} "
              f"(Ereignis {seen}) um {time.strftime('%H:%M:%S')}")
