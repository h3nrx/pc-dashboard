// Clips: records the whole screen into a short rolling buffer and turns the last seconds
// (or a manual recording) into an mp4. Video: ffmpeg (desktop duplication + GPU encoder), cut into
// 1 s pieces. Sound: WASAPI, written as 1 s raw pieces. Both use the same clock, so they line up.
use crate::{blocking, env_path, home, local, NO_WINDOW};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::JobObjects::*;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_APP, WM_HOTKEY};

const RATE: u64 = 48_000;
const FRAME: u64 = 4; // stereo, 16 bit
const CHUNK: u64 = RATE * FRAME;
const MAX_SECS: u32 = 600;
const MIN_FREE: u64 = 5 << 30;
const ENCODERS: usize = 4;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Cfg {
    on: bool, secs: u32, mic: bool, hotkey: String, rec_hotkey: String, encoder: usize,
    fps: u32, quality: usize, sound: bool, folder: String,
    lang: String, tray: bool, start_hidden: bool,
    name: String, look: String, fx: bool, hidden: Vec<String>,
}
impl Default for Cfg { fn default() -> Self { Cfg {
    on: false, secs: 30, mic: false, hotkey: "F9".into(), rec_hotkey: "F10".into(), encoder: 0,
    fps: 60, quality: 1, sound: true, folder: String::new(),
    lang: "en".into(), tray: true, start_hidden: true,
    name: String::new(), look: "chrome".into(), fx: true, hidden: vec![],
} } }

struct Session { dir: PathBuf, start: Instant, stop: Arc<AtomicBool>, child: Mutex<Child> }
impl Session {
    fn now(&self) -> f64 { self.start.elapsed().as_secs_f64() }
    fn close(&self) {
        self.stop.store(true, SeqCst);
        if let Ok(mut c) = self.child.lock() { let _ = c.kill(); let _ = c.wait(); }
    }
}

struct State { cfg: Cfg, session: Option<Arc<Session>>, rec: Option<f64>, rec_wish: bool, error: String, hotkey_error: String, quit: bool }
static ST: Mutex<Option<State>> = Mutex::new(None);
static SAVING: AtomicUsize = AtomicUsize::new(0);
static HOTKEY_THREAD: AtomicU32 = AtomicU32::new(0);
static APP: Mutex<Option<AppHandle>> = Mutex::new(None);

fn st<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(|| State {
        cfg: fs::read_to_string(cfg_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
        session: None, rec: None, rec_wish: false, error: String::new(), hotkey_error: String::new(), quit: false,
    }))
}
pub fn data() -> PathBuf {
    // the first version used another folder; it only counts while there is nothing newer
    let (new, old) = (local().join("pc-dashboard"), local().join("h3nrx-control"));
    if !new.join("clips.json").exists() && old.join("clips.json").exists() { old } else { new }
}
fn cfg_path() -> PathBuf { data().join("clips.json") }
fn buffer() -> PathBuf { data().join("clip-buffer") }
fn thumbs() -> PathBuf { data().join("thumbs") }
pub fn folder() -> PathBuf {
    let f = st(|s| s.cfg.folder.clone());
    if f.is_empty() { 
        // the first version used this name; keep it where it already exists
        let v = home().join("Videos");
        if v.join("h3nrx Clips").is_dir() { v.join("h3nrx Clips") } else { v.join("PC Dashboard Clips") }
    } else { f.into() }
}
pub fn german() -> bool { st(|s| s.cfg.lang == "de") }
pub fn tray() -> bool { st(|s| s.cfg.tray) }
pub fn start_hidden() -> bool { st(|s| s.cfg.start_hidden) }
fn ffmpeg() -> PathBuf {
    let near = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("ffmpeg.exe"))).unwrap_or_default();
    if near.exists() { near } else { data().join("ffmpeg.exe") }
}
fn save_cfg() -> Result<(), String> {
    let text = st(|s| serde_json::to_string_pretty(&s.cfg).unwrap_or_default());
    let path = cfg_path();
    let tmp = path.with_extension("json.tmp");
    let write = || -> std::io::Result<()> {
        fs::create_dir_all(data())?;
        let mut f = fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut f, text.as_bytes())?;
        f.sync_all()?;      // really on the disk before the old file is replaced
        drop(f);
        fs::rename(&tmp, &path)
    };
    write().map_err(|e| format!("Settings could not be saved ({e})"))
}
fn emit(kind: &str, text: &str) {
    if let Some(a) = APP.lock().unwrap_or_else(|e| e.into_inner()).as_ref() { let _ = a.emit("clips", (kind, text)); }
}

