// What is playing right now (Spotify first, otherwise whatever Windows shows in its media flyout).
// Uses the same Windows media controls as the volume flyout, so no Spotify login is needed.
use crate::blocking;
use serde::Serialize;
use std::sync::Mutex;
use windows::Media::Control::*;
use windows::Storage::Streams::DataReader;

#[derive(Serialize, Default)]
pub struct Now {
    app: String, title: String, artist: String, album: String, playing: bool,
    pos: f64, len: f64, art: String, can_next: bool, can_prev: bool,
}

static ART: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len() * 4 / 3 + 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 { s.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' }); }
    }
    s
}

fn session() -> windows::core::Result<GlobalSystemMediaTransportControlsSession> {
    crate::audio::com();
    let mgr = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
    if let Ok(list) = mgr.GetSessions() {
        for s in list {
            if s.SourceAppUserModelId().map(|id| id.to_string().to_lowercase().contains("spotify")).unwrap_or(false) { return Ok(s); }
        }
    }
    mgr.GetCurrentSession()
}

fn app_name(id: &str) -> String {
    let l = id.to_lowercase();
    for (k, v) in [("spotify", "Spotify"), ("chrome", "Chrome"), ("msedge", "Edge"), ("firefox", "Firefox"), ("brave", "Brave"), ("vlc", "VLC"), ("zunemusic", "Media Player"), ("music", "Media Player")] {
        if l.contains(k) { return v.into(); }
    }
    crate::audio::pretty(id.split(['!', '.', '\\']).next().unwrap_or(id))
}

fn now() -> windows::core::Result<Now> {
    let s = session()?;
    let p = s.TryGetMediaPropertiesAsync()?.join()?;
    let (title, artist, album) = (p.Title()?.to_string(), p.Artist()?.to_string(), p.AlbumTitle().map(|a| a.to_string()).unwrap_or_default());
    let app = app_name(&s.SourceAppUserModelId()?.to_string());
    let info = s.GetPlaybackInfo()?;
    let playing = info.PlaybackStatus()? == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
    let ctl = info.Controls()?;
    let t = s.GetTimelineProperties()?;
    let (mut pos, len) = (t.Position()?.Duration as f64 / 1e7, t.EndTime()?.Duration as f64 / 1e7);
    if playing {
        // the position is only updated now and then; add the time since then
        let upd = t.LastUpdatedTime()?.UniversalTime;
        let ft = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as i64 / 100 + 116_444_736_000_000_000).unwrap_or(upd);
        if upd > 0 && ft > upd { pos += (ft - upd) as f64 / 1e7; }
    }
    // the cover is only read when the song changes
    let key = format!("{app}|{title}|{artist}");
    let mut art = ART.lock().unwrap_or_else(|e| e.into_inner());
    if art.0 != key {
        let mut url = String::new();
        if let Ok(r) = p.Thumbnail() {
            if let Ok(st) = r.OpenReadAsync().and_then(|a| a.join()) {
                let size = st.Size().unwrap_or(0) as u32;
                if size > 0 && size < 4 << 20 {
                    if let Ok(rd) = DataReader::CreateDataReader(&st) {
                        if rd.LoadAsync(size).and_then(|a| a.join()).is_ok() {
                            let mut buf = vec![0u8; size as usize];
                            if rd.ReadBytes(&mut buf).is_ok() {
                                let ct = st.ContentType().map(|c| c.to_string()).unwrap_or_default();
                                url = format!("data:{};base64,{}", if ct.starts_with("image/") { ct } else { "image/png".into() }, b64(&buf));
                            }
                        }
                    }
                }
            }
        }
        *art = (key, url);
    }
    Ok(Now {
        app, title, artist, album, playing, pos: if len > 0.0 { pos.min(len) } else { pos }, len, art: art.1.clone(),
        can_next: ctl.IsNextEnabled().unwrap_or(false), can_prev: ctl.IsPreviousEnabled().unwrap_or(false),
    })
}

#[tauri::command]
pub async fn media_now() -> Option<Now> { blocking(|| now().ok().filter(|n| !n.title.is_empty())).await }

#[tauri::command]
pub async fn media_cmd(cmd: String, at: Option<f64>) -> Result<(), String> {
    blocking(move || {
        let s = session().map_err(|_| "Nothing is playing".to_string())?;
        let r = match cmd.as_str() {
            "toggle" => s.TryTogglePlayPauseAsync().and_then(|a| a.join()),
            "next" => s.TrySkipNextAsync().and_then(|a| a.join()),
            "prev" => s.TrySkipPreviousAsync().and_then(|a| a.join()),
            "seek" => s.TryChangePlaybackPositionAsync((at.unwrap_or(0.0).max(0.0) * 1e7) as i64).and_then(|a| a.join()),
            _ => return Err("unknown".into()),
        };
        r.map(|_| ()).map_err(|e| e.to_string())
    })
    .await
}
