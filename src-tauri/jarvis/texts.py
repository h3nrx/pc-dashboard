"""Every piece of wording, German and English, in one place.

The language is chosen once in setup.py and lands in settings.json. The Python
side reads it from here; the dashboard asks the server for its own strings at
load (GET /jarvis/config), so there is no second copy of them in the HTML.

Adding a language: copy a block, translate it, and add the code to LANGS.
"""
LANGS = ("de", "en")


def pick(table, lang):
    return table.get(lang, table["en"])


# ------------------------------------------------------------ Jarvis himself
PERSONA = {
"de": """Du bist JARVIS, der Assistent dieses PCs. Du sprichst Deutsch, wirst
vorgelesen und antwortest daher kurz: ein bis drei Saetze, keine Listen, keine
Emojis, keine Markdown-Zeichen, keine Abkuerzungen die man nicht sprechen kann.
Du bist trocken, ruhig und hoeflich, nennst den Nutzer "Sir" wenn es passt.
Antworte immer auf Deutsch, auch wenn ein Werkzeug englisch antwortet.
Allgemeine Fragen beantwortest du ganz normal aus deinem Wissen, auch wenn es
nur ein Richtwert ist - sag dann einfach "etwa" dazu. Nur bei den Messwerten
dieses Rechners erfindest du nichts: die stehen unten, nutze genau die. Wenn du
etwas wirklich nicht weisst, sag es in einem Satz.

Du kannst den PC auch bedienen: Programme starten, Maus und Tastatur steuern,
Fenster wechseln, Lautstaerke und Wiedergabe regeln. Nutze dafuer die Werkzeuge,
ohne vorher um Erlaubnis zu fragen - der Nutzer hat dich gerade darum gebeten.
Regeln dabei:
- Erst handeln, dann in einem kurzen Satz melden was du getan hast.
- Wenn ein Programmname nicht gefunden wird, schau mit list_apps nach statt zu raten.
- Du siehst den Bildschirm nicht. Wenn eine Aufgabe das Erkennen von
  Bildschirminhalten braucht, sag das in einem Satz, statt blind zu klicken.
- Fuer ein bestimmtes Fenster nimm focus_window oder ein Tastenkuerzel.
- lock_pc nur, wenn ausdruecklich danach gefragt wurde.""",

"en": """You are JARVIS, the assistant of this PC. You speak English and are read
aloud, so keep it short: one to three sentences, no lists, no emoji, no
markdown, nothing that cannot be spoken out loud.
You are dry, calm and courteous, and call the user "Sir" where it fits.
Always answer in English, even when a tool replies in another language.
General questions you answer normally from what you know, a ballpark figure is
fine - just say "about". Only with this machine's readings do you invent
nothing: they are below, use exactly those. If you really do not know
something, say so in one sentence.

You can also operate the PC: start programs, drive mouse and keyboard, switch
windows, change volume and playback. Use the tools for that without asking
permission first - the user has just asked you to. Rules:
- Act first, then report in one short sentence what you did.
- If a program name is not found, check with list_apps instead of guessing.
- You cannot see the screen. If a task needs reading what is on screen, say so
  in one sentence instead of clicking blindly.
- For a particular window use focus_window or a keyboard shortcut.
- lock_pc only when explicitly asked for.""",
}

# the handful of words the live readings are built from
WORDS = {
"de": {"load": "Last", "deg": "Grad", "ghz": "Gigahertz", "gb": "Gigabyte",
       "used": "belegt", "free": "frei", "drive": "Laufwerk", "full": "voll",
       "top": "Top-Prozesse", "uptime": "Laufzeit", "days": "Tage",
       "hours": "Stunden", "mins": "Minuten", "head": "Aktuelle Messwerte",
       "stuck": "Ich komme bei dieser Aufgabe nicht weiter, Sir.",
       "done": "Erledigt, Sir.", "empty": "leere Frage",
       "nokey": "kein Groq API-Key gesetzt", "notext": "leerer Text",
       "novoicekey": "kein ElevenLabs API-Key gesetzt"},
"en": {"load": "load", "deg": "degrees", "ghz": "gigahertz", "gb": "gigabytes",
       "used": "used", "free": "free", "drive": "Drive", "full": "full",
       "top": "Top processes", "uptime": "Uptime", "days": "days",
       "hours": "hours", "mins": "minutes", "head": "Current readings",
       "stuck": "I cannot get any further with this, Sir.",
       "done": "Done, Sir.", "empty": "empty question",
       "nokey": "no Groq API key set", "notext": "empty text",
       "novoicekey": "no ElevenLabs API key set"},
}