/// capture processes die together with this app, even if it crashes
fn tie_to_app(child: &Child) {
    static JOB: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let Ok(j) = CreateJobObjectW(None, PCWSTR::null()) else { return 0 };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let _ = SetInformationJobObject(j, JobObjectExtendedLimitInformation, &info as *const _ as _, std::mem::size_of_val(&info) as u32);
        j.0 as usize
    });
    if job != 0 { unsafe { let _ = AssignProcessToJobObject(HANDLE(job as _), HANDLE(child.as_raw_handle())); } }
}

/* ---------- capture ---------- */
fn open_session(cfg: &Cfg) -> Result<Session, String> {
    if !ffmpeg().exists() { return Err("The recorder is not installed yet".into()); }
    let base = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_err(|e| e.to_string())?.as_micros();
    let start = Instant::now();
    let dir = buffer().join(base.to_string());
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // stamp every frame with "seconds since base" so sound and picture share one clock
    let grab = format!("ddagrab=output_idx=0:framerate={}", cfg.fps);
    let q = cfg.quality.min(2);
    let (cq, rate, max) = (["28", "23", "19"][q], ["8M", "14M", "24M"][q], ["12M", "30M", "50M"][q]);
    let gop = cfg.fps.to_string();
    let stamp = format!("setpts='if(eq(N,0),st(0,RTCTIME-{base}),0)*0+PTS-STARTPTS+ld(0)/(TB*1000000)'");
    let (pre, enc): (&str, &[&str]) = match cfg.encoder {
        0 => ("", &["-c:v", "h264_nvenc", "-preset", "p4", "-rc", "vbr", "-cq", cq, "-b:v", "0"]),
        1 => ("", &["-c:v", "h264_amf", "-quality", "balanced", "-b:v", rate]),
        2 => (",hwdownload,format=bgra,format=nv12", &["-c:v", "h264_qsv", "-global_quality", cq]),
        _ => (",hwdownload,format=bgra,format=yuv420p", &["-c:v", "libx264", "-preset", "ultrafast", "-crf", cq]),
    };
    let mut c = Command::new(ffmpeg());
    c.current_dir(&dir).creation_flags(NO_WINDOW)
        .args(["-hide_banner", "-nostats", "-loglevel", "error", "-y", "-init_hw_device", "d3d11va", "-filter_complex"])
        .arg(format!("{grab},{stamp}{pre}"))
        .args(["-fps_mode", "passthrough"]).args(enc)
        .args(["-maxrate", max, "-bufsize", max, "-bf", "0", "-g", &gop])
        .args(["-f", "segment", "-segment_time", "1", "-segment_format", "matroska", "-segment_list", "list.csv",
               "-segment_list_type", "csv", "-segment_list_flags", "live", "v%06d.mkv"])
        .stdin(Stdio::piped()).stdout(Stdio::null())
        .stderr(File::create(dir.join("error.txt")).map_err(|e| e.to_string())?);
    let child = c.spawn().map_err(|e| format!("Recorder did not start: {e}"))?;
    tie_to_app(&child);
    let stop = Arc::new(AtomicBool::new(false));
    for (on, loopback, name) in [(true, true, "s"), (cfg.mic, false, "m")] {
        if !on { continue; }
        let (stop, dir) = (stop.clone(), dir.clone());
        thread::spawn(move || {
            unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
            let mut w = Pcm { dir, name, next: 0, open: None };
            while !stop.load(SeqCst) {
                let _ = unsafe { listen(loopback, start, &stop, &mut w) }; // ends when the device changes
                thread::sleep(Duration::from_millis(400));
            }
        });
    }
    Ok(Session { dir, start, stop, child: Mutex::new(child) })
}

