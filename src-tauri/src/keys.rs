// Global hotkeys for the V2 features (crosshair, notes). Clips keep their own.
// One thread owns all of them, because Windows ties a hotkey to the thread that registered it.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use tauri::AppHandle;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_APP, WM_HOTKEY};

type Action = Arc<dyn Fn() + Send + Sync>;
static KEYS: Mutex<Vec<(String, String, Action)>> = Mutex::new(Vec::new());
static THREAD: AtomicU32 = AtomicU32::new(0);
static APP: OnceLock<AppHandle> = OnceLock::new();

pub fn app() -> Option<&'static AppHandle> { APP.get() }

/// binds `key` (like "Ctrl+Shift+X") to an action under a name; an empty key removes it
pub fn set(name: &str, key: &str, f: impl Fn() + Send + Sync + 'static) {
    {
        let mut k = KEYS.lock().unwrap_or_else(|e| e.into_inner());
        k.retain(|(n, _, _)| n != name);
        if !key.is_empty() { k.push((name.into(), key.into(), Arc::new(f))); }
    }
    refresh();
}
/// keys that are taken by V2 features, so two features never share one
pub fn taken(except: &str) -> Vec<String> {
    KEYS.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|(n, _, _)| n != except).map(|(_, k, _)| k.clone()).collect()
}

fn refresh() {
    let t = THREAD.load(SeqCst);
    if t != 0 { unsafe { let _ = PostThreadMessageW(t, WM_APP, Default::default(), Default::default()); } }
}

pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    thread::spawn(|| unsafe {
        THREAD.store(GetCurrentThreadId(), SeqCst);
        let mut ids: HashMap<i32, Action> = HashMap::new();
        let register = |ids: &mut HashMap<i32, Action>| {
            for id in ids.keys() { let _ = UnregisterHotKey(None, *id); }
            ids.clear();
            let list = KEYS.lock().unwrap_or_else(|e| e.into_inner()).clone();
            for (i, (_, key, f)) in list.into_iter().enumerate() {
                let id = 1000 + i as i32;
                match crate::clips::parse_key(&key) {
                    Some((m, vk)) if RegisterHotKey(None, id, m | MOD_NOREPEAT, vk).is_ok() => { ids.insert(id, f); }
                    _ => {}
                }
            }
        };
        register(&mut ids);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_HOTKEY {
                if let Some(f) = ids.get(&(msg.wParam.0 as i32)).cloned() { thread::spawn(move || f()); }
            } else if msg.message == WM_APP { register(&mut ids); }
        }
    });
}
