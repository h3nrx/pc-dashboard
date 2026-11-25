"""Settings and the bit of HTTP everything here shares.

Nothing is configured in this file. Everything lives in settings.json next to
it, written by the PC Dashboard app. Environment variables still win, so you can keep the keys out of any file:
    setx GROQ_API_KEY "..."      setx ELEVENLABS_API_KEY "..."

settings.json holds your personal keys. Never pass it on.
"""
import json
import os
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))

SETTINGS_FILE = os.path.join(HERE, "settings.json")


def sources():
    return [SETTINGS_FILE] if os.path.exists(SETTINGS_FILE) else []


DEFAULTS = {
    "lang": "en",                        # what a fresh install starts in
    "groq_key": "",
    "elevenlabs_key": "",
    "voice": "JBFqnCBsd6RMkjVDRZzb",     # George - warm, British
    "voice_name": "George",
    "talk_button": "x2",                 # upper thumb button
    "talk_swallow": True,
    "cpu_alert": 70,
}


def load():
    """The settings merged over the defaults. Never raises."""
    data = dict(DEFAULTS)
    for path in sources():
        try:
            with open(path, encoding="utf-8") as f:
                data.update(json.load(f))
            break
        except Exception:
            continue
    return data


def _write(path, merged):
    """Write beside the target, prove it arrived, then swap it in.

    Never open the real file for writing first: opening it empties it, and if
    the write is then discarded - which is exactly what the folder protection
    does - the old settings are gone. This way a blocked write loses nothing.
    """
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    tmp = path + ".new"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(merged, f, indent=2, ensure_ascii=False)
    with open(tmp, encoding="utf-8") as f:
        json.load(f)                 # it really is on disk and readable
    os.replace(tmp, path)
    with open(path, encoding="utf-8") as f:
        json.load(f)                 # and the swap went through as well
    return path


def save(data):
    """Returns (settings, path)."""
    merged = load()
    merged.update(data)
    return merged, _write(SETTINGS_FILE, merged)


def configured():
    """Is this ready to run? A half-finished setup counts as not configured,
    so the launcher goes back to the menu instead of starting a mute Jarvis."""
    if not sources():
        return False
    s = load()
    return bool((os.environ.get("GROQ_API_KEY") or s["groq_key"])
                and (os.environ.get("ELEVENLABS_API_KEY") or s["elevenlabs_key"]))


S = load()

GROQ_KEY = os.environ.get("GROQ_API_KEY") or S["groq_key"]
ELEVEN_KEY = os.environ.get("ELEVENLABS_API_KEY") or S["elevenlabs_key"]
LANG = (os.environ.get("JARVIS_LANG") or S["lang"]).lower()[:2]
ELEVEN_VOICE = os.environ.get("ELEVENLABS_VOICE") or S["voice"]
TALK_BUTTON = str(S["talk_button"]).lower()
TALK_SWALLOW = bool(S["talk_swallow"])
CPU_ALERT = int(S["cpu_alert"])

GROQ_MODEL = os.environ.get("GROQ_MODEL", "openai/gpt-oss-120b")
ELEVEN_MODEL = os.environ.get("ELEVENLABS_MODEL", "eleven_multilingual_v2")
GROQ_URL = "https://api.groq.com/openai/v1/chat/completions"


def post(url, payload, headers, timeout=30):
    """POST and return the raw body. Groq sits behind Cloudflare, which rejects
    the default urllib user agent, so every call carries our own."""
    data = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, method="POST",
                                 headers={"User-Agent": "MissionControl/1.0", **headers})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def groq(payload, timeout=30):
    """One chat completion. Returns the message dict."""
    raw = post(GROQ_URL, payload,
               {"Authorization": "Bearer " + GROQ_KEY,
                "Content-Type": "application/json"}, timeout)
    return json.loads(raw)["choices"][0]["message"]


def error_text(e):
    """Turn an exception into something short enough to say out loud."""
    if isinstance(e, urllib.error.HTTPError):
        try:
            body = json.loads(e.read())
            msg = body.get("error")
            msg = msg.get("message") if isinstance(msg, dict) else msg
            if isinstance(body.get("detail"), dict):
                msg = body["detail"].get("message") or msg
            return f"HTTP {e.code}: {msg or 'unknown error'}"
        except Exception:
            return f"HTTP {e.code}"
    return str(e) or e.__class__.__name__
