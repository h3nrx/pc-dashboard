"""Jarvis - voice agent backend for Mission Control.

Brain:  Groq (chat completions, very fast)
Voice:  ElevenLabs (text to speech, German-capable multilingual model)
Ears:   the browser's own speech recognition - see dashboard.html
Hands:  actions.py
Keys and model names:       config.py

Pick another voice with:  python jarvis.py --voices
"""
import json
import sys
import urllib.request

import actions
import config
import texts
from config import ELEVEN_KEY, ELEVEN_MODEL, ELEVEN_VOICE, GROQ_KEY, GROQ_MODEL

ELEVEN_URL = "https://api.elevenlabs.io/v1/text-to-speech/"
W = texts.pick(texts.WORDS, config.LANG)        # the wording for this language

PERSONA = texts.pick(texts.PERSONA, config.LANG)

MAX_TURNS = 12          # user+assistant pairs kept in memory
history = []


def reset():
    history.clear()


# ----------------------------------------------------------------- helpers
_request, _error_text = config.post, config.error_text


def context(stats):
    """One compact block of live machine state for the system prompt."""
    if not stats:
        return ""
    cpu, gpu, ram = stats.get("cpu", {}), stats.get("gpu", {}), stats.get("ram", {})
    gb = lambda b: round((b or 0) / 1073741824, 1)
    bits = [f"CPU {cpu.get('name') or '?'}: {round(cpu.get('load') or 0)}% {W['load']}"]
    if cpu.get("temp") is not None:
        bits.append(f"{round(cpu['temp'])} {W['deg']}")
    if cpu.get("ghz") is not None:
        bits.append(f"{cpu['ghz']} {W['ghz']}")
    if gpu.get("load") is not None:
        g = f"GPU {gpu.get('name') or '?'}: {round(gpu['load'])}% {W['load']}"
        if gpu.get("temp") is not None:
            g += f", {round(gpu['temp'])} {W['deg']}"
        bits.append(g)
    bits.append(f"RAM: {gb(ram.get('used'))} / {gb(ram.get('total'))} {W['gb']} {W['used']} "
                f"({round(ram.get('percent') or 0)}%), "
                f"{gb((ram.get('total') or 0) - (ram.get('used') or 0))} {W['gb']} {W['free']}")
    for d in stats.get("disks", [])[:4]:
        bits.append(f"{W['drive']} {d['name']}: {round(d['percent'])}% {W['full']}, "
                    f"{gb(d['total'] - d['used'])} {W['gb']} {W['free']}")
    top = ", ".join(f"{t['name']} {t['cpu']}%" for t in (stats.get("top") or [])[:5])
    if top:
        bits.append(f"{W['top']}: " + top)
    u = stats.get("uptime") or 0
    bits.append(f"{W['uptime']}: {u // 86400} {W['days']} {u % 86400 // 3600} {W['hours']} "
                f"{u % 3600 // 60} {W['mins']}")
    return f"{W['head']}:\n- " + "\n- ".join(bits)


# -------------------------------------------------------------------- Groq
MAX_STEPS = 5           # tool rounds per question, so he cannot loop forever


def _complete(messages):
    return config.groq({
        "model": GROQ_MODEL,
        "messages": messages,
        "tools": actions.TOOLS,
        "tool_choice": "auto",
        "temperature": 0.5,
        "max_completion_tokens": 700,
        "reasoning_effort": "low",
    }, timeout=60)      # a look at the screen takes a while


def ask(text, stats=None):
    """Answer one spoken line, using tools when it needs them.

    Returns {"text": ..., "actions": [...]} or {"error": ...}.
    """
    text = (text or "").strip()
    if not text:
        return {"error": W["empty"]}
    if not GROQ_KEY:
        return {"error": W["nokey"]}

    messages = [{"role": "system", "content": PERSONA + "\n\n" + context(stats)}]
    messages += history
    messages.append({"role": "user", "content": text})

    done = []
    try:
        for _ in range(MAX_STEPS):
            msg = _complete(messages)
            calls = msg.get("tool_calls") or []
            if not calls:
                reply = (msg.get("content") or "").strip()
                break
            messages.append(msg)
            for c in calls:
                fn = c["function"]
                try:
                    args = json.loads(fn.get("arguments") or "{}")
                except ValueError:
                    args = {}
                out = actions.run(fn["name"], args)
                done.append({"tool": fn["name"], "args": args,
                             "ok": out.get("ok", False), "result": out.get("result", "")})
                messages.append({"role": "tool", "tool_call_id": c["id"],
                                 "content": json.dumps(out, ensure_ascii=False)})
        else:
            reply = W["stuck"]
    except Exception as e:
        return {"error": "Groq: " + _error_text(e), "actions": done}

    if not reply:
        reply = done[-1]["result"] if done else W["done"]
    history.append({"role": "user", "content": text})
    history.append({"role": "assistant", "content": reply})
    del history[:max(0, len(history) - MAX_TURNS * 2)]
    return {"text": reply, "actions": done}