# what the tools report back - shown in the dashboard, read by the model
TOOL = {
"de": {"screen": "Bildschirm {w}x{h}, Mauszeiger bei {x},{y}",
       "moved": "Maus auf {x},{y}", "click": "Klick {b}", "dclick": "Doppelklick {b}",
       "scrolled": "{n} Klicks gescrollt", "typed": "{n} Zeichen getippt",
       "nokey": "Taste '{k}' kenne ich nicht", "pressed": "{k} gedrueckt",
       "vol_up": "lauter", "vol_down": "leiser", "vol_mute": "stumm geschaltet",
       "media": "Medientaste {a}", "noapp": "kein App-Name angegeben",
       "opened": "{n} geoeffnet", "started": "{n} gestartet",
       "notinst": "'{n}' ist nicht installiert. Nutze list_apps um zu sehen was da ist.",
       "nothing": "nichts gefunden", "nourl": "keine Adresse angegeben",
       "nowin": "keine Fenster", "focused": "'{t}' in den Vordergrund geholt",
       "nosuchwin": "kein Fenster mit '{t}' offen", "closed": "Fenster geschlossen",
       "desktop": "Desktop gezeigt", "locked": "PC gesperrt",
       "notool": "unbekanntes Werkzeug {n}",
       "badargs": "falsche Parameter fuer {n}: {e}", "failed": "{n} fehlgeschlagen: {e}"},
"en": {"screen": "Screen {w}x{h}, cursor at {x},{y}",
       "moved": "Mouse at {x},{y}", "click": "{b} click", "dclick": "{b} double click",
       "scrolled": "scrolled {n} notches", "typed": "typed {n} characters",
       "nokey": "unknown key '{k}'", "pressed": "pressed {k}",
       "vol_up": "volume up", "vol_down": "volume down", "vol_mute": "muted",
       "media": "media key {a}", "noapp": "no app name given",
       "opened": "opened {n}", "started": "started {n}",
       "notinst": "'{n}' is not installed. Use list_apps to see what is.",
       "nothing": "nothing found", "nourl": "no address given",
       "nowin": "no windows", "focused": "brought '{t}' to the front",
       "nosuchwin": "no window matching '{t}'", "closed": "window closed",
       "desktop": "showing the desktop", "locked": "PC locked",
       "notool": "unknown tool {n}",
       "badargs": "wrong parameters for {n}: {e}", "failed": "{n} failed: {e}"},
}

# everything the dashboard shows - served to it at load, see monitor.py
UI = {
"de": {"speech": "de-DE",          # what the browser listens for
       "hint1": "{button} halten und sprechen, loslassen für die Antwort",
       "btn_x2": "Obere Daumentaste", "btn_x1": "Untere Daumentaste",
       "btn_middle": "Mausrad",
       "hint2": "Klick ins Feld geht auch", "reset": "Neu", "full": "Vollbild",
       "you": "Du", "jarvis": "Jarvis", "error": "Fehler", "action": "Aktion",
       "ready": "Bereit", "listening": "Hört zu",
       "thinking": "Denkt", "working": "Verarbeitet", "speaking": "Spricht",
       "connecting": "Verbinde …", "serverOff": "Server offline",
       "noSR": "Spracherkennung braucht Chrome oder Edge über http://localhost:",
       "noMic": "Mikrofon blockiert – im Browser erlauben und neu laden",
       "noMicDev": "Kein Mikrofon gefunden",
       "noHook": "Maustaste konnte nicht abgefangen werden – klick ins Feld",
       "noAudio": "Der Browser lässt noch keinen Ton zu – einmal ins Fenster klicken",
       "greetNight": "Gute Nacht", "greetMorning": "Guten Morgen",
       "greetDay": "Guten Tag", "greetEvening": "Guten Abend",
       "uptime": "Laufzeit", "live": "Live", "offline": "Offline – monitor.py starten",
       "pTitle": "Top-Prozesse", "pAlert": "CPU bei {n}% – wer sie braucht",
       "memory": "Speicher", "links": "Links",
       "noProc": "Keine Prozessdaten. monitor.py aktualisieren.",
       "noSensor": "Kein Sensor – python monitor.py --debug"},
"en": {"speech": "en-US",          # what the browser listens for
       "hint1": "Hold the {button} and speak, release for the answer",
       "btn_x2": "upper thumb button", "btn_x1": "lower thumb button",
       "btn_middle": "mouse wheel",
       "hint2": "Clicking the panel works too", "reset": "Reset", "full": "Fullscreen",
       "you": "You", "jarvis": "Jarvis", "error": "Error", "action": "Action",
       "ready": "Ready", "listening": "Listening",
       "thinking": "Thinking", "working": "Working", "speaking": "Speaking",
       "connecting": "Connecting …", "serverOff": "Server offline",
       "noSR": "Speech recognition needs Chrome or Edge on http://localhost:",
       "noMic": "Microphone blocked – allow it in the browser and reload",
       "noMicDev": "No microphone found",
       "noHook": "The mouse button could not be captured – click the panel",
       "noAudio": "The browser is still blocking sound – click the window once",
       "greetNight": "Good night", "greetMorning": "Good morning",
       "greetDay": "Good afternoon", "greetEvening": "Good evening",
       "uptime": "Uptime", "live": "Live", "offline": "Offline – start monitor.py",
       "pTitle": "Top processes", "pAlert": "CPU at {n}% – who is using it",
       "memory": "Memory", "links": "Links",
       "noProc": "No process data. Update monitor.py.",
       "noSensor": "No sensor – python monitor.py --debug"},
}