/// raw sound, one file per second of session time; gaps simply stay silent
struct Pcm { dir: PathBuf, name: &'static str, next: u64, open: Option<(u64, File)> }
impl Pcm {
    fn write(&mut self, at: u64, mut bytes: &[u8]) -> std::io::Result<()> {
        // the sound card runs its own clock: only jump when it is clearly off (silence, hiccup)
        if self.next.abs_diff(at) > RATE / 10 { self.next = at; }
        while !bytes.is_empty() {
            let pos = self.next * FRAME;
            let (idx, off) = (pos / CHUNK, pos % CHUNK);
            if self.open.as_ref().map(|o| o.0) != Some(idx) {
                let f = OpenOptions::new().create(true).truncate(false).write(true).open(self.dir.join(format!("{}{idx:06}.pcm", self.name)))?;
                self.open = Some((idx, f));
            }
            let n = bytes.len().min((CHUNK - off) as usize);
            let f = &mut self.open.as_mut().unwrap().1;
            f.seek(SeekFrom::Start(off))?;
            f.write_all(&bytes[..n])?;
            bytes = &bytes[n..];
            self.next += n as u64 / FRAME;
        }
        Ok(())
    }
}

unsafe fn listen(loopback: bool, start: Instant, stop: &AtomicBool, w: &mut Pcm) -> windows::core::Result<()> {
    let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let flow = if loopback { eRender } else { eCapture };
    let id_of = |d: &IMMDevice| -> windows::core::Result<String> {
        let p = d.GetId()?;
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as _));
        Ok(s)
    };
    let dev = en.GetDefaultAudioEndpoint(flow, eConsole)?;
    let id = id_of(&dev)?;
    let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
    let fmt = WAVEFORMATEX { wFormatTag: 1, nChannels: 2, nSamplesPerSec: RATE as u32, nAvgBytesPerSec: CHUNK as u32, nBlockAlign: FRAME as u16, wBitsPerSample: 16, cbSize: 0 };
    let mut flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    if loopback { flags |= AUDCLNT_STREAMFLAGS_LOOPBACK; }
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 5_000_000, 0, &fmt, None)?;
    let cap: IAudioCaptureClient = client.GetService()?;
    client.Start()?;
    let mut checked = Instant::now();
    while !stop.load(SeqCst) {
        while cap.GetNextPacketSize()? > 0 {
            let (mut p, mut n, mut f) = (std::ptr::null_mut(), 0u32, 0u32);
            cap.GetBuffer(&mut p, &mut n, &mut f, None, None)?;
            if n > 0 {
                let at = ((start.elapsed().as_secs_f64() * RATE as f64) as u64).saturating_sub(n as u64);
                let len = n as usize * FRAME as usize;
                if f & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || p.is_null() { let _ = w.write(at, &vec![0; len]); }
                else { let _ = w.write(at, std::slice::from_raw_parts(p, len)); }
            }
            cap.ReleaseBuffer(n)?;
        }
        if checked.elapsed() > Duration::from_secs(2) {
            checked = Instant::now();
            if id_of(&en.GetDefaultAudioEndpoint(flow, eConsole)?)? != id { break; } // headset plugged in etc.
        }
        thread::sleep(Duration::from_millis(8));
    }
    let _ = client.Stop();
    Ok(())
}

/// finished video pieces: (number, start, end). Piece 0 has no reliable start, so it is left out.
fn pieces(s: &Session) -> Vec<(u64, f64, f64)> {
    fs::read_to_string(s.dir.join("list.csv")).unwrap_or_default().lines().filter_map(|l| {
        let mut p = l.split(',');
        let n: u64 = p.next()?.strip_prefix('v')?.strip_suffix(".mkv")?.parse().ok()?;
        Some((n, p.next()?.parse().ok()?, p.next()?.trim().parse().ok()?))
    }).filter(|p| p.0 > 0).collect()
}