# -------------------------------------------------------------- ElevenLabs
def speak(text):
    """Text to mp3. Returns (audio_bytes, None) or (None, error_text)."""
    text = (text or "").strip()
    if not text:
        return None, W["notext"]
    if not ELEVEN_KEY:
        return None, W["novoicekey"]
    payload = json.dumps({
        "text": text[:2500],
        "model_id": ELEVEN_MODEL,
        "voice_settings": {"stability": 0.45, "similarity_boost": 0.8, "style": 0.0},
    }).encode()
    url = f"{ELEVEN_URL}{ELEVEN_VOICE}?output_format=mp3_44100_128"
    try:
        return _request(url, payload,
                        {"xi-api-key": ELEVEN_KEY,
                         "Content-Type": "application/json",
                         "Accept": "audio/mpeg"}, 40), None
    except Exception as e:
        return None, "ElevenLabs: " + _error_text(e)


def transcribe(audio, mime="audio/webm"):
    """Speech to text via Groq Whisper (same Groq key). Used by the PC Dashboard
    app, whose WebView has no built-in speech recognition. Returns (text, error)."""
    if not GROQ_KEY:
        return None, W["nokey"]
    if not audio:
        return None, W["empty"]
    ext = "ogg" if "ogg" in (mime or "") else "wav" if "wav" in (mime or "") else "webm"
    boundary = "----pcdash" + str(abs(hash(audio[:64])))
    parts = []
    for k, v in (("model", "whisper-large-v3-turbo"), ("language", config.LANG),
                 ("response_format", "json")):
        parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="{k}"\r\n\r\n{v}\r\n'.encode())
    parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="talk.{ext}"\r\n'
                 f'Content-Type: {mime or "audio/webm"}\r\n\r\n'.encode() + audio + b"\r\n")
    parts.append(f"--{boundary}--\r\n".encode())
    try:
        raw = _request("https://api.groq.com/openai/v1/audio/transcriptions", b"".join(parts),
                       {"Authorization": "Bearer " + GROQ_KEY,
                        "Content-Type": "multipart/form-data; boundary=" + boundary}, 40)
        return (json.loads(raw).get("text") or "").strip(), None
    except Exception as e:
        return None, "Groq Whisper: " + _error_text(e)


def voices():
    """All voices on the ElevenLabs account, for picking ELEVEN_VOICE."""
    try:
        req = urllib.request.Request("https://api.elevenlabs.io/v1/voices",
                                     headers={"xi-api-key": ELEVEN_KEY,
                                              "User-Agent": "MissionControl/1.0"})
        with urllib.request.urlopen(req, timeout=15) as r:
            data = json.load(r)
        return {"voices": [{"id": v["voice_id"], "name": v.get("name"),
                            "labels": v.get("labels", {})}
                           for v in data.get("voices", [])],
                "current": ELEVEN_VOICE}
    except Exception as e:
        return {"error": "ElevenLabs: " + _error_text(e)}


if __name__ == "__main__":
    if "--voices" in sys.argv:
        v = voices()
        if "error" in v:
            print(v["error"])
        else:
            print(f"Aktuelle Stimme: {v['current']}\n")
            for x in v["voices"]:
                print(f'{x["id"]}  {x["name"]:<22} {x["labels"]}')
        sys.exit()
    print("Groq-Modell:", GROQ_MODEL)
    print(ask(" ".join(sys.argv[1:]) or "Melde dich kurz, Jarvis."))
    audio, err = speak("Systeme bereit, Sir.")
    print("TTS:", err or f"{len(audio)} Bytes mp3 OK")
