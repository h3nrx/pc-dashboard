// PC Dashboard - backend.
// Everything that deletes goes to the Recycle Bin (never permanent), and only for
// items the app found itself (the UI sends ids, not paths).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clips;
mod gpu;
mod update;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const NO_WINDOW: u32 = 0x0800_0000;
const REPARSE: u32 = 0x400;
const ICONS_PS: &str = include_str!("icons.ps1");
const TASKS_PS: &str = include_str!("tasks.ps1");

fn env_path(var: &str) -> PathBuf { PathBuf::from(std::env::var(var).unwrap_or_default()) }
fn home() -> PathBuf { env_path("USERPROFILE") }
fn local() -> PathBuf { env_path("LOCALAPPDATA") }
fn roaming() -> PathBuf { env_path("APPDATA") }
fn base() -> PathBuf { home().join(r"Documents\ALL IN HERE") }

fn run(exe: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(exe).args(args).creation_flags(NO_WINDOW).output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    if out.status.success() { Ok(text) } else { Err(text.trim().to_string()) }
}

fn ps_file(script: &Path, args: &[&str]) -> Result<String, String> {
    let s = script.to_string_lossy().to_string();
    let mut a = vec!["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", s.as_str()];
    a.extend_from_slice(args);
    run("powershell", &a)
}

/// writes an embedded script to %TEMP% and runs it
fn ps_embedded(name: &str, body: &str, args: &[&str]) -> Result<String, String> {
    let dir = env_path("TEMP").join("pc-dashboard");
    let _ = fs::create_dir_all(&dir);
    let p = dir.join(name);
    // UTF-8 with BOM so Windows PowerShell reads umlauts correctly
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(body.as_bytes());
    fs::write(&p, bytes).map_err(|e| e.to_string())?;
    ps_file(&p, args)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tauri::async_runtime::spawn_blocking(f).await.expect("worker crashed")
}

// ---------------------------------------------------------------- disk + live stats

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FileTime { lo: u32, hi: u32 }
impl FileTime { fn v(&self) -> u64 { ((self.hi as u64) << 32) | self.lo as u64 } }

#[repr(C)]
struct MemStatus { len: u32, load: u32, total: u64, avail: u64, page_total: u64, page_avail: u64, virt_total: u64, virt_avail: u64, ext: u64 }

#[link(name = "kernel32")]
extern "system" {
    fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
    fn GetLogicalDrives() -> u32;
    fn GetDriveTypeW(root: *const u16) -> u32;
    fn GetVolumeInformationW(root: *const u16, name: *mut u16, len: u32, serial: *mut u32, max: *mut u32, flags: *mut u32, fs: *mut u16, fs_len: u32) -> i32;
    fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
    fn GlobalMemoryStatusEx(m: *mut MemStatus) -> i32;
    fn GetTickCount64() -> u64;
}

#[derive(Serialize)]
struct Disk { letter: char, name: String, total: u64, free: u64 }

fn disk(letter: char) -> Disk {
    let p: Vec<u16> = format!("{letter}:\\\0").encode_utf16().collect();
    let (mut a, mut t, mut f) = (0u64, 0u64, 0u64);
    let mut name = [0u16; 64];
    unsafe {
        GetDiskFreeSpaceExW(p.as_ptr(), &mut a, &mut t, &mut f);
        let n = std::ptr::null_mut();
        GetVolumeInformationW(p.as_ptr(), name.as_mut_ptr(), 64, n, n, n, std::ptr::null_mut(), 0);
    }
    Disk { letter, name: String::from_utf16_lossy(name.split(|c| *c == 0).next().unwrap_or(&[])), total: t, free: a }
}

/// every built-in drive, C: first (no USB sticks, DVD or network drives)
#[tauri::command]
async fn disks() -> Vec<Disk> {
    blocking(|| {
        let mask = unsafe { GetLogicalDrives() };
        let mut v: Vec<Disk> = ('A'..='Z').enumerate().filter(|(i, _)| mask >> i & 1 == 1).map(|(_, c)| c)
            .filter(|c| { let p: Vec<u16> = format!("{c}:\\\0").encode_utf16().collect(); unsafe { GetDriveTypeW(p.as_ptr()) == 3 } })
            .map(disk).filter(|d| d.total > 0).collect();
        v.sort_by_key(|d| d.letter != 'C');
        v
    }).await
}

static CPU_PREV: Mutex<(u64, u64)> = Mutex::new((0, 0));

#[derive(Serialize, Default)]
struct Stats {
    cpu: f64, cpu_name: String, ram_used: u64, ram_total: u64, uptime: u64,
    gpu_name: Option<String>, gpu: Option<f64>, gpu_temp: Option<f64>, vram_used: Option<u64>, vram_total: Option<u64>,
}

/// "AMD Ryzen 5 7500F 6-Core Processor" -> "Ryzen 5 7500F" (no temperature: Windows only gives that to drivers/admins)
fn cpu_name() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        let full: String = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0")
            .and_then(|k| k.get_value("ProcessorNameString")).unwrap_or_default();
        let short = full.replace("(R)", "").replace("(TM)", "").split(" @").next().unwrap_or("").to_string();
        short.split_whitespace()
            .filter(|w| !matches!(*w, "AMD" | "Intel" | "CPU" | "Processor") && !w.ends_with("-Core"))
            .collect::<Vec<_>>().join(" ")
    }).clone()
}

