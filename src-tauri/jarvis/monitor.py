"""Mission Control - system stats server.

Serves live PC stats as JSON at http://localhost:8765/stats
and the dashboard itself at http://localhost:8765/

Needs:   pip install psutil
Optional: LibreHardwareMonitor with its Remote Web Server on (port 8085) for CPU temperature.
GPU:     NVIDIA cards are read through nvidia-smi (installed with the driver), no extra setup.
Problems? Run:  python monitor.py --debug
"""
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

try:
    import psutil           # optional: lets Jarvis answer questions about the PC
except ImportError:
    psutil = None

import config
import hotkey
import jarvis
import texts

PORT = 8765
LHM_URL = "http://localhost:8085/data.json"
HERE = os.path.dirname(os.path.abspath(__file__))
HTML_FILE = os.path.join(HERE, "dashboard.html")
NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)
CPU_COUNT = os.cpu_count() or 1
PAGES = ("http://tauri.localhost", "https://tauri.localhost", "tauri://localhost")


def cpu_name():
    """Real CPU model name (same text Task Manager shows), no extra software needed."""
    try:
        import winreg
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE,
                            r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") as k:
            return re.sub(r"\s+", " ", winreg.QueryValueEx(k, "ProcessorNameString")[0]).strip()
    except Exception:
        return platform.processor()


CPU_NAME = cpu_name()
cpu_ghz = None


def freq_worker():
    """Actual CPU clock like Task Manager shows it: base MHz x '% Processor Performance'.
    Reads Windows' built-in performance counters directly (no PowerShell, no extra software)."""
    global cpu_ghz
    try:
        import ctypes
        from ctypes import wintypes

        class Value(ctypes.Structure):
            _fields_ = [("status", wintypes.DWORD), ("value", ctypes.c_double)]

        pdh = ctypes.WinDLL("pdh")
        handle = ctypes.POINTER(ctypes.c_void_p)
        pdh.PdhOpenQueryW.argtypes = [wintypes.LPCWSTR, ctypes.c_size_t, handle]
        pdh.PdhAddEnglishCounterW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR, ctypes.c_size_t, handle]
        pdh.PdhCollectQueryData.argtypes = [ctypes.c_void_p]
        pdh.PdhGetFormattedCounterValue.argtypes = [ctypes.c_void_p, wintypes.DWORD, ctypes.c_void_p, ctypes.POINTER(Value)]
        q, freq, perf = ctypes.c_void_p(), ctypes.c_void_p(), ctypes.c_void_p()
        if pdh.PdhOpenQueryW(None, 0, ctypes.byref(q)):
            return
        base = r"\Processor Information(_Total)"
        if pdh.PdhAddEnglishCounterW(q, base + r"\Processor Frequency", 0, ctypes.byref(freq)) \
                or pdh.PdhAddEnglishCounterW(q, base + r"\% Processor Performance", 0, ctypes.byref(perf)):
            return
        pdh.PdhCollectQueryData(q)
        v = Value()
        while True:
            time.sleep(1)
            if pdh.PdhCollectQueryData(q):
                continue
            if pdh.PdhGetFormattedCounterValue(freq, 0x200, None, ctypes.byref(v)):
                continue
            mhz = v.value
            if pdh.PdhGetFormattedCounterValue(perf, 0x200, None, ctypes.byref(v)):
                continue
            cpu_ghz = mhz * v.value / 100 / 1000
    except Exception:
        pass


def current_ghz():
    if cpu_ghz:
        return round(cpu_ghz, 2)
    try:
        f = psutil.cpu_freq()
        return round(f.current / 1000, 2) if f and f.current else None
    except Exception:
        return None

stats = {
    "cpu": {"load": 0, "temp": None, "name": CPU_NAME, "ghz": None},
    "gpu": {"load": None, "temp": None, "name": None},
    "ram": {"used": 0, "total": 1, "percent": 0},
    "net": {"up": 0, "down": 0},
    "disks": [],
    "top": [],
    "uptime": 0,
}


def number(text):
    """'45,5 °C' or '45.5 °C' -> 45.5 (works with any Windows language)."""
    m = re.search(r"-?\d+(?:[.,]\d+)?", str(text or ""))
    return float(m.group().replace(",", ".")) if m else None


# ---------------------------------------------------------------- NVIDIA
def find_nvidia_smi():
    found = shutil.which("nvidia-smi")
    if found:
        return found
    for p in (r"C:\Windows\System32\nvidia-smi.exe",
              r"C:\Program Files\NVIDIA Corporation\NVSMI\nvidia-smi.exe"):
        if os.path.exists(p):
            return p
    return None


NVSMI = find_nvidia_smi()