fn supervise() {
    let (mut fails, mut wait_until, mut next_v, mut next_a, mut tick) = (0u32, Instant::now(), 0u64, 0u64, 0u32);
    loop {
        thread::sleep(Duration::from_millis(400));
        tick += 1;
        let (cfg, session, want, quit) = st(|s| (s.cfg.clone(), s.session.clone(), s.cfg.on || s.rec_wish, s.quit));
        if quit { return; }
        match session {
            None if want && Instant::now() >= wait_until => match open_session(&cfg) {
                Ok(s) => { next_v = 0; next_a = 0; st(|x| x.session = Some(Arc::new(s))); }
                Err(e) => { st(|x| x.error = e); wait_until = Instant::now() + Duration::from_secs(5); emit("state", ""); }
            },
            Some(s) if !want => { if SAVING.load(SeqCst) == 0 { shut(&s); st(|x| x.error.clear()); emit("state", ""); } }
            Some(s) => {
                let dead = s.child.lock().map(|mut c| !matches!(c.try_wait(), Ok(None))).unwrap_or(true);
                let list = if tick % 5 == 0 || dead { pieces(&s) } else { vec![] };
                let newest = list.last().map(|p| p.2);
                let stuck = tick % 5 == 0 && s.now() - newest.unwrap_or(0.0) > 12.0;
                if dead || stuck {
                    let why = fs::read_to_string(s.dir.join("error.txt")).unwrap_or_default();
                    if st(|x| x.rec.is_some()) { finish_recording(&s); }
                    shut(&s);
                    fails += 1;
                    // never produced anything: this PC may not have that encoder, try the next one
                    if newest.is_none() { st(|x| x.cfg.encoder = (x.cfg.encoder + 1) % ENCODERS); }
                    if fails > ENCODERS as u32 {
                        st(|x| x.error = format!("Screen capture stopped, retrying. {}", why.lines().last().unwrap_or("")));
                        emit("state", "");
                    }
                    wait_until = Instant::now() + Duration::from_secs(if fails > 4 { 5 } else { 1 });
                    continue;
                }
                if newest.is_some() {
                    if fails > 0 || st(|x| !x.error.is_empty()) { fails = 0; st(|x| x.error.clear()); let _ = save_cfg(); emit("state", ""); }
                    // forget what is too old, unless a recording still needs it
                    if SAVING.load(SeqCst) == 0 {
                        let keep = st(|x| { let k = s.now() - x.cfg.secs as f64 - 8.0; x.rec.map_or(k, |r| k.min(r - 2.0)) });
                        let from_v = next_v;
                        for p in list.iter().filter(|p| p.2 < keep && p.0 >= from_v) { let _ = fs::remove_file(s.dir.join(format!("v{:06}.mkv", p.0))); next_v = p.0 + 1; }
                        let _ = fs::remove_file(s.dir.join("v000000.mkv"));
                        while ((next_a + 2) as f64) < keep {
                            for n in ["s", "m"] { let _ = fs::remove_file(s.dir.join(format!("{n}{next_a:06}.pcm"))); }
                            next_a += 1;
                        }
                    }
                    if tick % 25 == 0 && st(|x| x.rec.is_some()) && crate::disk('C').free < MIN_FREE {
                        emit("error", "Almost no space left, recording stopped");
                        finish_recording(&s);
                    }
                }
            }
            _ => {}
        }
    }
}
fn shut(s: &Session) {
    s.close();
    st(|x| x.session = None);
    let dir = s.dir.clone();
    thread::spawn(move || {
        for _ in 0..60 {
            thread::sleep(Duration::from_secs(1));
            if SAVING.load(SeqCst) == 0 { let _ = fs::remove_dir_all(&dir); return; }
        }
    });
}
/// stops a manual recording and writes it in the background
fn finish_recording(s: &Arc<Session>) {
    let from = st(|x| { x.rec_wish = false; x.rec.take() });
    if let Some(from) = from {
        let (s, to) = (s.clone(), s.now());
        SAVING.fetch_add(1, SeqCst);
        thread::spawn(move || { report(write_clip(&s, from, to, "Recording")); SAVING.fetch_sub(1, SeqCst); });
    }
    emit("state", "");
}

