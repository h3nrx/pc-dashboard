// Windows on top of everything: the crosshair and the sticky notes.
// Both are plain transparent windows (no game is touched), so they work in borderless /
// windowed-fullscreen games, not in exclusive fullscreen.
use crate::clips::data;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Cross { on: bool, shape: String, color: String, size: f32, thick: f32, gap: f32, dot: bool, outline: bool, opacity: f32, hotkey: String }
impl Default for Cross { fn default() -> Self { Cross {
    on: false, shape: "cross".into(), color: "#00ff9c".into(), size: 9.0, thick: 2.0, gap: 4.0, dot: false, outline: true, opacity: 1.0, hotkey: "Ctrl+Shift+X".into(),
} } }

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Note { id: String, text: String, color: String, x: i32, y: i32, w: u32, h: u32, shown: bool, opacity: f32, list: bool, updated: u64 }
impl Default for Note { fn default() -> Self { Note {
    id: String::new(), text: String::new(), color: "graphite".into(), x: -1, y: -1, w: 280, h: 240, shown: true, opacity: 0.94, list: false, updated: 0,
} } }

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Cfg { cross: Cross, notes: Vec<Note>, ghost_key: String }
impl Default for Cfg { fn default() -> Self { Cfg { cross: Cross::default(), notes: vec![], ghost_key: "Ctrl+Shift+N".into() } } }

static CFG: Mutex<Option<Cfg>> = Mutex::new(None);
static DIRTY: AtomicBool = AtomicBool::new(false);
static GHOST: AtomicBool = AtomicBool::new(false);

fn path() -> std::path::PathBuf { data().join("overlay.json") }
fn cfg<R>(f: impl FnOnce(&mut Cfg) -> R) -> R {
    let mut g = CFG.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(|| fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()))
}
fn save() -> Result<(), String> {
    DIRTY.store(false, SeqCst);
    let text = cfg(|c| serde_json::to_string_pretty(c).unwrap_or_default());
    let _ = fs::create_dir_all(data());
    let tmp = path().with_extension("json.tmp");
    fs::write(&tmp, &text).map_err(|e| format!("Could not save: {e}"))?;
    fs::rename(&tmp, path()).map_err(|e| format!("Could not save: {e}"))
}
fn now() -> u64 { SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0) }
fn app() -> Result<&'static AppHandle, String> { crate::keys::app().ok_or_else(|| "not ready".to_string()) }

/// a patch from the UI goes through serde, so unknown or wrong fields can't slip in
fn merge<T: Serialize + for<'a> Deserialize<'a>>(cur: &T, patch: serde_json::Value) -> Result<T, String> {
    let mut v = serde_json::to_value(cur).map_err(|e| e.to_string())?;
    if let (Some(o), Some(p)) = (v.as_object_mut(), patch.as_object()) {
        for (k, x) in p { if o.contains_key(k) && k != "id" { o.insert(k.clone(), x.clone()); } }
    }
    serde_json::from_value(v).map_err(|_| "Invalid value".to_string())
}

fn hwnd(w: &WebviewWindow) -> Option<HWND> { w.hwnd().ok().map(|h| HWND(h.0 as _)) }
/// never takes the keyboard from the game and never shows up in Alt+Tab
fn no_activate(w: &WebviewWindow) {
    if let Some(h) = hwnd(w) {
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize);
        }
    }
}

/* ---------- crosshair ---------- */
const CROSS: &str = "crosshair";

fn cross_apply(app: &AppHandle) -> Result<(), String> {
    let c = cfg(|c| c.cross.clone());
    if !c.on {
        if let Some(w) = app.get_webview_window(CROSS) { let _ = w.close(); }
        return Ok(());
    }
    let w = match app.get_webview_window(CROSS) {
        Some(w) => w,
        None => {
            let w = WebviewWindowBuilder::new(app, CROSS, WebviewUrl::App("crosshair.html".into()))
                .title("Crosshair").transparent(true).decorations(false).always_on_top(true).skip_taskbar(true)
                .resizable(false).focused(false).shadow(false).inner_size(240.0, 240.0).visible(false)
                .build().map_err(|e| e.to_string())?;
            let _ = w.set_ignore_cursor_events(true);
            no_activate(&w);
            w
        }
    };
    // exactly in the middle of the main screen
    if let Ok(Some(m)) = app.primary_monitor() {
        let (mp, ms) = (m.position(), m.size());
        if let Ok(sz) = w.outer_size() {
            let _ = w.set_position(PhysicalPosition::new(mp.x + (ms.width as i32 - sz.width as i32) / 2, mp.y + (ms.height as i32 - sz.height as i32) / 2));
        }
    }
    let _ = w.show();
    let _ = app.emit_to(CROSS, "cross", &c);
    Ok(())
}