def read_nvidia():
    if not NVSMI:
        return None
    try:
        out = subprocess.run(
            [NVSMI, "--query-gpu=name,utilization.gpu,temperature.gpu",
             "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=2, creationflags=NO_WINDOW,
        ).stdout.strip().splitlines()[0]
        name, load, temp = [x.strip() for x in out.split(",")]
        return {"name": name, "load": number(load), "temp": number(temp)}
    except Exception:
        return None


# ---------------------------------------------------- LibreHardwareMonitor
def leaves(node, path=(), icons=()):
    path = path + (node.get("Text", ""),)
    icons = icons + (node.get("ImageURL", "").lower(),)
    kids = node.get("Children") or []
    if not kids:
        yield path, icons, node.get("Value", "")
    for kid in kids:
        yield from leaves(kid, path, icons)


GPU_WORDS = ("geforce", "radeon", "rtx", "gtx", "nvidia", "arc ", "graphics", "gpu")


def lhm_sensors():
    """Return a list of classified sensors, or None if LibreHardwareMonitor isn't reachable."""
    try:
        with urllib.request.urlopen(LHM_URL, timeout=0.8) as r:
            tree = json.load(r)
    except Exception:
        return None
    out = []
    for path, icons, value in leaves(tree):
        hw_path, hw_icons = path[2:-2], icons[2:-2]
        hw_name = hw_path[0] if hw_path else ""
        hw_text = " ".join(hw_path).lower()
        name = path[-1].lower()
        is_cpu = any("cpu" in i for i in hw_icons)
        is_gpu = not is_cpu and (
            any(w in hw_text for w in GPU_WORDS)
            or any(w in " ".join(hw_icons) for w in ("nvidia", "ati", "gpu"))
            or name.startswith("gpu"))
        v = number(value)
        if v is None:
            continue
        kind = "temp" if "°c" in value.lower() else "load" if "%" in value else "other"
        out.append({"hw": "cpu" if is_cpu else "gpu" if is_gpu else "-",
                    "hw_name": hw_name, "name": name, "kind": kind, "value": v})
    return out


def pick(items, prefer):
    if not items:
        return None
    for key in prefer:
        for n, v in items:
            if key in n:
                return v
    return max(v for _, v in items)


def read_lhm():
    s = lhm_sensors() or []
    sel = lambda hw, kind: [(x["name"], x["value"]) for x in s if x["hw"] == hw and x["kind"] == kind]
    name = lambda hw: next((x["hw_name"] for x in s if x["hw"] == hw and x["hw_name"]), None)
    return {
        "cpu_temp": pick(sel("cpu", "temp"), ("package", "tctl", "tdie", "cpu")),
        "gpu_temp": pick(sel("gpu", "temp"), ("gpu core",)),
        "gpu_load": pick(sel("gpu", "load"), ("gpu core", "d3d 3d")),
        "cpu_name": name("cpu"), "gpu_name": name("gpu"),
    }


# ------------------------------------------------------------------ rest
def read_disks():
    out = []
    for p in psutil.disk_partitions(all=False):
        if "cdrom" in p.opts or not p.fstype:
            continue
        try:
            u = psutil.disk_usage(p.mountpoint)
        except Exception:
            continue
        out.append({"name": p.mountpoint.rstrip("\\/") or p.mountpoint,
                    "used": u.used, "total": u.total, "percent": u.percent})
    return out


def read_top():
    """Top CPU-hungry programs, summed per program name, as % of the whole CPU."""
    agg = {}
    for p in psutil.process_iter(["name", "cpu_percent"]):
        info = p.info
        if p.pid == 0 or not info.get("name"):
            continue
        agg[info["name"]] = agg.get(info["name"], 0.0) + (info.get("cpu_percent") or 0.0)
    top = sorted(agg.items(), key=lambda kv: kv[1], reverse=True)[:8]
    return [{"name": n, "cpu": round(c / CPU_COUNT, 1)} for n, c in top if c / CPU_COUNT >= 0.3]


def collector():
    psutil.cpu_percent(None)
    last, t0 = psutil.net_io_counters(), time.time()
    disks, disks_t = [], 0
    top, n = [], 0
    read_top()  # first call only primes the per-process counters
    while True:
        time.sleep(1)
        n += 1
        if n % 2 == 0:
            top = read_top()
        now = time.time()
        net = psutil.net_io_counters()
        dt = max(now - t0, 0.001)
        up = (net.bytes_sent - last.bytes_sent) / dt
        down = (net.bytes_recv - last.bytes_recv) / dt
        last, t0 = net, now
        if now - disks_t > 5:
            disks, disks_t = read_disks(), now

        lhm, nv = read_lhm(), read_nvidia() or {}
        pick_ = lambda a, b: a if a is not None else b
        mem = psutil.virtual_memory()
        stats.update(
            cpu={"load": psutil.cpu_percent(None), "temp": lhm["cpu_temp"],
                 "name": lhm["cpu_name"] or CPU_NAME, "ghz": current_ghz()},
            gpu={"load": pick_(nv.get("load"), lhm["gpu_load"]),
                 "temp": pick_(nv.get("temp"), lhm["gpu_temp"]),
                 "name": nv.get("name") or lhm["gpu_name"]},
            ram={"used": mem.used, "total": mem.total, "percent": mem.percent},
            net={"up": max(up, 0), "down": max(down, 0)},
            disks=disks,
            top=top,
            uptime=int(time.time() - psutil.boot_time()),
        )


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def foreign(self):
        """Jarvis can press keys and open apps, so only the dashboard itself may talk to him -
        never a website that happens to be open in the browser."""
        host = (self.headers.get("Host") or "").split(":")[0]
        return host not in ("127.0.0.1", "localhost") or self.headers.get("Origin") not in PAGES

    def allow(self):
        o = self.headers.get("Origin")
        if o in PAGES:
            self.send_header("Access-Control-Allow-Origin", o)
            self.send_header("Vary", "Origin")

    def send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.allow()
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.foreign():
            return self.send(403, b"Forbidden", "text/plain")
        path = self.path.split("?")[0]
        if path == "/stats":
            self.send(200, json.dumps(stats).encode(), "application/json")
        elif path == "/jarvis/config":
            # the dashboard asks for its own wording at load, so there is no
            # second copy of the translations inside the HTML
            self.send(200, json.dumps({
                "lang": config.LANG,
                "ui": texts.pick(texts.UI, config.LANG),
                "button": config.TALK_BUTTON,
                "cpuAlert": config.CPU_ALERT,
            }).encode(), "application/json")
        elif path == "/jarvis/voices":
            self.send(200, json.dumps(jarvis.voices()).encode(), "application/json")
        elif path == "/jarvis/trigger":
            # long poll: comes back the moment the talk button goes down or up
            q = self.path.split("?", 1)[1] if "?" in self.path else ""
            try:
                since = int(dict(p.split("=", 1) for p in q.split("&") if "=" in p)
                            .get("since", -1))
            except ValueError:
                since = -1
            n = hotkey.events if since < 0 else hotkey.wait(since, 25)
            self.send(200, json.dumps({"events": n, "down": hotkey.down,
                                       "hooked": hotkey.hooked,
                                       "button": hotkey._want()}).encode(),
                      "application/json")
        elif path in ("/", "/index.html", "/dashboard.html") and os.path.exists(HTML_FILE):
            with open(HTML_FILE, "rb") as f:
                self.send(200, f.read(), "text/html; charset=utf-8")
        else:
            self.send(404, b"Not found", "text/plain")

    def do_OPTIONS(self):
        self.send_response(204)
        self.allow()
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.end_headers()

    def do_POST(self):
        if self.foreign():
            return self.send(403, b"Forbidden", "text/plain")
        path = self.path.split("?")[0]
        if path == "/jarvis/transcribe":
            # raw audio body from the PC Dashboard app -> Groq Whisper
            try:
                n = int(self.headers.get("Content-Length") or 0)
                audio = self.rfile.read(n) if 0 < n < 25_000_000 else b""
            except Exception:
                audio = b""
            text, err = jarvis.transcribe(audio, self.headers.get("Content-Type") or "audio/webm")
            self.send(200, json.dumps({"text": text} if text is not None else {"error": err}).encode(),
                      "application/json")
            return
        try:
            n = int(self.headers.get("Content-Length") or 0)
            body = json.loads(self.rfile.read(n) or b"{}")
        except Exception:
            self.send(400, b'{"error":"bad request"}', "application/json")
            return
        if path == "/jarvis/ask":
            out = jarvis.ask(body.get("text"), stats)
            self.send(200, json.dumps(out).encode(), "application/json")
        elif path == "/jarvis/speak":
            audio, err = jarvis.speak(body.get("text"))
            if audio:
                self.send(200, audio, "audio/mpeg")
            else:
                self.send(502, json.dumps({"error": err}).encode(), "application/json")
        elif path == "/jarvis/reset":
            jarvis.reset()
            self.send(200, b'{"ok":true}', "application/json")
        else:
            self.send(404, b'{"error":"not found"}', "application/json")


def debug():
    print("CPU name:", CPU_NAME)
    threading.Thread(target=freq_worker, daemon=True).start()
    time.sleep(2.5)
    print("CPU clock (GHz):", current_ghz())
    print("nvidia-smi path:", NVSMI)
    print("nvidia-smi reading:", read_nvidia())
    s = lhm_sensors()
    if s is None:
        print("LibreHardwareMonitor: NOT reachable at", LHM_URL)
        print("  -> start it as administrator and enable Options > Remote Web Server > Run")
    else:
        print("LibreHardwareMonitor: reachable,", len(s), "sensors")
        for x in s:
            if x["kind"] in ("temp", "load") and x["hw"] != "-":
                print(f'  [{x["hw"]}] {x["hw_name"]} | {x["name"]} | {x["kind"]} = {x["value"]}')
        print("Summary:", read_lhm())
    read_top()
    time.sleep(1.5)
    print("Top processes right now:", read_top())


if __name__ == "__main__":
    if "--debug" in sys.argv:
        debug()
        sys.exit()
    if psutil:
        threading.Thread(target=freq_worker, daemon=True).start()
        threading.Thread(target=collector, daemon=True).start()
    print("Talk button:", hotkey._want(), "- hook active:", hotkey.start())
    print(f"Mission Control running: http://localhost:{PORT}")
    ThreadingHTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
