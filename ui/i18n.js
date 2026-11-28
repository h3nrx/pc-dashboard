// German texts. The app is written in English; this swaps every visible text while German is selected.
const LANG = (() => { try { return localStorage.getItem("lang") || "en"; } catch { return "en"; } })();
const LOC = LANG === "de" ? "de-DE" : "en-US";
const DE = {
  // navigation, general
  "Overview": "Übersicht", "Storage": "Speicher", "Startup apps": "Autostart", "Settings": "Einstellungen",
  "Clean up and see what eats your space": "Aufräumen und sehen, was Platz frisst",
  "Everything that launches when Windows starts": "Alles, was mit Windows startet",
  "Desktop themes found on this PC": "Desktop-Themes, die auf diesem PC gefunden wurden",
  "Make the app yours": "Stell die App so ein, wie du willst",
  "Uptime": "Laufzeit", "Cancel": "Abbrechen", "Yes, do it": "Ja, machen", "Save": "Speichern", "Saving …": "Speichert …", "Loading …": "Lädt …",
  "Minimize": "Minimieren", "Close": "Schließen", "System": "System", "Shortcut": "Verknüpfung", "Task": "Aufgabe",
  // overview
  "Set up": "Einrichten", "Set up Jarvis": "Jarvis einrichten", "Click to set up Jarvis": "Klicken, um Jarvis einzurichten", "Talk key": "Sprechtaste", "hold it to talk": "zum Sprechen halten",
  "Click, then press a key or a mouse button": "Klicken, dann eine Taste oder Maustaste drücken", "Mouse back": "Maus zurück", "Mouse forward": "Maus vor", "Mouse wheel button": "Mausrad-Taste",
  "That key already belongs to Clips.": "Die Taste gehört schon zu den Clips.", "That key already belongs to Jarvis.": "Die Taste gehört schon Jarvis.",
  "free": "frei", "used": "belegt", "total": "gesamt", "Processor load": "Prozessorlast", "Memory": "Arbeitsspeicher", "GPU": "Grafikkarte",
  "No NVIDIA GPU found": "Keine NVIDIA-Grafikkarte gefunden", "Can be cleaned": "Kann weg", "searching": "sucht", "all clean": "alles sauber",
  "only stuff to review": "nur Sachen zum Prüfen", "Desktop themes": "Desktop-Themes", "turned on": "eingeschaltet",
  // jarvis
  "Checking …": "Prüft …", "API keys": "API-Keys", "Not installed": "Nicht installiert", "Needs API keys": "Braucht API-Keys", "Offline": "Offline", "Online": "Online",
  "Listening": "Hört zu", "Thinking": "Denkt nach", "Speaking": "Spricht", "Starting …": "Startet …", "Jarvis is starting …": "Jarvis startet …",
  "Click here to set up Jarvis.": "Klick hier, um Jarvis einzurichten.", "Needs Python": "Braucht Python", "needed for Jarvis": "braucht Jarvis", "Download": "Herunterladen", "Hold to talk": "Zum Sprechen halten", "Click to set your API keys": "Klicken, um API-Keys zu setzen",
  "Jarvis API keys": "Jarvis API-Keys", "Saved only on this PC. Leave a field empty to keep the key you have.": "Nur auf diesem PC gespeichert. Leeres Feld = alter Key bleibt.",
  "brain + ears": "Gehirn + Ohren", "voice": "Stimme", "Get a key": "Key holen", "Language": "Sprache", "Saved. Jarvis restarted.": "Gespeichert. Jarvis wurde neu gestartet.",
  "Jarvis folder not found.": "Jarvis-Ordner nicht gefunden.", "Could not save the keys (write was blocked).": "Keys konnten nicht gespeichert werden (Schreiben blockiert).",
  "Jarvis did not start. Run start-dashboard.bat once to see the error.": "Jarvis ist nicht gestartet. Öffne einmal start-dashboard.bat, um den Fehler zu sehen.",
  // storage
  "Clean up": "Aufräumen", "Everything goes to the Recycle Bin, so you can always get it back.": "Alles landet im Papierkorb, du kannst es also immer zurückholen.",
  "Looking for junk …": "Sucht nach Müll …", "Selected": "Ausgewählt", "Move to Recycle Bin": "In den Papierkorb", "Biggest folders": "Größte Ordner",
  "What takes up the most space on C:": "Was auf C: am meisten Platz braucht", "Scan": "Scannen", "Scan again": "Nochmal scannen", "Scanning …": "Scannt …", "Measuring folders …": "Misst Ordner …",
  "Hit “Scan”. Takes about a minute.": "Drück auf „Scannen“. Dauert etwa eine Minute.", "Nothing to clean. Spotless!": "Nichts aufzuräumen. Blitzblank!", "Safe": "Sicher", "Review": "Prüfen",
  "Move to Recycle Bin?": "In den Papierkorb verschieben?", "These will be moved to the Recycle Bin. You can restore them from there.": "Das kommt in den Papierkorb. Du kannst es von dort wiederherstellen.",
  "Cleaning …": "Räumt auf …", "Open in Explorer": "Im Explorer öffnen", "That folder doesn't exist anymore": "Den Ordner gibt es nicht mehr",
  "Old CapCut versions": "Alte CapCut-Versionen", "Temporary files": "Temporäre Dateien", "Crash dumps": "Absturzberichte", "Old Medal clips": "Alte Medal-Clips", "Old installers in Downloads": "Alte Installer in Downloads",
  // startup
  "Starts with Windows": "Startet mit Windows", "Turning something off doesn't delete it, same as in Task Manager.": "Ausschalten löscht nichts, genau wie im Task-Manager.",
  "Leftovers of removed apps": "Reste von entfernten Apps", "Enabled": "Eingeschaltet", "Disabled": "Ausgeschaltet", "Remove this leftover entry": "Diesen Rest entfernen",
  "Remove leftover entry?": "Rest entfernen?", "This app isn't installed anymore. Only the dead startup entry gets removed.": "Die App ist nicht mehr installiert. Nur der tote Autostart-Eintrag wird entfernt.",
  "not installed": "nicht installiert", "Removed": "Entfernt", "Starts with Windows again": "Startet wieder mit Windows", "Won't start with Windows anymore": "Startet nicht mehr mit Windows",
  "This is a system entry, it needs admin rights.": "Das ist ein Systemeintrag, er braucht Adminrechte.", "That app is still installed. Just switch it off instead.": "Die App ist noch installiert. Schalte sie einfach aus.",
  // themes
  "Your desktop looks": "Deine Desktop-Looks", "Switch any of them on or off. Nothing gets deleted.": "Schalte alles einzeln an oder aus. Nichts wird gelöscht.",
  "Turned on": "Eingeschaltet", "Turned off": "Ausgeschaltet", "That didn't work": "Das hat nicht geklappt",
  "Boot screen": "Startbildschirm", "Screensaver": "Bildschirmschoner", "Taskbar icons": "Taskleisten-Icons", "Cursors": "Mauszeiger", "Discord theme": "Discord-Theme",
  "Chrome loading bar right after login": "Chrom-Ladebalken direkt nach der Anmeldung", "F6 toggles it, every other key is locked": "F6 schaltet ihn um, alle anderen Tasten sind gesperrt",
  "Chrome icons for Discord, Spotify, Chrome …": "Chrom-Icons für Discord, Spotify, Chrome …", "Chrome pointers with loading animation": "Chrom-Zeiger mit Ladeanimation",
  "No desktop themes on this PC.": "Keine Desktop-Themes auf diesem PC.", "Glass with chrome accents, via Vencord": "Glas mit Chrom-Akzenten, über Vencord",
  // clip editor
  "Cut": "Schneiden", "Split": "Teilen", "Delete part": "Teil löschen", "Restore": "Zurückholen", "Mute part": "Teil stumm", "Sound on": "Ton an",
  "Stop": "Stopp", "Saved as a new clip": "Als neuer Clip gespeichert", "At least one part has to stay": "Mindestens ein Teil muss bleiben",
  "Nothing left to save": "Nichts mehr übrig zum Speichern", "The clip is still loading, try again in a second": "Der Clip lädt noch, versuch es gleich nochmal",
  "Play / pause (Space)": "Abspielen / Pause (Leertaste)", "Cut at the white line (S)": "An der weißen Linie teilen (S)", "Selected part (Del)": "Ausgewählter Teil (Entf)",
  "Selected part (M)": "Ausgewählter Teil (M)", "Undo (Ctrl + Z)": "Rückgängig (Strg + Z)",
  // clips
  "Your whole screen, saved whenever something good happens": "Dein ganzer Bildschirm, gespeichert wenn was Gutes passiert",
  "Clip": "Clip", "Recording": "Aufnahme", "Clipping is on": "Clippen ist an", "Clipping is off": "Clippen ist aus", "Long recording": "Lange Aufzeichnung",
  "Switch it on to keep the last seconds of your screen.": "Schalte es an, um die letzten Sekunden deines Bildschirms zu behalten.",
  "Runs in the background, also with the window closed.": "Läuft im Hintergrund, auch wenn das Fenster zu ist.",
  "Saves what just happened": "Speichert, was gerade passiert ist", "Records until you stop it": "Nimmt auf, bis du stoppst", "Switch clipping on first": "Schalte erst das Clippen an", "Recording now": "Nimmt gerade auf",
  "Save clip": "Clip speichern", "Start recording": "Aufnahme starten", "Stop recording": "Aufnahme stoppen", "Length": "Länge", "Hotkey": "Taste",
  "Click, then press the new key. Backspace = no key.": "Klicken, dann neue Taste drücken. Rücktaste = keine Taste.", "Type a length, like 45, 90s, 2 min or 1:30": "Länge eintippen, z. B. 45, 90s, 2 min oder 1:30",
  "Press a key …": "Taste drücken …", "None": "Keine", "Open folder": "Ordner öffnen", "No clips yet.": "Noch keine Clips.", "All": "Alle", "Clips": "Clips", "Recordings": "Aufnahmen", "Nothing here yet.": "Hier ist noch nichts.",
  "Copy": "Kopieren", "Show file": "Datei zeigen", "Delete": "Löschen", "Delete this clip?": "Diesen Clip löschen?", "It goes to the Recycle Bin.": "Er kommt in den Papierkorb.", "Moved to the Recycle Bin": "In den Papierkorb verschoben",
  "Copied. Paste it into Discord with Ctrl + V.": "Kopiert. In Discord mit Strg + V einfügen.", "Saved": "Gespeichert", "Recording started": "Aufnahme läuft", "Clipping is off": "Clippen ist aus",
  "Type something like 45 s, 2 min or 1:30": "Schreib z. B. 45 s, 2 min oder 1:30", "Between 5 seconds and 10 minutes": "Zwischen 5 Sekunden und 10 Minuten",
  "That key can't be used. Try F9 or Alt + a letter.": "Die Taste geht nicht. Nimm z. B. F9 oder Alt + Buchstabe.", "Clip and recording need different keys.": "Clip und Aufnahme brauchen verschiedene Tasten.",
  "Nothing recorded yet, try again in a few seconds": "Noch nichts aufgenommen, probier es in ein paar Sekunden nochmal", "Not enough free space to record": "Zu wenig Speicherplatz zum Aufnehmen",
  "The recorder kept failing, so clipping was turned off to keep your PC safe. Switch it on to try again.": "Der Rekorder ist immer wieder ausgefallen, deshalb wurde Clipping zum Schutz deines PCs ausgeschaltet. Schalte es wieder an, um es erneut zu versuchen.",
  "Almost no space left on the drive, clipping was turned off": "Fast kein Platz mehr auf dem Laufwerk, Clipping wurde ausgeschaltet", "Not enough free space to save the clip": "Zu wenig Speicherplatz, um den Clip zu speichern",
  "Almost no space left, recording stopped": "Fast kein Platz mehr, Aufnahme gestoppt", "The recorder did not start": "Der Rekorder ist nicht gestartet", "That clip is gone": "Den Clip gibt es nicht mehr",
  "The recorder is not installed yet": "Der Rekorder ist noch nicht installiert", "Download the recorder?": "Rekorder herunterladen?", "Recorder installed": "Rekorder installiert",
  "Clips need ffmpeg, a free screen recorder. It is loaded once from GitHub.": "Clips brauchen ffmpeg, einen kostenlosen Rekorder. Er wird einmal von GitHub geladen.",
  "Already downloading": "Lädt schon", "Download failed. Check your internet connection.": "Download fehlgeschlagen. Prüf deine Internetverbindung.",
  "The download is damaged, nothing was changed": "Der Download ist beschädigt, es wurde nichts geändert", "Could not unpack the recorder": "Der Rekorder konnte nicht entpackt werden",
  "On": "An", "Off": "Aus", "Screen clips": "Bildschirm-Clips",
  // tabs, looks, updates
  "Hide": "Ausblenden", "Show all tabs": "Alle Tabs zeigen", "Visible tabs": "Sichtbare Tabs", "You can also right-click a tab to hide it": "Du kannst Tabs auch per Rechtsklick ausblenden",
  "Save and record your screen": "Bildschirm clippen und aufnehmen", "Clean up and find big folders": "Aufräumen und große Ordner finden", "What launches with Windows": "Was mit Windows startet", "Voice assistant on the overview": "Sprachassistent auf der Übersicht",
  "Appearance": "Aussehen", "Pick a look for the dashboard": "Such dir einen Look fürs Dashboard aus", "Animations": "Animationen", "Snow, petals, stars and lights": "Schnee, Blüten, Sterne und Lichter",
  "Grey glass": "Graues Glas", "Snow and fairy lights": "Schnee und Lichterkette", "Midnight": "Mitternacht", "Milky Way": "Milchstraße", "Ocean": "Ozean", "Under water": "Unter Wasser", "Falling petals": "Fallende Blüten", "Sunset": "Sonnenuntergang", "Evening sky": "Abendhimmel",
  "Your name": "Dein Name", "Shown at the top left": "Steht oben links", "Updates": "Updates", "Check": "Suchen", "Install": "Installieren", "Installed": "Installiert", "New": "Neu", "Restarting …": "Startet neu …",
  "Update found": "Update gefunden", "Everything is up to date": "Alles ist aktuell", "Nothing published yet.": "Noch nichts veröffentlicht.", "GitHub could not be reached": "GitHub ist nicht erreichbar",
  "You already have the newest version": "Du hast schon die neueste Version",
  // settings
  "General": "Allgemein", "How the app behaves": "Wie sich die App verhält", "Everything in the app, including the tray menu": "Alles in der App, auch das Menü neben der Uhr",
  "Start with Windows": "Mit Windows starten", "Opens by itself when you log in": "Öffnet sich von selbst, wenn du dich anmeldest",
  "Start in the background": "Im Hintergrund starten", "At Windows start only the icon next to the clock appears": "Beim Windows-Start erscheint nur das Icon neben der Uhr",
  "Keep running when closed": "Beim Schließen weiterlaufen", "Otherwise the X quits the app and stops clipping": "Sonst beendet das X die App und das Clippen",
  "Recording settings": "Aufnahme-Einstellungen", "For clips and long recordings": "Für Clips und lange Aufzeichnungen",
  "Microphone": "Mikrofon", "Records your voice along with the PC sound": "Nimmt deine Stimme zusammen mit dem PC-Sound auf", "Sound when saved": "Ton beim Speichern", "Short tone, so you know it worked": "Kurzer Ton, damit du weißt, dass es geklappt hat",
  "Frames per second": "Bilder pro Sekunde", "60 is smoother, 30 makes smaller files": "60 ist flüssiger, 30 macht kleinere Dateien", "Quality": "Qualität", "Higher looks better but needs more space": "Höher sieht besser aus, braucht aber mehr Platz",
  "Small": "Klein", "Normal": "Normal", "High": "Hoch", "Save folder": "Speicherordner", "Change": "Ändern",
  "Starts with Windows now": "Startet jetzt mit Windows", "Not possible while recording": "Geht nicht während einer Aufnahme",
  // V2: sidebar
  "Notes": "Notizen", "Games & media": "Spiele & Medien", "Tools": "Tools", "Themes": "Themes",
  "Collapse sidebar": "Seitenleiste einklappen", "Expand sidebar": "Seitenleiste ausklappen",
  "Crosshair and ping": "Fadenkreuz und Ping", "What's playing and the volume of every app": "Was läuft und die Lautstärke jeder App",
  "Sticky notes that stay on top of everything": "Notizzettel, die über allem bleiben", "Now playing and volume per app": "Läuft gerade und Lautstärke pro App",
  "Sticky notes on top of everything": "Notizzettel über allem",
  // V2: gaming
  "Crosshair": "Fadenkreuz", "Sits in the middle of your screen. Works in borderless and windowed games.": "Sitzt in der Mitte deines Bildschirms. Klappt in Spielen im Fenster- oder randlosen Modus.",
  "Shape": "Form", "Cross": "Kreuz", "Circle": "Kreis", "Dot": "Punkt", "Color": "Farbe", "Any color": "Beliebige Farbe", "Size": "Größe", "Thickness": "Dicke",
  "Gap": "Abstand", "Opacity": "Deckkraft", "Center dot": "Mittelpunkt", "Outline": "Umrandung", "turns it on and off in game": "schaltet es im Spiel an und aus",
  "Crosshair on": "Fadenkreuz an", "Crosshair off": "Fadenkreuz aus", "Invalid color": "Ungültige Farbe", "Invalid value": "Ungültiger Wert",
  "How fast servers answer, measured every 2 seconds": "Wie schnell Server antworten, alle 2 Sekunden gemessen", "Add a server, like 1.1.1.1": "Server hinzufügen, z. B. 1.1.1.1",
  "Add": "Hinzufügen", "Remove": "Entfernen", "Add a server to see its ping.": "Füge einen Server hinzu, um seinen Ping zu sehen.", "no answer": "keine Antwort",
  "That doesn't look like a server address": "Das sieht nicht wie eine Serveradresse aus", "Already in the list": "Schon in der Liste", "12 servers at most": "Höchstens 12 Server",
  // V2: sound
  "Now playing": "Läuft gerade", "Nothing is playing": "Gerade läuft nichts", "Start a song in Spotify or anywhere else.": "Starte einen Song in Spotify oder woanders.",
  "Previous": "Zurück", "Next": "Weiter", "Volume": "Lautstärke", "All sounds": "Alle Sounds", "System sounds": "Systemsounds", "Mute": "Stumm", "Unmute": "Ton an",
  "No app is playing sound right now.": "Gerade spielt keine App Ton ab.", "Play": "Abspielen",
  "No key": "Keine Taste", "Another app already uses this key": "Eine andere App benutzt diese Taste schon",
  "That key is already used in this app.": "Die Taste wird in der App schon benutzt.", "Sound not found": "Sound nicht gefunden",
  // V2: notes
  "New note": "Neue Notiz", "Click-through": "Durchklicken", "Notes stay visible, but the mouse goes to the game below": "Notizen bleiben sichtbar, aber die Maus geht ans Spiel darunter",
  "No notes yet. A new note appears at the top right of your screen.": "Noch keine Notizen. Eine neue Notiz erscheint oben rechts auf dem Bildschirm.",
  "Empty note": "Leere Notiz", "Hide from screen": "Vom Bildschirm ausblenden", "Show on screen": "Auf dem Bildschirm zeigen", "Delete this note?": "Diese Notiz löschen?",
  "Notes are not kept in the Recycle Bin, it is gone for good.": "Notizen landen nicht im Papierkorb, sie ist dann endgültig weg.", "just now": "gerade eben",
  "Notes: click-through on": "Notizen: Durchklicken an", "Notes: click-through off": "Notizen: Durchklicken aus", "That's a lot of notes. Delete one first.": "Das sind viele Notizen. Lösch erst eine.",
  "Note not found": "Notiz nicht gefunden", "Checklist": "Checkliste", "Color and opacity": "Farbe und Deckkraft", "Hide (it stays in the app)": "Ausblenden (bleibt in der App)", "Write something …": "Schreib etwas …",
  // V2: hotkeys
  "Hotkeys": "Tasten", "Every key of the app in one place. Click a key, then press the new one. Backspace = no key.": "Alle Tasten der App an einem Ort. Auf eine Taste klicken, dann die neue drücken. Rücktaste = keine Taste.",
  "Start / stop recording": "Aufnahme starten / stoppen", "Crosshair on / off": "Fadenkreuz an / aus", "Notes click-through": "Notizen durchklicken",
  "Talk to Jarvis": "Mit Jarvis sprechen", "Hold it while you talk": "Beim Sprechen gedrückt halten", "Not set up": "Nicht eingerichtet",
  // V2: downloads
  "Sorts loose files into folders by type. Unknown files stay where they are.": "Sortiert lose Dateien nach Typ in Ordner. Unbekannte Dateien bleiben, wo sie sind.",
  "Sort now": "Jetzt sortieren", "Sort automatically": "Automatisch sortieren", "Every 10 minutes. Files that are still downloading are left alone.": "Alle 10 Minuten. Dateien, die noch laden, bleiben unberührt.",
  "Images": "Bilder", "Music": "Musik", "Documents": "Dokumente", "Archives": "Archive", "Installers": "Installer", "Everything is sorted": "Alles ist sortiert",
  "Old installers, probably already installed": "Alte Installer, wahrscheinlich schon installiert", "Nothing to sort": "Nichts zu sortieren", "Nothing to undo": "Nichts rückgängig zu machen",
};
const DE_RULES = [
  [/^(· )?avg (\d+) ms · jitter (\d+) ms(?: · (\d+) lost)?$/, (_, a, b, c, d) => `${a || ""}Ø ${b} ms · Schwankung ${c} ms` + (d ? ` · ${d} verloren` : "")],
  [/^(\d+) days old$/, "$1 Tage alt"],
  [/^Screen capture stopped, trying again in (\d+) s$/, "Bildschirmaufnahme gestoppt, neuer Versuch in $1 s"],
  [/^Sorted (\d+) files?$/, (_, n) => `${n} ${n === "1" ? "Datei" : "Dateien"} sortiert`], [/^Put back (\d+) files?$/, (_, n) => `${n} ${n === "1" ? "Datei" : "Dateien"} zurückgelegt`],
  [/^(\d+) min ago$/, "vor $1 Min."], [/^(\d+) h ago$/, "vor $1 Std."],
  [/^(V[\d.]+) installed$/, "$1 installiert"], [/^(V[\d.]+) is available$/, "$1 ist verfügbar"], [/^(V[\d.]+) is up to date$/, "$1 ist aktuell"],
  [/^Settings could not be saved (.+)$/, "Einstellungen konnten nicht gespeichert werden $1"], [/^Downloading recorder … (\d+)%$/, "Lädt den Rekorder … $1 %"], [/^Last (.+)$/, "Letzte $1"], [/^The app folder is write-protected (.+)$/, "Der App-Ordner ist schreibgeschützt $1"],
  [/^(\d+) on$/, "$1 an"], [/^(\d+) off$/, "$1 aus"], [/^(\d+) leftover$/, "$1 Reste"], [/^(\d+) of (\d+)$/, "$1 von $2"],
  [/^(\d+) turned off(?: · (\d+) leftover)?$/, (_, a, b) => `${a} ausgeschaltet` + (b ? ` · ${b} Reste` : "")],
  [/^(.+ [GM]B) of it risk-free$/, "davon $1 ohne Risiko"], [/^(.+ [GM]B) of (.+ [GM]B)$/, "$1 von $2"],
  [/^Freed (.+)$/, "$1 freigemacht"], [/^Partly failed: (.+)$/, "Teilweise fehlgeschlagen: $1"], [/^Scan done in (\d+)s$/, "Scan fertig in $1 s"],
  [/^Older than 7 days, no app needs them anymore(.*)$/, (_, a) => "Älter als 7 Tage, keine App braucht sie mehr" + a.replace("items", "Dateien")],
  [/^Leftovers from apps that crashed(.*)$/, (_, a) => "Reste von abgestürzten Apps" + a.replace("items", "Dateien")],
  [/^Clips older than 3 months(.*)$/, (_, a) => "Clips älter als 3 Monate" + a.replace("items", "Dateien")],
  [/^\.exe \/ \.msi older than 2 weeks, usually already installed(.*)$/, (_, a) => ".exe / .msi älter als 2 Wochen, meist schon installiert" + a.replace("items", "Dateien")],
  [/^Active is (\S+)\. Old: (.*)$/, (_, a, b) => `Aktiv ist ${a}. Alt: ` + b.replace("items", "Ordner")],
  [/^New length (.+)$/, "Neue Länge $1"], [/^Saving … (\d+)%$/, "Speichert … $1 %"], [/^Could not save the edit\.(.*)$/, "Schnitt konnte nicht gespeichert werden.$1"],
  [/^Drive ([A-Z]:)$/, "Laufwerk $1"], [/^Hold (?!your )(.+) and talk\.$/, "Halte $1 und sprich."],
  [/^Hold your (\w+) mouse button and talk\.$/, (_, b) => `Halte deine ${{ back: "Zurück-Maustaste", forward: "Vorwärts-Maustaste", middle: "mittlere Maustaste" }[b] || b} und sprich.`],
  [/^Saved · ends in (.+)$/, "Gespeichert · endet auf $1"], [/^No microphone access: (.+)$/, "Kein Mikrofonzugriff: $1"], [/^Python not found (.+)$/, "Python nicht gefunden $1"],
  [/^(.+) (?:is|are) already used by another app$/, "$1 wird schon von einer anderen App benutzt"],
  [/^Recording ([\d:]+)$/, "Aufnahme $1"], [/^(\d+) clips? · (.+)$/, "$1 Clips · $2"], [/^(Clip|Recording) · (.+)$/, (_, a, b) => (a === "Clip" ? "Clip" : "Aufnahme") + " · " + b],
  [/^Screen capture stopped, retrying\.(.*)$/, "Bildschirmaufnahme gestoppt, versucht es erneut.$1"], [/^Clip could not be saved\.(.*)$/, "Clip konnte nicht gespeichert werden.$1"], [/^Recorder did not start: (.+)$/, "Rekorder ist nicht gestartet: $1"],
];
function tr(s) {
  if (LANG !== "de") return s;
  if (Object.hasOwn(DE, s)) return DE[s];
  for (const [re, to] of DE_RULES) if (re.test(s)) return s.replace(re, to);
  return s;
}
if (LANG === "de") {
  document.documentElement.lang = "de";
  // names of apps, folders and what you said are never touched
  const skip = ".raw, .ping .name, #folder-list .row, .j-line:not(.idle), #auto-list .sub, .app-ic, #p-name, #m-list, #brand, .u-notes, .d-name, script, style";
  const text = (n) => {
    const raw = n.nodeValue, t = raw.trim();
    if (!t || n.parentElement?.closest(skip) || n.parentElement?.matches("#auto-list .name")) return;
    const d = tr(t);
    if (d !== t) n.nodeValue = raw.replace(t, () => d);
  };
  const attrs = (el) => { for (const a of ["title", "placeholder"]) { const v = el.getAttribute?.(a); if (v && tr(v) !== v) el.setAttribute(a, tr(v)); } };
  const walk = (root) => {
    if (root.nodeType === 3) return text(root);
    if (root.nodeType !== 1) return;
    attrs(root);
    root.querySelectorAll("[title],[placeholder]").forEach(attrs);
    const w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    while (w.nextNode()) text(w.currentNode);
  };
  walk(document.body);
  new MutationObserver((list) => {
    for (const m of list) {
      if (m.type === "characterData") text(m.target);
      else m.addedNodes.forEach(walk);
    }
  }).observe(document.body, { subtree: true, childList: true, characterData: true });
}