#[tauri::command]
async fn stats() -> Stats {
    blocking(|| {
        let mut s = Stats { cpu_name: cpu_name(), ..Default::default() };
        let (mut i, mut k, mut u) = (FileTime::default(), FileTime::default(), FileTime::default());
        unsafe { GetSystemTimes(&mut i, &mut k, &mut u) };
        let (idle, busy) = (i.v(), k.v() + u.v()); // kernel time includes idle
        let mut prev = CPU_PREV.lock().unwrap();
        let (di, dt) = (idle.saturating_sub(prev.0), busy.saturating_sub(prev.1));
        s.cpu = if prev.1 == 0 || dt == 0 { 0.0 } else { (1.0 - di as f64 / dt as f64).clamp(0.0, 1.0) * 100.0 };
        *prev = (idle, busy);

        let mut m = MemStatus { len: std::mem::size_of::<MemStatus>() as u32, load: 0, total: 0, avail: 0, page_total: 0, page_avail: 0, virt_total: 0, virt_avail: 0, ext: 0 };
        unsafe { GlobalMemoryStatusEx(&mut m) };
        s.ram_total = m.total;
        s.ram_used = m.total - m.avail;
        s.uptime = unsafe { GetTickCount64() } / 1000;

        // no NVIDIA driver: don't try to start its tool every second
        static NO_SMI: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if NO_SMI.load(Ordering::SeqCst) {
        } else if let Ok(out) = run("nvidia-smi", &["--query-gpu=name,utilization.gpu,temperature.gpu,memory.used,memory.total", "--format=csv,noheader,nounits"]) {
            let f: Vec<&str> = out.lines().next().unwrap_or("").split(',').map(|x| x.trim()).collect();
            if f.len() >= 5 {
                s.gpu_name = Some(f[0].replace("NVIDIA ", "").replace("GeForce ", ""));
                s.gpu = f[1].parse().ok();
                s.gpu_temp = f[2].parse().ok();
                s.vram_used = f[3].parse::<u64>().ok().map(|x| x * 1048576);
                s.vram_total = f[4].parse::<u64>().ok().map(|x| x * 1048576);
            }
        } else { NO_SMI.store(true, Ordering::SeqCst); }
        if s.gpu.is_none() {
            if let Some(g) = gpu::read() {
                let name = g.name.replace("(R)", "").replace("(TM)", "").replace("AMD ", "").replace("Intel ", "").replace("NVIDIA ", "").replace("GeForce ", "");
                s.gpu_name = Some(name.split_whitespace().collect::<Vec<_>>().join(" "));
                s.gpu = Some(g.load);
                s.vram_used = Some(g.used);
                s.vram_total = Some(g.total);
            }
        }
        s
    })
    .await
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = fs::read_dir(&d) {
            for e in rd.flatten() {
                if let Ok(m) = e.metadata() {
                    if m.file_attributes() & REPARSE != 0 { continue; } // junctions / symlinks
                    if m.is_dir() { stack.push(e.path()) } else { total += m.len() }
                }
            }
        }
    }
    total
}

fn path_size(p: &Path) -> u64 {
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => dir_size(p),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

#[derive(Serialize)]
struct Folder { name: String, parent: String, path: String, size: u64 }

fn children(root: &Path, skip: &[&str], out: &mut Vec<PathBuf>) {
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten() {
            if let Ok(m) = e.metadata() {
                let n = e.file_name().to_string_lossy().to_string();
                if m.is_dir() && m.file_attributes() & REPARSE == 0 && !skip.iter().any(|s| s.eq_ignore_ascii_case(&n)) {
                    out.push(e.path());
                }
            }
        }
    }
}