fn cross_keys() {
    let key = cfg(|c| c.cross.hotkey.clone());
    crate::keys::set("crosshair", &key, || {
        cfg(|c| c.cross.on = !c.cross.on);
        let _ = save();
        if let Ok(a) = app() {
            let a2 = a.clone();
            let _ = a.run_on_main_thread(move || { let _ = cross_apply(&a2); });
            let _ = a.emit("cross-state", cfg(|c| c.cross.clone()));
        }
    });
}

#[tauri::command]
pub fn cross_get() -> Cross { cfg(|c| c.cross.clone()) }

#[tauri::command]
pub async fn cross_set(app: AppHandle, patch: serde_json::Value) -> Result<Cross, String> {
    let mut n: Cross = merge(&cfg(|c| c.cross.clone()), patch)?;
    n.size = n.size.clamp(1.0, 60.0); n.thick = n.thick.clamp(1.0, 12.0); n.gap = n.gap.clamp(0.0, 40.0); n.opacity = n.opacity.clamp(0.1, 1.0);
    if !["cross", "dot", "circle", "t", "x"].contains(&n.shape.as_str()) { n.shape = "cross".into(); }
    if !(n.color.len() == 7 && n.color.starts_with('#') && n.color[1..].chars().all(|c| c.is_ascii_hexdigit())) { return Err("Invalid color".into()); }
    if !n.hotkey.is_empty() {
        if !crate::clips::usable_key(&n.hotkey) { return Err("That key can't be used. Try F9 or Alt + a letter.".into()); }
        if crate::clips::own_keys().contains(&n.hotkey) || crate::keys::taken("crosshair").contains(&n.hotkey) { return Err("That key is already used in this app.".into()); }
    }
    cfg(|c| c.cross = n.clone());
    save()?;
    cross_keys();
    let a = app.clone();
    let _ = app.run_on_main_thread(move || { let _ = cross_apply(&a); });
    Ok(n)
}

/* ---------- notes ---------- */
fn label(id: &str) -> String { format!("note-{id}") }

fn note_open(app: &AppHandle, n: &Note) -> Result<(), String> {
    let l = label(&n.id);
    if let Some(w) = app.get_webview_window(&l) { let _ = w.show(); return Ok(()); }
    let w = WebviewWindowBuilder::new(app, &l, WebviewUrl::App(format!("note.html?id={}", n.id).into()))
        .title("Note").transparent(true).decorations(false).always_on_top(true).skip_taskbar(true)
        .resizable(true).focused(false).shadow(false).inner_size(280.0, 240.0).min_inner_size(170.0, 110.0).visible(false)
        .build().map_err(|e| e.to_string())?;
    // new notes go to the top right of the main screen, next to each other
    let (mut x, mut y) = (n.x, n.y);
    if x < 0 || y < 0 {
        if let Ok(Some(m)) = app.primary_monitor() {
            let k = cfg(|c| c.notes.iter().position(|o| o.id == n.id).unwrap_or(0)) as i32 % 4;
            x = m.position().x + m.size().width as i32 - n.w as i32 - 40 - k * (n.w as i32 + 16);
            y = m.position().y + 60;
        }
    }
    let _ = w.set_size(PhysicalSize::new(n.w, n.h));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    if GHOST.load(SeqCst) { let _ = w.set_ignore_cursor_events(true); }
    let _ = w.show();
    Ok(())
}

/// called for every move / resize of a note window
pub fn note_moved(w: &tauri::Window) {
    let Some(id) = w.label().strip_prefix("note-").map(String::from) else { return };
    let (Ok(p), Ok(s)) = (w.outer_position(), w.outer_size()) else { return };
    if s.width < 50 || s.height < 50 { return; } // minimized
    cfg(|c| if let Some(n) = c.notes.iter_mut().find(|n| n.id == id) { n.x = p.x; n.y = p.y; n.w = s.width; n.h = s.height; });
    DIRTY.store(true, SeqCst);
}
/// the X of a note only hides it; it stays in the list
pub fn note_closed(w: &tauri::Window) {
    let Some(id) = w.label().strip_prefix("note-").map(String::from) else { return };
    let _ = w.hide();
    cfg(|c| if let Some(n) = c.notes.iter_mut().find(|n| n.id == id) { n.shown = false; });
    let _ = save();
    if let Ok(a) = app() { let _ = a.emit("notes", ()); }
}

#[derive(Serialize)]
pub struct NotesState { items: Vec<Note>, ghost: bool, ghost_key: String }
fn notes_state() -> NotesState { cfg(|c| NotesState { items: c.notes.clone(), ghost: GHOST.load(SeqCst), ghost_key: c.ghost_key.clone() }) }