/* ---------- saving ---------- */
fn stamp_name() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02} {:02}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}
fn report(r: Result<String, String>) {
    match r { Ok(n) => { emit("saved", &n); ding(true); } Err(e) => emit("error", &e) }
}
fn write_clip(s: &Session, from: f64, to: f64, label: &str) -> Result<String, String> {
    // wait for the piece that contains "now" to be finished
    let t0 = Instant::now();
    let mut list = pieces(s);
    while t0.elapsed() < Duration::from_secs(4) && list.last().map_or(true, |p| p.2 < to) {
        thread::sleep(Duration::from_millis(100));
        list = pieces(s);
        if s.stop.load(SeqCst) { break; }
    }
    let list: Vec<_> = list.into_iter().filter(|p| p.2 > from && p.1 < to && s.dir.join(format!("v{:06}.mkv", p.0)).exists()).collect();
    let (Some(first), Some(last)) = (list.first(), list.last()) else { return Err("Nothing recorded yet, try again in a few seconds".into()) };
    let (begin, end) = (first.1, last.2);
    let _ = fs::create_dir_all(folder());
    let mut name = format!("{label} {}.mp4", stamp_name());
    if folder().join(&name).exists() { name = name.replace(".mp4", " (2).mp4"); }
    let mut txt = String::from("ffconcat version 1.0\n");
    for (i, p) in list.iter().enumerate() {
        // up to the start of the next piece, so nothing drifts over long recordings
        let d = list.get(i + 1).map_or(p.2, |n| n.1) - p.1;
        txt += &format!("file v{:06}.mkv\nduration {d:.6}\n", p.0);
    }
    let concat = s.dir.join(format!("{}.ffconcat", name.replace([' ', '(', ')'], "_")));
    fs::write(&concat, txt).map_err(|e| e.to_string())?;
    let tmp = s.dir.join(format!("{}.part.mp4", name.replace([' ', '(', ')'], "_")));
    let mut child = Command::new(ffmpeg()).current_dir(&s.dir).creation_flags(NO_WINDOW)
        .args(["-hide_banner", "-nostats", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i"]).arg(&concat)
        .args(["-f", "s16le", "-ar", "48000", "-ac", "2", "-i", "pipe:0", "-map", "0:v", "-map", "1:a",
               "-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-colorspace", "bt470bg", "-color_range", "tv", "-movflags", "+faststart"]).arg(&tmp)
        .stdin(Stdio::piped()).stdout(Stdio::null())
        .stderr(File::create(s.dir.join("save-error.txt")).map_err(|e| e.to_string())?)
        .spawn().map_err(|e| e.to_string())?;
    tie_to_app(&child);
    if let Some(mut pipe) = child.stdin.take() {
        let (mut at, stop) = ((begin.max(0.0) * RATE as f64) as u64 * FRAME, (end * RATE as f64) as u64 * FRAME);
        while at < stop {
            let (idx, off) = (at / CHUNK, (at % CHUNK) as usize);
            let n = ((CHUNK as usize) - off).min((stop - at) as usize);
            let mut mix = vec![0u8; n];
            for src in ["s", "m"] {
                let Ok(mut f) = File::open(s.dir.join(format!("{src}{idx:06}.pcm"))) else { continue };
                let mut b = Vec::with_capacity(CHUNK as usize);
                if f.read_to_end(&mut b).is_err() { continue; }
                b.resize(CHUNK as usize, 0);
                for (o, i) in mix.chunks_exact_mut(2).zip(b[off..].chunks_exact(2)) {
                    let v = i16::from_le_bytes([o[0], o[1]]).saturating_add(i16::from_le_bytes([i[0], i[1]]));
                    o.copy_from_slice(&v.to_le_bytes());
                }
            }
            if pipe.write_all(&mix).is_err() { break; }
            at += n as u64;
        }
    }
    let ok = child.wait().map(|c| c.success()).unwrap_or(false);
    if !ok {
        let why = fs::read_to_string(s.dir.join("save-error.txt")).unwrap_or_default();
        return Err(format!("Clip could not be saved. {}", why.lines().last().unwrap_or("")));
    }
    let out = folder().join(&name);
    if fs::rename(&tmp, &out).is_err() {
        fs::copy(&tmp, &out).map_err(|e| e.to_string())?; // other drive
        let _ = fs::remove_file(&tmp);
    }
    let _ = fs::remove_file(concat);
    make_thumb(&out);
    Ok(name)
}
pub fn save_last() {
    let got = st(|s| if s.cfg.on { s.session.clone().map(|x| (x, s.cfg.secs)) } else { None });
    let Some((s, secs)) = got else { emit("error", "Clipping is off"); return };
    let to = s.now();
    SAVING.fetch_add(1, SeqCst);
    emit("saving", "");
    thread::spawn(move || { report(write_clip(&s, to - secs as f64, to, "Clip")); SAVING.fetch_sub(1, SeqCst); });
}

/// short soft two-tone sound so you know it worked without leaving the game
fn ding(done: bool) {
    if !st(|s| s.cfg.sound) { return; }
    let mut pcm = Vec::new();
    for i in 0..9600 {
        let t = i as f32 / 48000.0;
        let f = if (t < 0.08) == done { 880.0 } else { 1318.5 };
        let local = if t < 0.08 { t } else { t - 0.08 };
        let v = (t * f * std::f32::consts::TAU).sin() * (-local * 22.0).exp() * (local * 400.0).min(1.0) * 0.22;
        pcm.extend(((v * 32767.0) as i16).to_le_bytes());
    }
    let mut w = Vec::from(*b"RIFF");
    w.extend((36 + pcm.len() as u32).to_le_bytes());
    w.extend(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
    w.extend(48000u32.to_le_bytes()); w.extend(96000u32.to_le_bytes());
    w.extend(b"\x02\0\x10\0data"); w.extend((pcm.len() as u32).to_le_bytes()); w.extend(pcm);
    unsafe { let _ = PlaySoundW(PCWSTR(w.as_ptr() as _), None, SND_MEMORY | SND_SYNC | SND_NODEFAULT); }
}

/* ---------- hotkey ---------- */
fn parse_key(s: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let (mut m, mut vk) = (HOT_KEY_MODIFIERS(0), None);
    for p in s.split('+') {
        match p {
            "Ctrl" => m |= MOD_CONTROL, "Alt" => m |= MOD_ALT, "Shift" => m |= MOD_SHIFT,
            k => vk = Some(match k {
                "Insert" => 0x2D, "Delete" => 0x2E, "Home" => 0x24, "End" => 0x23, "PageUp" => 0x21, "PageDown" => 0x22, "Pause" => 0x13,
                k if k.len() > 1 && k.starts_with('F') => { let n: u32 = k[1..].parse().ok()?; if !(1..=24).contains(&n) { return None } 0x6F + n }
                k if k.len() == 1 && k.as_bytes()[0].is_ascii_alphanumeric() => k.to_ascii_uppercase().as_bytes()[0] as u32,
                _ => return None,
            }),
        }
    }
    let vk = vk?;
    // a bare letter would be swallowed everywhere you type
    if (0x30..=0x5A).contains(&vk) && !m.contains(MOD_CONTROL) && !m.contains(MOD_ALT) { return None; }
    Some((m, vk))
}
pub fn usable_key(s: &str) -> bool { parse_key(s).is_some() }
pub fn own_keys() -> [String; 2] { st(|s| [s.cfg.hotkey.clone(), s.cfg.rec_hotkey.clone()]) }
fn hotkeys() {
    unsafe {
        HOTKEY_THREAD.store(GetCurrentThreadId(), SeqCst);
        let register = || {
            let (on, clip, rec) = st(|s| (s.cfg.on, s.cfg.hotkey.clone(), s.cfg.rec_hotkey.clone()));
            let mut bad = vec![];
            // the clip key is only taken while clipping is on
            for (id, want, key) in [(1, on, clip), (2, true, rec)] {
                let _ = UnregisterHotKey(None, id);
                if !want || key.is_empty() { continue; }
                if !parse_key(&key).map_or(false, |(m, k)| RegisterHotKey(None, id, m | MOD_NOREPEAT, k).is_ok()) { bad.push(key); }
            }
            st(|s| s.hotkey_error = if bad.is_empty() { String::new() } else { format!("{} is already used by another app", bad.join(", ")) });
            emit("state", "");
        };
        register();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_HOTKEY { if msg.wParam.0 == 2 { record_bg() } else { save_last() } }
            else if msg.message == WM_APP { register(); }
        }
    }
}

/* ---------- clip list ---------- */
fn stem(p: &Path) -> String { p.file_stem().unwrap_or_default().to_string_lossy().to_string() }
fn find_thumb(st: &str) -> Option<(PathBuf, f64)> {
    let pre = format!("{st}~");
    fs::read_dir(thumbs()).ok()?.flatten().find_map(|e| {
        let n = e.file_name().to_string_lossy().to_string();
        let secs = n.strip_prefix(&pre)?.strip_suffix(".jpg")?.parse().ok()?;
        Some((e.path(), secs))
    })
}
fn make_thumb(p: &Path) -> Option<(PathBuf, f64)> {
    let _ = fs::create_dir_all(thumbs());
    let out = Command::new(ffmpeg()).creation_flags(NO_WINDOW).args(["-hide_banner", "-i"]).arg(p).output().ok()?;
    let txt = String::from_utf8_lossy(&out.stderr).to_string();
    let d = txt.split("Duration: ").nth(1)?.split(',').next()?.to_string();
    let secs = d.split(':').try_fold(0.0, |a, x| x.trim().parse::<f64>().ok().map(|v| a * 60.0 + v))?;
    let jpg = thumbs().join(format!("{}~{:.1}.jpg", stem(p), secs));
    Command::new(ffmpeg()).creation_flags(NO_WINDOW)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-ss", &format!("{:.2}", secs / 2.0), "-i"]).arg(p)
        .args(["-frames:v", "1", "-vf", "scale=480:-2", "-q:v", "4"]).arg(&jpg).output().ok()?;
    Some((jpg, secs))
}
#[derive(Serialize)]
pub struct Clip { name: String, path: String, thumb: String, secs: f64, size: u64, time: u64 }
#[derive(Serialize)]
pub struct ClipState { on: bool, active: bool, secs: u32, mic: bool, hotkey: String, rec_hotkey: String, fps: u32, quality: usize, sound: bool, folder: String, lang: String, tray: bool, start_hidden: bool, name: String, user: String, look: String, fx: bool, hidden: Vec<String>, recorder: bool, version: &'static str, recording: Option<f64>, saving: bool, error: String, hotkey_error: String }

fn state_now() -> ClipState {
    let (dir, recorder) = (folder().to_string_lossy().to_string(), ffmpeg().exists());
    st(|s| ClipState {
        on: s.cfg.on, active: s.session.is_some(), secs: s.cfg.secs, mic: s.cfg.mic, hotkey: s.cfg.hotkey.clone(),
        rec_hotkey: s.cfg.rec_hotkey.clone(), fps: s.cfg.fps, quality: s.cfg.quality, sound: s.cfg.sound, folder: dir,
        lang: s.cfg.lang.clone(), tray: s.cfg.tray, start_hidden: s.cfg.start_hidden,
        name: s.cfg.name.clone(), user: std::env::var("USERNAME").unwrap_or_default(), look: s.cfg.look.clone(), fx: s.cfg.fx,
        hidden: s.cfg.hidden.clone(), recorder, version: env!("CARGO_PKG_VERSION"),
        recording: if s.rec_wish { Some(match (&s.session, s.rec) { (Some(x), Some(r)) => x.now() - r, _ => 0.0 }) } else { None },
        saving: SAVING.load(SeqCst) > 0, error: s.error.clone(), hotkey_error: s.hotkey_error.clone(),
    })
}
#[tauri::command]
pub fn clips_state() -> ClipState { state_now() }

#[tauri::command]
pub async fn clips_list() -> Vec<Clip> {
    blocking(|| {
        let mut v: Vec<Clip> = fs::read_dir(folder()).into_iter().flatten().flatten().filter_map(|e| {
            let p = e.path();
            if !p.extension()?.eq_ignore_ascii_case("mp4") { return None; }
            let m = e.metadata().ok()?;
            let (thumb, secs) = find_thumb(&stem(&p)).or_else(|| make_thumb(&p)).unwrap_or_default();
            let time = m.modified().ok()?.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_millis() as u64;
            Some(Clip { name: stem(&p), path: p.to_string_lossy().into(), thumb: thumb.to_string_lossy().into(), secs, size: m.len(), time })
        }).collect();
        v.sort_by(|a, b| b.time.cmp(&a.time));
        v
    }).await
}

#[tauri::command]
pub async fn clips_config(patch: serde_json::Map<String, serde_json::Value>) -> Result<ClipState, String> {
    blocking(move || {
        let old = st(|s| s.cfg.clone());
        let mut v = serde_json::to_value(&old).map_err(|e| e.to_string())?;
        for (k, x) in patch {
            // only known settings; the encoder is picked automatically, the folder has its own dialog
            if v.get(&k).is_some() && k != "encoder" && k != "folder" { v[k] = x; }
        }
        let mut new: Cfg = serde_json::from_value(v).map_err(|e| e.to_string())?;
        for k in [&new.hotkey, &new.rec_hotkey] {
            if !k.is_empty() && parse_key(k).is_none() { return Err("That key can't be used. Try F9 or Alt + a letter.".to_string()); }
        }
        if !new.hotkey.is_empty() && new.hotkey == new.rec_hotkey { return Err("Clip and recording need different keys.".into()); }
        new.secs = new.secs.clamp(5, MAX_SECS);
        if new.fps != 30 { new.fps = 60; }
        new.quality = new.quality.min(2);
        new.name = new.name.trim().chars().take(20).collect();
        // overview and settings always stay, otherwise you could lock yourself out
        new.hidden.retain(|h| ["clips", "storage", "autostart", "themes", "jarvis"].contains(&h.as_str()));
        new.hidden.sort(); new.hidden.dedup();
        let restart = (old.mic, old.fps, old.quality) != (new.mic, new.fps, new.quality);
        let (session, rec) = st(|s| { s.cfg = new; (s.session.clone(), s.rec_wish) });
        let saved = save_cfg();
        // these only apply to a fresh buffer (never in the middle of a recording)
        if let (true, false, Some(s)) = (restart, rec, session) { if SAVING.load(SeqCst) == 0 { shut(&s); } }
        rebind();
        saved?;
        Ok(state_now())
    }).await
}
fn rebind() {
    let t = HOTKEY_THREAD.load(SeqCst);
    if t != 0 { unsafe { let _ = PostThreadMessageW(t, WM_APP, Default::default(), Default::default()); } }
    thread::sleep(Duration::from_millis(150));
}

/// lets you pick where clips are saved (Windows folder dialog)
#[tauri::command]
pub async fn clips_folder(app: AppHandle) -> Result<ClipState, String> {
    let owner = app.get_webview_window("main").and_then(|w| w.hwnd().ok()).map(|h| h.0 as isize).unwrap_or(0);
    blocking(move || {
        let picked = unsafe { pick_folder(owner) };
        if let Some(p) = picked {
            let p = PathBuf::from(p);
            if !p.is_dir() { return Err("That folder doesn't exist anymore".to_string()); }
            let _ = app.asset_protocol_scope().allow_directory(&p, false);
            st(|s| s.cfg.folder = p.to_string_lossy().to_string());
            save_cfg()?;
        }
        Ok(state_now())
    }).await
}
unsafe fn pick_folder(owner: isize) -> Option<String> {
    use windows::Win32::UI::Shell::*;
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    let d: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
    d.SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
    d.Show(Some(windows::Win32::Foundation::HWND(owner as _))).ok()?;
    let p = d.GetResult().ok()?.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let s = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as _));
    s
}

#[tauri::command]
pub fn clips_save() { save_last(); }

#[tauri::command]
pub async fn clips_record() -> Result<ClipState, String> { blocking(|| toggle_record().map(|_| state_now())).await }

pub fn record_bg() {
    thread::spawn(|| if let Err(e) = toggle_record() { emit("error", &e); });
}
fn toggle_record() -> Result<(), String> {
    static BUSY: Mutex<()> = Mutex::new(());
    let Ok(_one) = BUSY.try_lock() else { return Ok(()) }; // still starting, ignore the second press
    if st(|s| s.rec_wish) {
        match st(|s| s.session.clone()) { Some(s) => finish_recording(&s), None => st(|s| { s.rec_wish = false; s.rec = None }) }
        return Ok(());
    }
    if !ffmpeg().exists() { return Err("The recorder is not installed yet".into()); }
    if crate::disk('C').free < MIN_FREE { return Err("Not enough free space to record".into()); }
    st(|s| s.rec_wish = true);
    emit("state", "");
    // the recorder may have to start first
    let t = Instant::now();
    loop {
        if let Some(s) = st(|x| x.session.clone()) {
            if !pieces(&s).is_empty() || s.dir.join("v000001.mkv").exists() {
                ding(false);
                st(|x| if x.rec_wish { x.rec = Some(s.now()) });
                emit("recording", "");
                return Ok(());
            }
        }
        if t.elapsed() > Duration::from_secs(20) { st(|s| s.rec_wish = false); emit("state", ""); return Err("The recorder did not start".into()); }
        thread::sleep(Duration::from_millis(100));
    }
}

/// the UI only ever sends the name of a clip, never a path
fn clip_path(name: &str) -> Result<PathBuf, String> {
    if name.is_empty() || name.contains(['/', '\\', ':']) || name.contains("..") { return Err("Invalid name".into()); }
    let p = folder().join(format!("{name}.mp4"));
    if p.is_file() { Ok(p) } else { Err("That clip is gone".into()) }
}
#[tauri::command]
pub async fn clips_delete(name: String) -> Result<(), String> {
    blocking(move || {
        let p = clip_path(&name)?;
        trash::delete(&p).map_err(|e| e.to_string())?;
        if let Some((t, _)) = find_thumb(&name) { let _ = fs::remove_file(t); }
        Ok(())
    }).await
}
#[tauri::command]
pub async fn clips_copy(name: String) -> Result<(), String> {
    blocking(move || {
        let p = clip_path(&name)?.to_string_lossy().replace('\'', "''");
        crate::run("powershell", &["-NoProfile", "-Command", &format!("Set-Clipboard -LiteralPath '{p}'")]).map(|_| ())
    }).await
}
#[tauri::command]
pub fn clips_show(name: String) -> Result<(), String> {
    let mut c = Command::new(env_path("WINDIR").join("explorer.exe"));
    if name.is_empty() { let _ = fs::create_dir_all(folder()); c.arg(folder()); }
    else { c.raw_arg(format!("/select,\"{}\"", clip_path(&name)?.to_string_lossy())); }
    c.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/* ---------- lifecycle ---------- */
pub fn start(app: &AppHandle) {
    let _ = fs::remove_dir_all(buffer()); // leftovers from a crash
    let _ = fs::create_dir_all(folder());
    let _ = fs::create_dir_all(thumbs());
    let scope = app.asset_protocol_scope();
    let _ = scope.allow_directory(folder(), false);
    let _ = scope.allow_directory(thumbs(), false);
    *APP.lock().unwrap_or_else(|e| e.into_inner()) = Some(app.clone());
    thread::spawn(supervise);
    thread::spawn(hotkeys);
}
/// finishes a running recording and cleans up; blocks until everything is on disk
pub fn stop() {
    if let Some(s) = st(|s| s.session.clone()) {
        if st(|x| x.rec.is_some()) { finish_recording(&s); }
    }
    let t = Instant::now();
    while SAVING.load(SeqCst) > 0 && t.elapsed() < Duration::from_secs(120) { thread::sleep(Duration::from_millis(100)); }
    st(|s| s.quit = true);
    if let Some(s) = st(|s| s.session.take()) { s.close(); }
    thread::sleep(Duration::from_millis(60));
    let _ = fs::remove_dir_all(buffer());
}
