// AirPlay: this PC shows up as a screen in the iPad's / iPhone's "Screen Mirroring" list.
// Announced on the Wi-Fi (mDNS / Bonjour); rtsp.rs talks to the iPad, mirror.rs receives the picture.
mod display;
mod fairplay;
mod mirror;
mod pin;
mod rtsp;
mod sound;
use crate::{blocking, clips::data};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const PORTS: [u16; 3] = [7000, 7100, 7200];
const FEATURES: &str = "0x5A7FFFF7,0x1E";
const FEATURES_NUM: u64 = 0x1E << 32 | 0x5A7FFFF7;
const MODEL: &str = "AppleTV3,2";
const SRCVERS: &str = "220.68";
/// the screen we offer; the iPad keeps its own shape inside it
const DISPLAY: (u64, u64) = (1920, 1080);

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Cfg { on: bool, name: String, id: String, key: String, on_top: bool, pin: bool, known: Vec<String> }
impl Default for Cfg {
    fn default() -> Self { Cfg { on: false, name: "PC Dashboard".into(), id: String::new(), key: String::new(), on_top: false, pin: true, known: vec![] } }
}

#[derive(Clone, Serialize)]
pub struct ApState { on: bool, name: String, live: bool, port: u16, device: String, error: String, mirroring: bool, size: String, pin: bool, code: String, paired: usize }

struct Run { mdns: ServiceDaemon, names: Vec<String>, port: u16 }
struct State { cfg: Cfg, run: Option<Run>, device: String, error: String, mirror: Option<(String, u32, u32)>, code: String }
static ST: Mutex<Option<State>> = Mutex::new(None);
static APP: Mutex<Option<AppHandle>> = Mutex::new(None);
/// the listener lives for the whole app run; it only answers while AirPlay is on
static LISTEN: Mutex<Option<u16>> = Mutex::new(None);

fn cfg_path() -> PathBuf { data().join("airplay.json") }
fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(|| {
        let mut cfg: Cfg = fs::read_to_string(cfg_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        // a fixed identity, so the iPad remembers this PC
        if cfg.id.len() != 17 || cfg.key.len() != 64 {
            let mut b = [0u8; 38];
            let _ = getrandom::getrandom(&mut b);
            b[0] = (b[0] & 0xFE) | 0x02; // locally administered address, never a real network card
            cfg.id = b[..6].iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(":");
            cfg.key = b[6..].iter().map(|x| format!("{x:02x}")).collect();
            let _ = save(&cfg);
        }
        State { cfg, run: None, device: String::new(), error: String::new(), mirror: None, code: String::new() }
    }))
}
fn save(cfg: &Cfg) -> Result<(), String> {
    let _ = fs::create_dir_all(data());
    let tmp = cfg_path().with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(cfg).unwrap_or_default()).and_then(|_| fs::rename(&tmp, cfg_path()))
        .map_err(|e| format!("Settings could not be saved ({e})"))
}
fn emit() {
    if let Some(a) = APP.lock().unwrap_or_else(|e| e.into_inner()).as_ref() { let _ = a.emit("airplay", ()); }
}
fn now() -> ApState {
    st(|s| ApState {
        on: s.cfg.on, name: s.cfg.name.clone(), live: s.run.is_some(), port: s.run.as_ref().map_or(0, |r| r.port),
        device: s.device.clone(), error: s.error.clone(),
        mirroring: s.mirror.is_some(), size: s.mirror.as_ref().map(|m| format!("{} × {}", m.1, m.2)).unwrap_or_default(),
        pin: s.cfg.pin, code: s.code.clone(), paired: s.cfg.known.len(),
    })
}