#[tauri::command]
async fn scan_folders() -> Vec<Folder> {
    blocking(|| {
        let mut dirs = vec![];
        children(Path::new("C:\\"), &["Users", "Program Files", "Program Files (x86)", "Windows", "ProgramData", "$Recycle.Bin", "System Volume Information", "Recovery", "$WinREAgent"], &mut dirs);
        children(Path::new(r"C:\Program Files"), &[], &mut dirs);
        children(Path::new(r"C:\Program Files (x86)"), &[], &mut dirs);
        children(&home(), &["AppData"], &mut dirs);
        children(&local(), &[], &mut dirs);
        children(&roaming(), &[], &mut dirs);

        let next = AtomicUsize::new(0);
        let found = Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= dirs.len() { break }
                    let d = &dirs[i];
                    let size = dir_size(d);
                    found.lock().unwrap().push(Folder {
                        name: d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                        parent: d.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
                        path: d.to_string_lossy().to_string(),
                        size,
                    });
                });
            }
        });
        let mut v = found.into_inner().unwrap();
        v.sort_by(|a, b| b.size.cmp(&a.size));
        v.truncate(14);
        v
    })
    .await
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() { return Err("That folder doesn't exist anymore".into()) }
    Command::new("explorer.exe").arg(&path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- cleanup

#[derive(Serialize, Clone)]
struct Candidate { id: String, title: String, desc: String, size: u64, count: usize, safe: bool, #[serde(skip)] paths: Vec<PathBuf> }

fn older_than(p: &Path, days: u64) -> bool {
    fs::symlink_metadata(p).and_then(|m| m.modified())
        .map(|t| SystemTime::now().duration_since(t).unwrap_or_default() > Duration::from_secs(days * 86400))
        .unwrap_or(false)
}

fn files_recursive(root: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten() {
            if let Ok(m) = e.metadata() {
                if m.file_attributes() & REPARSE != 0 { continue }
                if m.is_dir() { files_recursive(&e.path(), out) } else { out.push(e.path()) }
            }
        }
    }
}

fn make(id: &str, title: &str, desc: String, safe: bool, paths: Vec<PathBuf>) -> Option<Candidate> {
    if paths.is_empty() { return None }
    let size: u64 = paths.iter().map(|p| path_size(p)).sum();
    if size < 1024 * 1024 { return None } // under 1 MB is not worth showing
    Some(Candidate { id: id.into(), title: title.into(), desc, size, count: paths.len(), safe, paths })
}

fn find_candidates() -> Vec<Candidate> {
    let mut list = vec![];

    // 1. old CapCut versions: CapCut keeps every version after an update
    let apps = local().join(r"CapCut\Apps");
    if let Ok(info) = fs::read_to_string(apps.join("ProductInfo.xml")) {
        if let Some(cur) = info.split("full_appver value=\"").nth(1).and_then(|s| s.split('"').next()) {
            let mut old = vec![];
            if let Ok(rd) = fs::read_dir(&apps) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    let looks_like_version = n.split('.').count() == 4 && n.split('.').all(|p| p.chars().all(|c| c.is_ascii_digit()));
                    if e.path().is_dir() && looks_like_version && n != cur { old.push(e.path()) }
                }
            }
            let names: Vec<String> = old.iter().filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string())).collect();
            list.extend(make("capcut", "Old CapCut versions", format!("Active is {cur}. Old: {}", names.join(", ")), true, old));
        }
    }

    // 2. temp files older than 7 days
    let mut tmp = vec![];
    if let Ok(rd) = fs::read_dir(env_path("TEMP")) {
        for e in rd.flatten() { if older_than(&e.path(), 7) { tmp.push(e.path()) } }
    }
    list.extend(make("temp", "Temporary files", "Older than 7 days, no app needs them anymore".into(), true, tmp));

    // 3. crash dumps
    let mut dumps = vec![];
    files_recursive(&local().join("CrashDumps"), &mut dumps);
    list.extend(make("dumps", "Crash dumps", "Leftovers from apps that crashed".into(), true, dumps));

    // 4. Medal clips older than 90 days
    let mut clips = vec![];
    files_recursive(Path::new(r"C:\Medal\Clips"), &mut clips);
    clips.retain(|p| older_than(p, 90));
    list.extend(make("medal", "Old Medal clips", "Clips older than 3 months".into(), false, clips));

    // 5. installers in Downloads older than 14 days
    let mut inst = vec![];
    if let Ok(rd) = fs::read_dir(home().join("Downloads")) {
        for e in rd.flatten() {
            let p = e.path();
            let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            if p.is_file() && (ext == "exe" || ext == "msi") && older_than(&p, 14) { inst.push(p) }
        }
    }
    list.extend(make("installers", "Old installers in Downloads", ".exe / .msi older than 2 weeks, usually already installed".into(), false, inst));

    list
}

#[tauri::command]
async fn cleanup_candidates() -> Vec<Candidate> { blocking(find_candidates).await }

#[derive(Serialize)]
struct CleanResult { freed: u64, done: Vec<String>, failed: Vec<String> }

#[tauri::command]
async fn clean(ids: Vec<String>) -> CleanResult {
    blocking(move || {
        let before = disk('C').free;
        let (mut done, mut failed) = (vec![], vec![]);
        for c in find_candidates().into_iter().filter(|c| ids.contains(&c.id)) {
            // one by one, so a single locked file doesn't stop the rest
            let mut errors = 0;
            for p in &c.paths { if trash::delete(p).is_err() { errors += 1 } }
            if errors == 0 { done.push(c.title) } else { failed.push(format!("{} ({errors} files in use)", c.title)) }
        }
        CleanResult { freed: disk('C').free.saturating_sub(before), done, failed }
    })
    .await
}

// ---------------------------------------------------------------- startup apps

const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";
const PKG_TASKS: &str = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\SystemAppData";
const ADMIN: &str = "This is a system entry, it needs admin rights.";

