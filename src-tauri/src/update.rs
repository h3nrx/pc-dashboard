// Updates come from the GitHub releases of this project. The file is checked against the SHA-256
// GitHub lists for it before it replaces the running exe. The screen recorder (ffmpeg) is fetched the
// same way, but pinned to one known build.
use crate::{blocking, env_path, NO_WINDOW};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const REPO: &str = "h3nrx/pc-dashboard";
const ASSET: &str = "PC-Dashboard.exe";
const FFMPEG: [(&str, &str, u64); 2] = [
    ("https://github.com/GyanD/codexffmpeg/releases/download/9.1/ffmpeg-9.1-essentials_build.7z", "835b9a733445ba7a56a37a26cf7bde84e2f3ce7da2a8ec984276bb2abb1b922c", 36_671_021),
    ("https://github.com/GyanD/codexffmpeg/releases/download/9.1/ffmpeg-9.1-essentials_build.zip", "b9bbbb3acece000cbeec994f856649354e52c773807841e78908374b24d658fd", 119_467_219),
];
static BUSY: Mutex<()> = Mutex::new(());

fn system32(exe: &str) -> PathBuf { env_path("WINDIR").join("System32").join(exe) }
fn curl() -> Command {
    let mut c = Command::new(system32("curl.exe"));
    c.creation_flags(NO_WINDOW).args(["-fsSL", "--proto", "=https", "--proto-redir", "=https", "-A", "pc-dashboard", "--connect-timeout", "15"]);
    c
}

#[derive(Serialize, Clone)]
pub struct Release { version: String, date: String, notes: String, size: u64, #[serde(skip)] url: String, #[serde(skip)] sha: String }
#[derive(Serialize)]
pub struct Updates { current: String, newer: bool, releases: Vec<Release> }

fn parse(v: &str) -> Option<[u32; 3]> {
    let mut out = [0; 3];
    for (i, p) in v.trim().trim_start_matches(['v', 'V']).split('.').enumerate() { *out.get_mut(i)? = p.parse().ok()?; }
    Some(out)
}
fn current() -> &'static str { env!("CARGO_PKG_VERSION") }

fn releases() -> Result<Vec<Release>, String> {
    let out = curl().args(["--max-time", "30", "-H", "Accept: application/vnd.github+json"])
        .arg(format!("https://api.github.com/repos/{REPO}/releases?per_page=15")).output().map_err(|e| e.to_string())?;
    if !out.status.success() { return Err("GitHub could not be reached".into()); }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| "GitHub could not be reached")?;
    let mut list: Vec<([u32; 3], Release)> = v.as_array().into_iter().flatten().filter_map(|r| {
        if r["draft"] == true || r["prerelease"] == true { return None; }
        let tag = r["tag_name"].as_str()?;
        let a = r["assets"].as_array()?.iter().find(|a| a["name"] == ASSET)?;
        Some((parse(tag)?, Release {
            version: tag.trim_start_matches(['v', 'V']).into(),
            date: r["published_at"].as_str()?.into(),
            notes: r["body"].as_str().unwrap_or("").into(),
            size: a["size"].as_u64()?,
            url: a["browser_download_url"].as_str()?.into(),
            sha: a["digest"].as_str()?.strip_prefix("sha256:")?.into(),
        }))
    }).collect();
    list.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(list.into_iter().map(|x| x.1).collect())
}
fn is_newer(r: &Release) -> bool { parse(&r.version) > parse(current()) }

#[tauri::command]
pub async fn updates_check() -> Result<Updates, String> {
    blocking(|| {
        let releases = releases()?;
        Ok(Updates { current: current().into(), newer: releases.first().map_or(false, is_newer), releases })
    }).await
}