fn signing_key(seed_hex: &str) -> ed25519_dalek::SigningKey {
    let mut seed = [0u8; 32];
    for (i, b) in seed.iter_mut().enumerate() { *b = u8::from_str_radix(seed_hex.get(i * 2..i * 2 + 2).unwrap_or("00"), 16).unwrap_or(0); }
    ed25519_dalek::SigningKey::from_bytes(&seed)
}
/// who this PC is on the network
pub(crate) struct Me { key: ed25519_dalek::SigningKey, id: String, name: String, pi: String, txt_airplay: Vec<(String, String)>, txt_raop: Vec<(String, String)> }
fn me() -> Me {
    let cfg = st(|s| s.cfg.clone());
    let key = signing_key(&cfg.key);
    let pk: String = key.verifying_key().to_bytes().iter().map(|x| format!("{x:02x}")).collect();
    let pi = format!("{}-{}-4{}-a{}-{}", &cfg.key[..8], &cfg.key[8..12], &cfg.key[13..16], &cfg.key[17..20], &cfg.key[20..32]);
    let own = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
    // with a PIN the iPad asks for it (pw = true, sf = 0x8c)
    let (pw, sf) = if cfg.pin { ("true", "0x8c") } else { ("false", "0x4") };
    let txt_airplay = own(&[("deviceid", &cfg.id), ("features", FEATURES), ("pw", pw), ("flags", "0x4"), ("model", MODEL), ("pk", &pk), ("pi", &pi), ("srcvers", SRCVERS), ("vv", "2")]);
    let txt_raop = own(&[
        ("ch", "2"), ("cn", "0,1,2,3"), ("da", "true"), ("et", "0,3,5"), ("vv", "2"), ("ft", FEATURES), ("am", MODEL),
        ("md", "0,1,2"), ("rhd", "5.1.14"), ("pw", pw), ("sr", "44100"), ("ss", "16"), ("sv", "false"),
        ("tp", "UDP"), ("txtvers", "1"), ("sf", sf), ("vs", SRCVERS), ("vn", "65537"), ("pk", &pk),
    ]);
    Me { key, id: cfg.id, name: cfg.name, pi, txt_airplay, txt_raop }
}
fn live() -> bool { st(|s| s.run.is_some()) }
fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
fn pin_mode() -> bool { st(|s| s.cfg.pin) }
/// devices that entered the PIN once
fn is_known(pk: &[u8; 32]) -> bool { let h = hex(pk); st(|s| s.cfg.known.contains(&h)) }
fn remember(pk: &[u8; 32]) {
    let h = hex(pk);
    let cfg = st(|s| { if !s.cfg.known.contains(&h) { s.cfg.known.push(h); } s.cfg.known.truncate(50); s.cfg.clone() });
    let _ = save(&cfg);
}
/// shows the PIN big in the app (and brings the app to the front)
fn show_pin(code: &str) {
    st(|s| s.code = code.to_string());
    emit();
    if !code.is_empty() {
        if let Some(a) = APP.lock().unwrap_or_else(|e| e.into_inner()).clone() { crate::show_main(&a); let _ = a.emit("airplay-pin", ()); }
    }
    // a code nobody typed in goes away after 2 minutes
    if !code.is_empty() {
        let c = code.to_string();
        thread::spawn(move || { thread::sleep(Duration::from_secs(120)); if current_pin() == c { show_pin(""); } });
    }
}
fn current_pin() -> String { st(|s| s.code.clone()) }
fn set_device(name: &str) { st(|s| s.device = name.to_string()); emit(); }
fn set_mirroring(m: Option<(String, u32, u32)>) { st(|s| s.mirror = m); emit(); }
/// what the picture is handed to: the window, or (testing only, PCD_AIRPLAY_DUMP=<file>) a file
fn sink(device: String, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Box<dyn mirror::Sink> {
    set_mirroring(Some((device.clone(), 0, 0)));
    if let Some(file) = std::env::var("PCD_AIRPLAY_DUMP").ok().and_then(|p| fs::File::create(p).ok()) {
        return Box::new(mirror::Probe { device, file: Some(file), frames: 0, size: (0, 0) });
    }
    Box::new(Tell { device: device.clone(), inner: display::Player::new(device, stop, st(|s| s.cfg.on_top)) })
}
/// passes the picture on and keeps the tab up to date
struct Tell { device: String, inner: display::Player }
impl mirror::Sink for Tell {
    fn format(&mut self, w: u32, h: u32) { set_mirroring(Some((self.device.clone(), w, h))); self.inner.format(w, h); }
    fn frame(&mut self, b: &[u8]) { self.inner.frame(b); }
}
/// the control connection of the session that is showing; closing it ends the mirroring
static SESSION: Mutex<Option<std::net::TcpStream>> = Mutex::new(None);
fn set_session(s: &std::net::TcpStream) { *SESSION.lock().unwrap_or_else(|e| e.into_inner()) = s.try_clone().ok(); }
fn kick() {
    if let Some(s) = SESSION.lock().unwrap_or_else(|e| e.into_inner()).take() { let _ = s.shutdown(std::net::Shutdown::Both); }
}
fn remember_on_top(on: bool) {
    let cfg = st(|s| { s.cfg.on_top = on; s.cfg.clone() });
    let _ = save(&cfg);
}

/* ---------- network ---------- */
fn listen() -> Result<u16, String> {
    let mut g = LISTEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(p) = *g { return Ok(p); }
    let (l, port) = PORTS.iter().find_map(|p| TcpListener::bind(("0.0.0.0", *p)).ok().map(|l| (l, *p)))
        .ok_or("The AirPlay port is used by another program")?;
    thread::spawn(move || {
        for c in l.incoming().flatten() {
            if !st(|s| s.run.is_some()) { continue; } // off: the iPad simply finds nothing
            thread::spawn(move || { let _ = rtsp::serve(c); });
        }
    });
    *g = Some(port);
    Ok(port)
}

fn announce(cfg: &Cfg, port: u16) -> Result<Run, String> {
    let mdns = ServiceDaemon::new().map_err(|e| format!("Could not reach the network ({e})"))?;
    let raw_id = cfg.id.replace(':', "");
    let me = me();
    let host = format!("pcd-{}.local.", raw_id.to_ascii_lowercase());
    let as_ref = |v: &Vec<(String, String)>| v.iter().map(|(a, b)| (a.clone(), b.clone())).collect::<std::collections::HashMap<_, _>>();
    let (airplay, raop) = (as_ref(&me.txt_airplay), as_ref(&me.txt_raop));
    let mut names = vec![];
    for (ty, inst, props) in [("_airplay._tcp.local.", cfg.name.clone(), airplay), ("_raop._tcp.local.", format!("{raw_id}@{}", cfg.name), raop)] {
        let info = ServiceInfo::new(ty, &inst, &host, "", port, props).map_err(|e| e.to_string())?.enable_addr_auto();
        names.push(info.get_fullname().to_string());
        mdns.register(info).map_err(|e| format!("Could not announce this PC ({e})"))?;
    }
    Ok(Run { mdns, names, port })
}

fn apply() {
    let cfg = st(|s| s.cfg.clone());
    if let Some(r) = st(|s| s.run.take()) {
        for n in &r.names { if let Ok(rx) = r.mdns.unregister(n) { let _ = rx.recv_timeout(Duration::from_secs(1)); } }
        let _ = r.mdns.shutdown();
    }
    kick();
    st(|s| { s.error.clear(); s.device.clear(); s.mirror = None; s.code.clear(); });
    if cfg.on {
        match listen().and_then(|p| announce(&cfg, p)) {
            Ok(r) => st(|s| s.run = Some(r)),
            Err(e) => st(|s| s.error = e),
        }
    }
    emit();
}

/* ---------- commands ---------- */
#[tauri::command]
pub fn airplay_state() -> ApState { now() }

#[tauri::command]
pub async fn airplay_set(on: Option<bool>, name: Option<String>, pin: Option<bool>, forget: Option<bool>) -> Result<ApState, String> {
    blocking(move || {
        let mut cfg = st(|s| s.cfg.clone());
        if let Some(o) = on { cfg.on = o; }
        if let Some(p) = pin { cfg.pin = p; }
        if forget == Some(true) { cfg.known.clear(); }
        if let Some(n) = name {
            // the name ends up in network packets: keep it short and plain
            let n: String = n.trim().chars().filter(|c| !c.is_control() && *c != '@' && *c != '.').take(30).collect();
            if n.is_empty() { return Err("The name can't be empty".to_string()); }
            cfg.name = n;
        }
        save(&cfg)?;
        st(|s| s.cfg = cfg);
        apply();
        Ok(now())
    }).await
}

pub fn start(app: &AppHandle) {
    *APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app.clone());
    if st(|s| s.cfg.on) { thread::spawn(apply); }
    // no network at Windows start or a lost port: try again every 20 s
    thread::spawn(|| loop {
        thread::sleep(Duration::from_secs(20));
        if st(|s| s.cfg.on && s.run.is_none() && s.mirror.is_none()) { apply(); }
    });
}
pub fn stop() {
    if let Some(r) = st(|s| s.run.take()) {
        for n in &r.names { let _ = r.mdns.unregister(n); }
        thread::sleep(Duration::from_millis(150));
        let _ = r.mdns.shutdown();
    }
}