struct Source { id: &'static str, hive: isize, run: Option<&'static str>, folder: Option<PathBuf>, approved: &'static str, system: bool }

fn sources() -> Vec<Source> {
    vec![
        Source { id: "hkcu", hive: 0, run: Some(r"Software\Microsoft\Windows\CurrentVersion\Run"), folder: None, approved: "Run", system: false },
        Source { id: "hklm", hive: 1, run: Some(r"Software\Microsoft\Windows\CurrentVersion\Run"), folder: None, approved: "Run", system: true },
        Source { id: "hklm32", hive: 1, run: Some(r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run"), folder: None, approved: "Run32", system: true },
        Source { id: "folder", hive: 0, run: None, folder: Some(roaming().join(r"Microsoft\Windows\Start Menu\Programs\Startup")), approved: "StartupFolder", system: false },
        Source { id: "folderall", hive: 1, run: None, folder: Some(env_path("ProgramData").join(r"Microsoft\Windows\Start Menu\Programs\StartUp")), approved: "StartupFolder", system: true },
    ]
}

fn hive(h: isize) -> RegKey { RegKey::predef(if h == 0 { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE }) }

/// "LivelyWallpaper" -> "Lively Wallpaper"
fn split_camel(s: &str) -> String {
    let mut out = String::new();
    let cs: Vec<char> = s.chars().collect();
    for (i, c) in cs.iter().enumerate() {
        if i > 0 && c.is_uppercase() && (cs[i - 1].is_lowercase() || cs.get(i + 1).map(|n| n.is_lowercase()).unwrap_or(false)) && cs[i - 1] != ' ' { out.push(' ') }
        out.push(*c);
    }
    out
}

fn pretty(name: &str) -> String {
    let n = name.trim_end_matches(".lnk");
    let n = n.strip_prefix("electron.app.").unwrap_or(n);
    if n.starts_with("GoogleUpdater") { return "Google Updater".into() }
    match n {
        "SecurityHealth" => "Windows Security".into(),
        "RtkAudUService" => "Realtek Audio".into(),
        "CloudflareWARP" => "Cloudflare WARP".into(),
        "EpicGamesLauncher" => "Epic Games".into(),
        "RobloxPlayerBeta" => "Roblox".into(),
        "MEmuSVC" => "MEmu".into(),
        "GCC" => "GIGABYTE Control Center".into(),
        "AmdX3DInstaller" => "AMD X3D Installer".into(),
        "EqualizerAPOUpdateChecker" => "Equalizer APO Updater".into(),
        "WhatsAppDesktop" => "WhatsApp".into(),
        _ => n.to_string(),
    }
}

/// path of the program inside a Run command line
fn exe_of(cmd: &str) -> String {
    let c = cmd.trim();
    if let Some(rest) = c.strip_prefix('"') { return rest.split('"').next().unwrap_or("").to_string() }
    let low = c.to_lowercase();
    for ext in [".exe", ".lnk", ".bat", ".cmd"] {
        if let Some(i) = low.find(ext) { return c[..i + ext.len()].to_string() }
    }
    c.split(' ').next().unwrap_or("").to_string()
}

#[derive(Serialize)]
struct StartItem { id: String, name: String, command: String, icon: String, kind: String, system: bool, enabled: bool, missing: bool }

/// true when the program an entry starts is no longer on disk (app was uninstalled)
fn target_missing(exe: &str) -> bool {
    let mut p = exe.trim().trim_matches('"').to_string();
    if p.is_empty() { return false }
    // expand %VAR%
    while let Some(a) = p.find('%') {
        let Some(len) = p[a + 1..].find('%') else { break };
        let var = &p[a + 1..a + 1 + len];
        let val = std::env::var(var).unwrap_or_default();
        if val.is_empty() { break }
        p = format!("{}{}{}", &p[..a], val, &p[a + 2 + len..]);
    }
    // bare names like "powershell" come from PATH
    if !p.contains('\\') { return false }
    !Path::new(&p).exists()
}

#[derive(Deserialize)]
struct TaskInfo { path: String, name: String, exe: String, args: String, enabled: bool, system: bool }

fn autostart_items() -> Vec<StartItem> {
    let mut items = vec![];

    // registry Run keys + startup folders
    for s in sources() {
        let approved = hive(s.hive).open_subkey(format!(r"{APPROVED}\{}", s.approved)).ok();
        let is_on = |name: &str| -> bool {
            approved.as_ref().and_then(|k| k.get_raw_value(name).ok())
                .map(|v| { let b: &[u8] = v.bytes.as_ref(); b.first().map(|x| x & 1 == 0).unwrap_or(true) })
                .unwrap_or(true)
        };
        let mut add = |name: String, command: String, kind: &str| {
            // a shortcut in the startup folder: check where it points
            let target = if kind == "Shortcut" { lnk_target(&command).unwrap_or_default() } else { exe_of(&command) };
            items.push(StartItem { id: format!("{}|{}", s.id, name), enabled: is_on(&name), icon: exe_of(&command), missing: target_missing(&target), name: pretty(&name), command, kind: kind.into(), system: s.system });
        };
        if let Some(run) = s.run {
            if let Ok(k) = hive(s.hive).open_subkey(run) {
                for (name, _) in k.enum_values().flatten() {
                    let cmd = k.get_value::<String, _>(&name).unwrap_or_default();
                    add(name, cmd, "App");
                }
            }
        }
        if let Some(dir) = &s.folder {
            if let Ok(rd) = fs::read_dir(dir) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    if n.eq_ignore_ascii_case("desktop.ini") { continue }
                    add(n, e.path().to_string_lossy().to_string(), "Shortcut");
                }
            }
        }
    }

    // Store apps (Lively, WhatsApp, ...) - they have their own startup task
    if let Ok(root) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(PKG_TASKS) {
        for pfn in root.enum_keys().flatten() {
            if let Ok(pk) = root.open_subkey(&pfn) {
                for task in pk.enum_keys().flatten() {
                    if let Ok(state) = pk.open_subkey(&task).and_then(|t| t.get_value::<u32, _>("State")) {
                        let pkg = pfn.split('_').next().unwrap_or(&pfn).to_string();
                        let short = pkg.rsplit('.').next().unwrap_or(&pkg);
                        let name = pretty(&split_camel(short).replace("Wallpaper", "Wallpaper"));
                        items.push(StartItem {
                            id: format!("pkg|{pfn}|{task}"), name, command: format!("Microsoft Store app · {pkg}"),
                            icon: format!("pkg:{pkg}"), kind: "Store".into(), missing: false,
                            system: state == 3 || state == 4, enabled: state == 2 || state == 4,
                        });
                    }
                }
            }
        }
    }

    // scheduled tasks that run at logon
    if let Ok(json) = ps_embedded("tasks.ps1", TASKS_PS, &[]) {
        let json = json.trim();
        let list: Vec<TaskInfo> = if json.starts_with('[') { serde_json::from_str(json).unwrap_or_default() }
            else { serde_json::from_str::<TaskInfo>(json).map(|t| vec![t]).unwrap_or_default() };
        for t in list {
            let short = t.name.split('{').next().unwrap_or(&t.name).trim().to_string();
            items.push(StartItem {
                id: format!("task|{}", t.path), name: pretty(&short),
                command: format!("{} {}", t.exe, t.args).trim().to_string(), icon: t.exe.clone(),
                kind: "Task".into(), system: t.system, enabled: t.enabled, missing: target_missing(&t.exe),
            });
        }
    }

    items.sort_by(|a, b| b.enabled.cmp(&a.enabled).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    items
}

#[tauri::command]
async fn autostart_list() -> Vec<StartItem> { blocking(autostart_items).await }

#[tauri::command]
async fn autostart_set(id: String, enabled: bool) -> Result<(), String> {
    blocking(move || {
        let (src, rest) = id.split_once('|').ok_or("invalid")?;
        match src {
            "pkg" => {
                let (pfn, task) = rest.split_once('|').ok_or("invalid")?;
                let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(format!(r"{PKG_TASKS}\{pfn}\{task}"), KEY_READ | KEY_SET_VALUE)
                    .map_err(|_| "Couldn't change that entry.".to_string())?;
                // 2 = enabled, 1 = disabled by user (what the Task Manager writes)
                key.set_value("State", &(if enabled { 2u32 } else { 1u32 })).map_err(|_| "Couldn't change that entry.".to_string())
            }
            "task" => run("schtasks", &["/Change", "/TN", rest, if enabled { "/ENABLE" } else { "/DISABLE" }])
                .map(|_| ()).map_err(|e| if e.contains("Zugriff") || e.to_lowercase().contains("access") { ADMIN.to_string() } else { e }),
            _ => {
                let s = sources().into_iter().find(|s| s.id == src).ok_or("invalid")?;
                // same format the Task Manager writes: 0x02 = on, 0x03 = off (+ time it was switched off)
                let mut data = vec![if enabled { 2u8 } else { 3u8 }, 0, 0, 0];
                let ft: u64 = if enabled { 0 } else {
                    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64 / 100 + 116_444_736_000_000_000
                };
                data.extend_from_slice(&ft.to_le_bytes());
                let err = || if s.system { ADMIN.to_string() } else { "Couldn't change that entry.".to_string() };
                let key = hive(s.hive).create_subkey(format!(r"{APPROVED}\{}", s.approved)).map_err(|_| err())?.0;
                key.set_raw_value(rest, &RegValue { bytes: data.into(), vtype: REG_BINARY }).map_err(|_| err())
            }
        }
    })
    .await
}

/// where a .lnk points (binary format: target path is in the LinkInfo block)
fn lnk_target(path: &str) -> Option<String> {
    if !path.to_lowercase().ends_with(".lnk") { return None }
    let out = run("powershell", &["-NoProfile", "-Command",
        &format!("(New-Object -ComObject WScript.Shell).CreateShortcut('{}').TargetPath", path.replace('\'', "''"))]).ok()?;
    let t = out.trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

/// removes a startup entry of an app that is no longer installed (only your own, non-system entries)
#[tauri::command]
async fn autostart_remove(id: String) -> Result<(), String> {
    blocking(move || {
        let item = autostart_items().into_iter().find(|i| i.id == id).ok_or("Entry not found.")?;
        if !item.missing { return Err("That app is still installed. Just switch it off instead.".into()) }
        if item.system { return Err(ADMIN.into()) }
        let (src, name) = id.split_once('|').ok_or("invalid")?;
        let s = sources().into_iter().find(|s| s.id == src).ok_or("Only startup entries can be removed here.")?;
        if let Some(run) = s.run {
            hive(s.hive).open_subkey_with_flags(run, KEY_SET_VALUE)
                .and_then(|k| k.delete_value(name)).map_err(|_| "Couldn't remove that entry.".to_string())?;
        } else if let Some(dir) = &s.folder {
            trash::delete(dir.join(name)).map_err(|e| e.to_string())?;
        }
        // the on/off flag that belonged to it
        if let Ok(k) = hive(s.hive).open_subkey_with_flags(format!(r"{APPROVED}\{}", s.approved), KEY_SET_VALUE) {
            let _ = k.delete_value(name);
        }
        Ok(())
    }).await
}

/// real app icons as PNG data URLs, keyed by the source path the list gave
#[tauri::command]
async fn icons(sources: Vec<String>) -> HashMap<String, String> {
    blocking(move || {
        let dir = env_path("TEMP").join("pc-dashboard");
        let _ = fs::create_dir_all(&dir);
        let input: Vec<serde_json::Value> = sources.iter().filter(|s| !s.is_empty()).map(|s| serde_json::json!({ "id": s, "src": s })).collect();
        let (i, o) = (dir.join("icons-in.json"), dir.join("icons-out.json"));
        let _ = fs::remove_file(&o);
        if fs::write(&i, serde_json::to_string(&input).unwrap()).is_err() { return HashMap::new() }
        let _ = ps_embedded("icons.ps1", ICONS_PS, &["-In", &i.to_string_lossy(), "-Out", &o.to_string_lossy()]);
        fs::read_to_string(&o).ok()
            .map(|s| s.trim_start_matches('\u{feff}').to_string())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    })
    .await
}

// ---------------------------------------------------------------- desktop themes

#[derive(Serialize)]
struct Theme { id: &'static str, name: &'static str, desc: &'static str, on: bool, available: bool, hint: &'static str }

fn task_on(name: &str) -> Option<bool> {
    let out = run("schtasks", &["/Query", "/TN", &format!(r"\h3nrx\{name}"), "/FO", "LIST"]).ok()?;
    Some(!(out.contains("Deaktiviert") || out.contains("Disabled")))
}

fn vencord_settings() -> PathBuf { roaming().join(r"Vencord\settings\settings.json") }
const DISCORD_THEME: &str = "h3nrx-chrome.theme.css";

fn theme_list() -> Vec<Theme> {
    let b = base();
    let t = |id, name, desc, on: Option<bool>, hint| Theme { id, name, desc, on: on.unwrap_or(false), available: on.is_some(), hint };

    let cursor = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Control Panel\Cursors").ok().filter(|_| b.join(r"h3nrx Mauszeiger\mauszeiger.ps1").exists())
        .map(|k| k.get_value::<String, _>("").unwrap_or_default() == "h3nrx");
    let discord = fs::read_to_string(vencord_settings()).ok().filter(|_| roaming().join(r"Vencord\themes").join(DISCORD_THEME).exists()).and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| v["enabledThemes"].as_array().map(|a| a.iter().any(|x| x == DISCORD_THEME)).unwrap_or(false));

    vec![
        t("startup", "Boot screen", "Chrome loading bar right after login", task_on("h3nrx-startup"), ""),
        t("saver", "Screensaver", "F6 toggles it, every other key is locked", task_on("h3nrx-screensaver"), ""),
        t("icons", "Taskbar icons", "Chrome icons for Discord, Spotify, Chrome …", task_on("h3nrx-icons"), ""),
        t("cursor", "Cursors", "Chrome pointers with loading animation", cursor, ""),
        t("discord", "Discord theme", "Glass with chrome accents, via Vencord", discord, "Restart Discord to apply"),
    ]
}

#[tauri::command]
async fn themes() -> Vec<Theme> { blocking(theme_list).await }

fn set_vencord_theme(on: bool) -> Result<(), String> {
    let path = vencord_settings();
    let text = fs::read_to_string(&path).map_err(|_| "Vencord settings not found")?;
    let mut v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let list = v["enabledThemes"].as_array().cloned().unwrap_or_default();
    let mut list: Vec<serde_json::Value> = list.into_iter().filter(|x| x != DISCORD_THEME).collect();
    if on { list.push(DISCORD_THEME.into()) }
    v["enabledThemes"] = serde_json::Value::Array(list);
    fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn theme_set(id: String, on: bool) -> Result<Vec<Theme>, String> {
    blocking(move || {
        let b = base();
        let task = |name: &str| run("schtasks", &["/Change", "/TN", &format!(r"\h3nrx\{name}"), if on { "/ENABLE" } else { "/DISABLE" }]).map(|_| ());
        let mode = if on { "apply" } else { "restore" };
        match id.as_str() {
            "startup" => task("h3nrx-startup")?,
            "saver" => {
                let exe = b.join(r"h3nrx Bildschirmschoner\h3nrx-screensaver.exe");
                if on {
                    task("h3nrx-screensaver")?;
                    Command::new(&exe).creation_flags(NO_WINDOW).spawn().map_err(|e| e.to_string())?;
                } else {
                    let _ = Command::new(&exe).arg("--quit").creation_flags(NO_WINDOW).status();
                    task("h3nrx-screensaver")?;
                }
            }
            "icons" => { ps_file(&b.join(r"h3nrx Taskbar Icons\taskbar-icons.ps1"), &["-Mode", mode])?; }
            "cursor" => { ps_file(&b.join(r"h3nrx Mauszeiger\mauszeiger.ps1"), &[mode])?; }
            "discord" => set_vencord_theme(on)?,
            _ => return Err("unknown".into()),
        }
        Ok(theme_list())
    })
    .await
}

// ---------------------------------------------------------------- jarvis

const JARVIS_PORT: u16 = 8765;
static JARVIS_OURS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn jarvis_dir() -> PathBuf { clips::data().join("jarvis") }
const JARVIS_FILES: [(&str, &str); 6] = [
    ("actions.py", include_str!("../jarvis/actions.py")), ("config.py", include_str!("../jarvis/config.py")), ("hotkey.py", include_str!("../jarvis/hotkey.py")),
    ("jarvis.py", include_str!("../jarvis/jarvis.py")), ("monitor.py", include_str!("../jarvis/monitor.py")), ("texts.py", include_str!("../jarvis/texts.py")),
];
fn jarvis_unpack() {
    let dir = jarvis_dir();
    let _ = fs::create_dir_all(&dir);
    for (n, text) in JARVIS_FILES {
        if fs::read_to_string(dir.join(n)).ok().as_deref() != Some(text) { let _ = fs::write(dir.join(n), text); }
    }
}
fn has_python() -> bool {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .any(|d| d.join("pythonw.exe").exists() || d.join("pyw.exe").exists()) || env_path("WINDIR").join("pyw.exe").exists()
}
fn jarvis_file() -> PathBuf { jarvis_dir().join("settings.json") }

fn jarvis_settings() -> serde_json::Map<String, serde_json::Value> {
    match fs::read_to_string(jarvis_file()).ok().and_then(|s| serde_json::from_str(s.trim_start_matches('\u{feff}')).ok()) {
        Some(serde_json::Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    }
}

/// Versions before 1.3 used three folders in %LOCALAPPDATA%; everything moves into "pc-dashboard" once.
fn one_folder() {
    let data = clips::data();
    // Jarvis: keys, language and talk key
    let (old, new) = (local().join(r"MissionControl\settings.json"), jarvis_file());
    let time = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    if old.exists() {
        let _ = fs::create_dir_all(jarvis_dir());
        // the newer one is the one that was in use; the old file only goes once the copy is proven
        let fine = time(&new) >= time(&old) || (fs::copy(&old, &new).is_ok() && fs::read(&old).ok() == fs::read(&new).ok());
        if fine && fs::remove_file(&old).is_ok() { let _ = fs::remove_dir(old.parent().unwrap()); }      // only if empty
    }
    // the window's own browser data
    let (old, new) = (local().join("com.h3nrx.control"), data.join("webview"));
    if old.is_dir() {
        let _ = fs::create_dir_all(&data);
        for _ in 0..12 {
            if new.exists() || fs::rename(&old, &new).is_ok() { break; }
            std::thread::sleep(Duration::from_millis(250));      // the closing old version still holds it
        }
        if new.exists() { let _ = fs::remove_dir_all(&old); }
    }
    std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", &new);
}

fn jarvis_up() -> bool {
    std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], JARVIS_PORT).into(), Duration::from_millis(250)).is_ok()
}

fn key_hint(v: Option<&serde_json::Value>) -> Option<String> {
    let k = v?.as_str()?.trim();
    if k.len() < 8 { return None; }
    Some(k[k.len() - 4..].to_string())
}

#[derive(Serialize)]
struct JarvisStatus { installed: bool, python: bool, running: bool, groq: Option<String>, eleven: Option<String>, lang: String, voice: String, talk: String }

fn jarvis_status_now() -> JarvisStatus {
    let s = jarvis_settings();
    JarvisStatus {
        installed: jarvis_dir().join("monitor.py").exists(),
        python: has_python(),
        running: jarvis_up(),
        groq: key_hint(s.get("groq_key")),
        eleven: key_hint(s.get("elevenlabs_key")),
        lang: s.get("lang").and_then(|v| v.as_str()).unwrap_or("en").to_string(),
        voice: s.get("voice_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        talk: s.get("talk_button").and_then(|v| v.as_str()).unwrap_or("x2").to_string(),
    }
}

#[tauri::command]
async fn jarvis_status() -> JarvisStatus { blocking(jarvis_status_now).await }

fn jarvis_kill() {
    // only python processes listening on the Jarvis port
    let _ = run("powershell", &["-NoProfile", "-Command",
        "Get-NetTCPConnection -LocalPort 8765 -State Listen -ErrorAction SilentlyContinue | ForEach-Object { $p = Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue; if ($p -and $p.ProcessName -like 'python*') { Stop-Process -Id $p.Id -Force } }"]);
    for _ in 0..20 { if !jarvis_up() { break; } std::thread::sleep(Duration::from_millis(150)); }
}

fn jarvis_spawn() -> Result<(), String> {
    if jarvis_up() { return Ok(()); }
    jarvis_unpack();
    let dir = jarvis_dir();
    if !dir.join("monitor.py").exists() { return Err("Jarvis folder not found.".into()); }
    let mut last = String::new();
    let mut ok = false;
    for exe in ["pythonw", "pyw"] {
        match Command::new(exe).arg("monitor.py").current_dir(&dir).creation_flags(NO_WINDOW).spawn() {
            Ok(_) => { ok = true; break; }
            Err(e) => last = e.to_string(),
        }
    }
    if !ok { return Err(format!("Python not found ({last}).")); }
    JARVIS_OURS.store(true, Ordering::SeqCst);
    for _ in 0..60 { if jarvis_up() { return Ok(()); } std::thread::sleep(Duration::from_millis(200)); }
    Err("Jarvis did not start. Run start-dashboard.bat once to see the error.".into())
}

#[tauri::command]
async fn jarvis_start() -> Result<JarvisStatus, String> {
    blocking(|| { jarvis_spawn()?; Ok(jarvis_status_now()) }).await
}

#[tauri::command]
async fn jarvis_save(groq: String, eleven: String, lang: String, talk: String) -> Result<JarvisStatus, String> {
    blocking(move || {
        if !["x1", "x2", "middle"].contains(&talk.as_str()) && !clips::usable_key(&talk) { return Err("That key can't be used. Try F9 or Alt + a letter.".into()); }
        if clips::own_keys().contains(&talk) { return Err("That key already belongs to Clips.".into()); }
        let mut s = jarvis_settings();
        s.insert("talk_button".into(), talk.as_str().into());
        let (g, e) = (groq.trim(), eleven.trim());
        if !g.is_empty() { s.insert("groq_key".into(), g.into()); }
        if !e.is_empty() { s.insert("elevenlabs_key".into(), e.into()); }
        if lang == "en" || lang == "de" { s.insert("lang".into(), lang.as_str().into()); }
        let text = serde_json::to_string_pretty(&serde_json::Value::Object(s)).map_err(|e| e.to_string())?;
        let file = jarvis_file();
        let _ = fs::create_dir_all(jarvis_dir());
        let tmp = file.with_extension("json.tmp");
        fs::write(&tmp, &text).map_err(|e| format!("Could not save: {e}"))?;
        if fs::read_to_string(&tmp).ok().as_deref() != Some(text.as_str()) {
            let _ = fs::remove_file(&tmp);
            return Err("Could not save the keys (write was blocked).".into());
        }
        fs::rename(&tmp, &file).map_err(|e| format!("Could not save: {e}"))?;
        // Jarvis reads its keys at start, so restart it
        let was_ours = JARVIS_OURS.load(Ordering::SeqCst);
        jarvis_kill();
        let r = jarvis_spawn();
        if !was_ours && r.is_ok() { JARVIS_OURS.store(true, Ordering::SeqCst); }
        r?;
        Ok(jarvis_status_now())
    }).await
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    const OK: [&str; 3] = ["https://www.python.org/downloads/", "https://console.groq.com/keys", "https://elevenlabs.io/app/settings/api-keys"];
    if !OK.contains(&url.as_str()) { return Err("blocked".into()); }
    Command::new("explorer").arg(&url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

// ----------------------------------------------------------------

struct TrayItems(Vec<MenuItem<tauri::Wry>>);
fn tray_text(i: usize) -> &'static str {
    (if clips::german() { ["PC Dashboard öffnen", "Clip speichern", "Aufnahme starten / stoppen", "Beenden"] }
     else { ["Open PC Dashboard", "Save clip", "Start / stop recording", "Quit"] })[i]
}
/// the language itself is stored with the other settings; this only relabels the tray menu
#[tauri::command]
fn app_lang(app: AppHandle) {
    if let Some(t) = app.try_state::<TrayItems>() { for (i, m) in t.0.iter().enumerate() { let _ = m.set_text(tray_text(i)); } }
}

/// false only for the very first call after Windows started the app in the background
#[tauri::command]
fn first_show() -> bool {
    static ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let auto = std::env::args().any(|a| a == "--autostart");
    ASKED.swap(true, Ordering::SeqCst) || !(auto && clips::start_hidden())
}

/// the app's own entry in the startup list (null = just read it)
#[tauri::command]
fn app_autostart(on: Option<bool>) -> Result<bool, String> {
    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "PC Dashboard";
    const OLD: &str = "h3nrx Control";
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let approved = format!(r"{APPROVED}\Run");
    if let Some(on) = on {
        let (k, _) = hkcu.create_subkey(RUN).map_err(|e| e.to_string())?;
        if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            k.set_value(NAME, &format!("\"{}\" --autostart", exe.display())).map_err(|e| e.to_string())?;
        } else if k.get_raw_value(NAME).is_ok() {
            k.delete_value(NAME).map_err(|e| e.to_string())?;
        }
        let _ = k.delete_value(OLD);
        // forget an old "disabled" mark from Task Manager for this entry
        if let Ok(a) = hkcu.open_subkey_with_flags(&approved, KEY_SET_VALUE) { let _ = a.delete_value(NAME); let _ = a.delete_value(OLD); }
    }
    let there = hkcu.open_subkey(RUN).map(|k| k.get_raw_value(NAME).is_ok()).unwrap_or(false);
    let off = hkcu.open_subkey(&approved).ok().and_then(|k| k.get_raw_value(NAME).ok())
        .map(|v| { let b: &[u8] = v.bytes.as_ref(); b.first().map(|x| x & 1 == 1).unwrap_or(false) }).unwrap_or(false);
    Ok(there && !off)
}

/// the app used to be called differently: carry its startup entry over once
fn migrate_autostart() {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(k) = hkcu.open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_READ | KEY_SET_VALUE) {
        if k.get_raw_value("h3nrx Control").is_err() { return; }
        let off = hkcu.open_subkey(format!(r"{APPROVED}\Run")).ok().and_then(|a| a.get_raw_value("h3nrx Control").ok())
            .map(|v| { let b: &[u8] = v.bytes.as_ref(); b.first().map(|x| x & 1 == 1).unwrap_or(false) }).unwrap_or(false);
        let _ = app_autostart(Some(!off));
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show(); let _ = w.unminimize(); let _ = w.set_focus(); let _ = w.emit("shown", ());
    }
}
pub(crate) fn quit_app(app: &AppHandle) {
    let app = app.clone();
    if let Some(w) = app.get_webview_window("main") { let _ = w.hide(); }
    std::thread::spawn(move || { clips::stop(); app.exit(0); });
}

fn main() {
    update::finish();
    one_folder();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_main(app)))
        .invoke_handler(tauri::generate_handler![
            disks, stats, scan_folders, open_folder, cleanup_candidates, clean,
            autostart_list, autostart_set, autostart_remove, icons, themes, theme_set,
            jarvis_status, jarvis_start, jarvis_save, open_url,
            clips::clips_state, clips::clips_list, clips::clips_config, clips::clips_save, clips::clips_record,
            clips::clips_delete, clips::clips_copy, clips::clips_show, clips::clips_folder,
            first_show, app_autostart, app_lang,
            update::updates_check, update::update_install, update::recorder_install
        ])
        .setup(|app| {
            let h = app.handle().clone();
            clips::start(&h);
            migrate_autostart();
            // tray icon: the clipper keeps running when the window is closed
            let open = MenuItem::with_id(app, "open", tray_text(0), true, None::<&str>)?;
            let save = MenuItem::with_id(app, "clip", tray_text(1), true, None::<&str>)?;
            let rec = MenuItem::with_id(app, "rec", tray_text(2), true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", tray_text(3), true, None::<&str>)?;
            let mut tray = TrayIconBuilder::new().tooltip("PC Dashboard").menu(&Menu::with_items(app, &[&open, &save, &rec, &quit])?)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => show_main(app),
                    "clip" => clips::save_last(),
                    "rec" => clips::record_bg(),
                    "quit" => quit_app(app),
                    _ => {}
                })
                .on_tray_icon_event(|t, e| if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e { show_main(t.app_handle()) });
            if let Some(i) = app.default_window_icon() { tray = tray.icon(i.clone()); }
            tray.build(app)?;
            app.manage(TrayItems(vec![open, save, rec, quit]));
            Ok(())
        })
        .on_window_event(|w, e| if let tauri::WindowEvent::CloseRequested { api, .. } = e {
            api.prevent_close();
            if clips::tray() { let _ = w.hide(); let _ = w.emit("hidden", ()); } else { quit_app(w.app_handle()); }
        })
        .build(tauri::generate_context!())
        .expect("PC Dashboard failed to start")
        .run(|_, ev| {
            // stop Jarvis again if this app started it
            if let tauri::RunEvent::Exit = ev {
                if JARVIS_OURS.load(Ordering::SeqCst) { jarvis_kill(); }
            }
        });
}