#[tauri::command]
pub fn notes_list() -> NotesState { notes_state() }

#[tauri::command]
pub fn note_get(id: String) -> Option<Note> { cfg(|c| c.notes.iter().find(|n| n.id == id).cloned()) }

#[tauri::command]
pub async fn note_new(app: AppHandle) -> Result<Note, String> {
    if cfg(|c| c.notes.len()) >= 30 { return Err("That's a lot of notes. Delete one first.".into()); }
    let n = Note { id: format!("{:x}", now()), updated: now(), ..Default::default() };
    cfg(|c| c.notes.push(n.clone()));
    save()?;
    let (a, n2) = (app.clone(), n.clone());
    let _ = app.run_on_main_thread(move || { let _ = note_open(&a, &n2); });
    let _ = app.emit("notes", ());
    Ok(n)
}

#[tauri::command]
pub async fn note_save(app: AppHandle, id: String, patch: serde_json::Value) -> Result<Note, String> {
    let cur = cfg(|c| c.notes.iter().find(|n| n.id == id).cloned()).ok_or("Note not found")?;
    let mut n: Note = merge(&cur, patch)?;
    n.id = id.clone();
    if n.text.len() > 20_000 { n.text.truncate(n.text.char_indices().take_while(|(i, _)| *i < 20_000).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(0)); }
    n.opacity = n.opacity.clamp(0.35, 1.0);
    if !["graphite", "amber", "mint", "sky", "rose", "violet"].contains(&n.color.as_str()) { n.color = "graphite".into(); }
    n.updated = now();
    let show_changed = n.shown != cur.shown;
    cfg(|c| if let Some(x) = c.notes.iter_mut().find(|x| x.id == id) { *x = n.clone(); });
    save()?;
    if show_changed {
        let (a, n2) = (app.clone(), n.clone());
        let _ = app.run_on_main_thread(move || {
            if n2.shown { let _ = note_open(&a, &n2); } else if let Some(w) = a.get_webview_window(&label(&n2.id)) { let _ = w.hide(); }
        });
    }
    let _ = app.emit("note", &n);
    let _ = app.emit("notes", ());
    Ok(n)
}

#[tauri::command]
pub async fn note_delete(app: AppHandle, id: String) -> Result<(), String> {
    cfg(|c| c.notes.retain(|n| n.id != id));
    save()?;
    if let Some(w) = app.get_webview_window(&label(&id)) { let _ = w.close(); }
    let _ = app.emit("notes", ());
    Ok(())
}

/// click-through: notes stay visible, but the mouse goes straight to the game below
fn set_ghost(app: &AppHandle, on: bool) {
    GHOST.store(on, SeqCst);
    for (l, w) in app.webview_windows() { if l.starts_with("note-") { let _ = w.set_ignore_cursor_events(on); } }
    let _ = app.emit("ghost", on);
}

#[tauri::command]
pub fn notes_ghost(app: AppHandle, on: bool) -> NotesState { set_ghost(&app, on); notes_state() }

#[tauri::command]
pub fn notes_key(key: String) -> Result<NotesState, String> {
    if !key.is_empty() {
        if !crate::clips::usable_key(&key) { return Err("That key can't be used. Try F9 or Alt + a letter.".into()); }
        if crate::clips::own_keys().contains(&key) || crate::keys::taken("notes").contains(&key) { return Err("That key is already used in this app.".into()); }
    }
    cfg(|c| c.ghost_key = key);
    save()?;
    ghost_keys();
    Ok(notes_state())
}
fn ghost_keys() {
    let key = cfg(|c| c.ghost_key.clone());
    crate::keys::set("notes", &key, || { if let Ok(a) = app() { set_ghost(a, !GHOST.load(SeqCst)); } });
}

pub fn start(app: &AppHandle) {
    cross_keys();
    ghost_keys();
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        let _ = cross_apply(&a);
        for n in cfg(|c| c.notes.clone()).iter().filter(|n| n.shown) { let _ = note_open(&a, n); }
    });
    // window positions are saved a moment after moving stops, not on every pixel
    thread::spawn(|| loop {
        thread::sleep(Duration::from_secs(2));
        if DIRTY.load(SeqCst) { let _ = save(); }
    });
    // some games put themselves on top; the crosshair takes its place back
    thread::spawn(|| loop {
        thread::sleep(Duration::from_secs(2));
        let Some(a) = crate::keys::app() else { continue };
        if let Some(h) = a.get_webview_window(CROSS).and_then(|w| hwnd(&w)) {
            unsafe { let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE); }
        }
    });
}