/// downloads to `to`, reports progress, and only succeeds when size and checksum match
fn fetch(app: &AppHandle, event: &str, url: &str, sha: &str, size: u64, to: &Path) -> Result<(), String> {
    if !url.starts_with("https://github.com/") { return Err("Unexpected download address".into()); }
    let mut child = curl().args(["--max-time", "1800", "-o"]).arg(to).arg(url)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
    let ok = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? { break s.success(); }
        let have = fs::metadata(to).map(|m| m.len()).unwrap_or(0);
        let _ = app.emit(event, (have * 100 / size.max(1)).min(99));
        std::thread::sleep(Duration::from_millis(200));
    };
    let check = || -> Result<(), String> {
        if !ok { return Err("Download failed. Check your internet connection.".into()); }
        let mut f = fs::File::open(to).map_err(|e| e.to_string())?;
        let (mut h, mut buf, mut len) = (Sha256::new(), vec![0; 1 << 16], 0u64);
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 { break; }
            len += n as u64;
            h.update(&buf[..n]);
        }
        let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if len != size || !got.eq_ignore_ascii_case(sha) { return Err("The download is damaged, nothing was changed".into()); }
        Ok(())
    };
    let r = check();
    if r.is_err() { let _ = fs::remove_file(to); }
    r
}

#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    blocking(move || {
        let Ok(_one) = BUSY.try_lock() else { return Err("Already downloading".to_string()) };
        let rel = releases()?.into_iter().next().filter(is_newer).ok_or("You already have the newest version")?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let (new, old) = (exe.with_extension("new"), exe.with_extension("old"));
        fetch(&app, "update-progress", &rel.url, &rel.sha, rel.size, &new)?;
        let _ = fs::remove_file(&old);
        // a running exe may be renamed, just not overwritten
        if let Err(e) = fs::rename(&exe, &old) { let _ = fs::remove_file(&new); return Err(format!("The app folder is write-protected ({e})")); }
        if let Err(e) = fs::rename(&new, &exe) { let _ = fs::rename(&old, &exe); return Err(e.to_string()); }
        // the new one waits until this one has closed (it may still be writing a recording)
        if let Err(e) = Command::new(&exe).args(["--updated", &std::process::id().to_string()]).spawn() {
            let _ = fs::remove_file(&exe);
            let _ = fs::rename(&old, &exe);
            return Err(e.to_string());
        }
        crate::quit_app(&app);
        Ok(())
    }).await
}

/// first thing in main(): after an update, wait for the old version to close and remove it
pub fn finish() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pid) = args.iter().position(|a| a == "--updated").and_then(|i| args.get(i + 1)).and_then(|p| p.parse::<u32>().ok()) {
        use windows::Win32::System::Threading::*;
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
                WaitForSingleObject(h, 180_000);
                let _ = windows::Win32::Foundation::CloseHandle(h);
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if let Ok(exe) = std::env::current_exe() { let _ = fs::remove_file(exe.with_extension("old")); }
}

/// the recorder is not part of the exe (it is 100 MB), so it is loaded once when clips are first used
#[tauri::command]
pub async fn recorder_install(app: AppHandle) -> Result<(), String> {
    blocking(move || {
        let Ok(_one) = BUSY.try_lock() else { return Err("Already downloading".to_string()) };
        let dir = crate::clips::data().join("recorder-download");
        let mut last = String::new();
        for (url, sha, size) in FFMPEG {
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let file = dir.join(if url.ends_with(".7z") { "f.7z" } else { "f.zip" });
            let r = fetch(&app, "recorder-progress", url, sha, size, &file).and_then(|_| {
                // tar is part of Windows; older ones cannot read 7z, then the zip is used
                let ok = Command::new(system32("tar.exe")).current_dir(&dir).creation_flags(NO_WINDOW)
                    .arg("-xf").arg(&file).args(["--strip-components", "2", "*/bin/ffmpeg.exe"])
                    .stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
                let got = dir.join("ffmpeg.exe");
                if !ok || !got.exists() { return Err("Could not unpack the recorder".into()); }
                fs::rename(&got, crate::clips::data().join("ffmpeg.exe")).map_err(|e| e.to_string())
            });
            match r { Ok(()) => { let _ = fs::remove_dir_all(&dir); return Ok(()); } Err(e) => last = e }
        }
        let _ = fs::remove_dir_all(&dir);
        Err(last)
    }).await
}
