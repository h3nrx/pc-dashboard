// Volume per app, like the Windows volume mixer: the sound sessions of the default speakers.
use crate::blocking;
use serde::Serialize;
use windows::core::{Interface, GUID};
use windows::Win32::Foundation::{CloseHandle, S_OK};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};

#[derive(Serialize)]
pub struct AppVol { key: String, name: String, path: String, volume: f32, muted: bool, playing: bool }
#[derive(Serialize)]
pub struct Mixer { device: String, master: f32, muted: bool, apps: Vec<AppVol> }

pub fn com() { unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); } }

/// friendly name of a sound device, like "Speakers (Realtek(R) Audio)"
pub unsafe fn device_name(d: &IMMDevice) -> String {
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    let Ok(store) = d.OpenPropertyStore(STGM_READ) else { return String::new() };
    let Ok(v) = store.GetValue(&PKEY_Device_FriendlyName) else { return String::new() };
    let Ok(p) = PropVariantToStringAlloc(&v) else { return String::new() };
    let s = p.to_string().unwrap_or_default();
    CoTaskMemFree(Some(p.0 as _));
    s
}
fn exe_of(pid: u32) -> String {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 1024];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut n).is_ok();
        let _ = CloseHandle(h);
        if ok { String::from_utf16_lossy(&buf[..n as usize]) } else { String::new() }
    }
}

/// a readable name for an exe
pub fn pretty(stem: &str) -> String {
    let known = [
        ("chrome", "Chrome"), ("msedge", "Edge"), ("firefox", "Firefox"), ("brave", "Brave"), ("opera", "Opera"),
        ("discord", "Discord"), ("spotify", "Spotify"), ("steam", "Steam"), ("steamwebhelper", "Steam"),
        ("robloxplayerbeta", "Roblox"), ("fortniteclient-win64-shipping", "Fortnite"), ("epicgameslauncher", "Epic Games"),
        ("vlc", "VLC"), ("whatsapp.root", "WhatsApp"), ("whatsapp", "WhatsApp"), ("pc dashboard", "PC Dashboard"), ("msedgewebview2", "Web apps"),
    ];
    let l = stem.to_lowercase();
    known.iter().find(|(k, _)| *k == l).map(|(_, v)| v.to_string()).unwrap_or_else(|| {
        let mut c = stem.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    })
}

struct Sess { key: String, path: String, vol: ISimpleAudioVolume, playing: bool }

unsafe fn speakers() -> windows::core::Result<IMMDevice> {
    let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    en.GetDefaultAudioEndpoint(eRender, eMultimedia)
}

unsafe fn sessions(dev: &IMMDevice) -> windows::core::Result<Vec<Sess>> {
    let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None)?;
    let list = mgr.GetSessionEnumerator()?;
    let mut out = vec![];
    for i in 0..list.GetCount()? {
        let Ok(c) = list.GetSession(i) else { continue };
        let state = c.GetState().unwrap_or(AudioSessionStateExpired);
        if state == AudioSessionStateExpired { continue; }
        let Ok(c2) = c.cast::<IAudioSessionControl2>() else { continue };
        let Ok(vol) = c.cast::<ISimpleAudioVolume>() else { continue };
        let playing = state == AudioSessionStateActive;
        if c2.IsSystemSoundsSession() == S_OK {
            out.push(Sess { key: "system".into(), path: String::new(), vol, playing });
            continue;
        }
        let pid = c2.GetProcessId().unwrap_or(0);
        if pid == 0 { continue; }
        let path = exe_of(pid);
        let stem = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if stem.is_empty() { continue; }
        out.push(Sess { key: stem.to_lowercase(), path, vol, playing });
    }
    Ok(out)
}

fn read() -> Result<Mixer, String> {
    com();
    unsafe {
        let dev = speakers().map_err(|_| "No speakers found".to_string())?;
        let ep: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None).map_err(|e| e.to_string())?;
        let mut apps: Vec<AppVol> = vec![];
        for s in sessions(&dev).map_err(|e| e.to_string())? {
            // one row per program, even if it has several sound streams (Discord, browsers)
            if let Some(a) = apps.iter_mut().find(|a| a.key == s.key) { a.playing |= s.playing; continue; }
            let name = if s.key == "system" { "System sounds".to_string() } else {
                pretty(std::path::Path::new(&s.path).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default().as_str())
            };
            apps.push(AppVol {
                key: s.key, name, path: s.path,
                volume: s.vol.GetMasterVolume().unwrap_or(1.0), muted: s.vol.GetMute().map(|b| b.as_bool()).unwrap_or(false), playing: s.playing,
            });
        }
        // this app itself is not interesting, system sounds go last
        apps.retain(|a| a.key != "pc dashboard");
        apps.sort_by(|a, b| (a.key == "system").cmp(&(b.key == "system")).then(b.playing.cmp(&a.playing)).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(Mixer {
            device: device_name(&dev),
            master: ep.GetMasterVolumeLevelScalar().unwrap_or(0.0),
            muted: ep.GetMute().map(|b| b.as_bool()).unwrap_or(false),
            apps,
        })
    }
}

#[tauri::command]
pub async fn volume_list() -> Result<Mixer, String> { blocking(read).await }

/// key "master" is the whole PC, anything else one program
#[tauri::command]
pub async fn volume_set(key: String, volume: Option<f32>, muted: Option<bool>) -> Result<(), String> {
    blocking(move || {
        com();
        unsafe {
            let dev = speakers().map_err(|e| e.to_string())?;
            let ctx: *const GUID = std::ptr::null();
            if key == "master" {
                let ep: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None).map_err(|e| e.to_string())?;
                if let Some(v) = volume { ep.SetMasterVolumeLevelScalar(v.clamp(0.0, 1.0), ctx).map_err(|e| e.to_string())?; }
                if let Some(m) = muted { ep.SetMute(m, ctx).map_err(|e| e.to_string())?; }
                return Ok(());
            }
            for s in sessions(&dev).map_err(|e| e.to_string())?.into_iter().filter(|s| s.key == key) {
                if let Some(v) = volume { let _ = s.vol.SetMasterVolume(v.clamp(0.0, 1.0), ctx); }
                if let Some(m) = muted { let _ = s.vol.SetMute(m, ctx); }
            }
            Ok(())
        }
    })
    .await
}
