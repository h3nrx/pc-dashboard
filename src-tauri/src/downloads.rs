// Downloads tidy-up: sorts loose files into folders by type, can undo the last run,
// and offers old installers for the Recycle Bin. Only top-level files are touched.
use crate::{blocking, clips::data, home};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

const CATS: [(&str, &str, &str, &[&str]); 6] = [
    ("images", "Images", "Bilder", &["jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "svg", "ico", "tif", "tiff", "avif", "psd"]),
    ("videos", "Videos", "Videos", &["mp4", "mkv", "mov", "avi", "webm", "wmv", "flv", "m4v"]),
    ("music", "Music", "Musik", &["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus", "wma"]),
    ("documents", "Documents", "Dokumente", &["pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "odt", "ods", "csv", "rtf", "md", "epub"]),
    ("archives", "Archives", "Archive", &["zip", "rar", "7z", "tar", "gz", "xz", "bz2", "iso"]),
    ("installers", "Installers", "Installer", &["exe", "msi", "msix", "msixbundle", "appx", "appxbundle"]),
];
const PARTIAL: [&str; 6] = ["crdownload", "part", "tmp", "opdownload", "download", "partial"];

#[derive(Serialize)]
pub struct DlFile { name: String, size: u64, cat: String, days: u64 }
#[derive(Serialize)]
pub struct DlState { dir: String, files: Vec<DlFile>, old: Vec<DlFile>, can_undo: bool, auto: bool }
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Cfg { auto: bool }

fn cfg_path() -> PathBuf { data().join("downloads.json") }
fn undo_path() -> PathBuf { data().join("downloads-undo.json") }
fn load() -> Cfg { fs::read_to_string(cfg_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default() }

pub fn dir() -> PathBuf {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None) {
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as _));
            if !s.is_empty() { return PathBuf::from(s); }
        }
    }
    home().join("Downloads")
}

fn cat_of(name: &str) -> Option<&'static (&'static str, &'static str, &'static str, &'static [&'static str])> {
    let ext = Path::new(name).extension()?.to_string_lossy().to_lowercase();
    CATS.iter().find(|c| c.3.contains(&ext.as_str()))
}
fn folder_name(c: &(&str, &str, &str, &[&str])) -> &'static str {
    let c = CATS.iter().find(|x| x.0 == c.0).unwrap();
    if crate::clips::german() { c.2 } else { c.1 }
}

/// loose files that can be sorted: not hidden, not still downloading, not touched in the last 2 minutes
fn loose() -> Vec<(PathBuf, String, u64, u64)> {
    let now = SystemTime::now();
    let mut out = vec![];
    let Ok(rd) = fs::read_dir(dir()) else { return out };
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if !m.is_file() { continue; }
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x6 != 0 { continue; } // hidden / system
        let name = e.file_name().to_string_lossy().to_string();
        let ext = Path::new(&name).extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        if PARTIAL.contains(&ext.as_str()) || name.eq_ignore_ascii_case("desktop.ini") { continue; }
        let age = m.modified().ok().and_then(|t| now.duration_since(t).ok()).unwrap_or_default();
        if age < Duration::from_secs(120) { continue; }
        out.push((e.path(), name, m.len(), age.as_secs() / 86400));
    }
    out
}

fn state() -> DlState {
    let mut files = vec![];
    let mut old = vec![];
    for (_, name, size, days) in loose() {
        let Some(c) = cat_of(&name) else { continue };
        if c.0 == "installers" && days >= 30 { old.push(DlFile { name: name.clone(), size, cat: c.0.into(), days }); }
        files.push(DlFile { name, size, cat: c.0.into(), days });
    }
    files.sort_by(|a, b| a.cat.cmp(&b.cat).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    old.sort_by(|a, b| b.size.cmp(&a.size));
    DlState { dir: dir().to_string_lossy().to_string(), files, old, can_undo: undo_path().exists(), auto: load().auto }
}

fn free_name(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() { return p; }
    let (stem, ext) = (Path::new(name).file_stem().unwrap_or_default().to_string_lossy().to_string(), Path::new(name).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default());
    (2..).map(|i| dir.join(format!("{stem} ({i}){ext}"))).find(|p| !p.exists()).unwrap()
}

fn sort_now() -> Result<usize, String> {
    let base = dir();
    let mut moved: Vec<(String, String)> = vec![];
    for (path, name, _, _) in loose() {
        let Some(c) = cat_of(&name) else { continue };
        let target = base.join(folder_name(c));
        if fs::create_dir_all(&target).is_err() { continue; }
        let to = free_name(&target, &name);
        if fs::rename(&path, &to).is_ok() { moved.push((path.to_string_lossy().to_string(), to.to_string_lossy().to_string())); }
    }
    if !moved.is_empty() {
        let _ = fs::create_dir_all(data());
        fs::write(undo_path(), serde_json::to_string(&moved).unwrap_or_default()).map_err(|e| e.to_string())?;
    }
    Ok(moved.len())
}

#[tauri::command]
pub async fn downloads_state() -> DlState { blocking(state).await }

#[tauri::command]
pub async fn downloads_sort() -> Result<(usize, DlState), String> { blocking(|| sort_now().map(|n| (n, state()))).await }

#[tauri::command]
pub async fn downloads_undo() -> Result<(usize, DlState), String> {
    blocking(|| {
        let list: Vec<(String, String)> = fs::read_to_string(undo_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).ok_or("Nothing to undo")?;
        let base = dir();
        let mut n = 0;
        for (from, to) in list.iter().rev() {
            let (from, to) = (PathBuf::from(from), PathBuf::from(to));
            // only paths inside Downloads, in case the file was edited by hand
            if !from.starts_with(&base) || !to.starts_with(&base) || from.exists() || !to.exists() { continue; }
            if fs::rename(&to, &from).is_ok() { n += 1; if let Some(p) = to.parent() { let _ = fs::remove_dir(p); } } // folder only if empty
        }
        let _ = fs::remove_file(undo_path());
        Ok((n, state()))
    })
    .await
}

/// old installers to the Recycle Bin; the UI sends names, which are checked again here
#[tauri::command]
pub async fn downloads_recycle(names: Vec<String>) -> Result<(u64, DlState), String> {
    blocking(move || {
        let old: Vec<(PathBuf, u64)> = loose().into_iter()
            .filter(|(_, n, _, d)| *d >= 30 && cat_of(n).map(|c| c.0 == "installers").unwrap_or(false) && names.contains(n))
            .map(|(p, _, s, _)| (p, s)).collect();
        let mut freed = 0;
        for (p, s) in old { if trash::delete(&p).is_ok() { freed += s; } }
        Ok((freed, state()))
    })
    .await
}

#[tauri::command]
pub async fn downloads_auto(on: bool) -> Result<DlState, String> {
    blocking(move || {
        let _ = fs::create_dir_all(data());
        fs::write(cfg_path(), serde_json::to_string(&Cfg { auto: on }).unwrap_or_default()).map_err(|e| e.to_string())?;
        if on { let _ = sort_now(); }
        Ok(state())
    })
    .await
}

#[tauri::command]
pub fn downloads_open() { let _ = std::process::Command::new("explorer").arg(dir()).spawn(); }

/// with auto sort on, Downloads is tidied every 10 minutes
pub fn start() {
    thread::spawn(|| loop {
        if load().auto { let _ = sort_now(); }
        thread::sleep(Duration::from_secs(600));
    });
}
