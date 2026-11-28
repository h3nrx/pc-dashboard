// Shows the mirrored picture: ffmpeg decodes the H.264 stream to raw pixels, a plain Win32 window
// draws them, borderless inside, always in the right shape. A small bar shows while the mouse moves:
// fullscreen (also F11 / double-click), always on top, disconnect.
use super::mirror::Sink;
use crate::NO_WINDOW;
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_F11};
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_FRAME: u32 = WM_APP + 1;
const WM_SHAPE: u32 = WM_APP + 2;
const WM_QUIET_CLOSE: u32 = WM_APP + 3; // the session ended by itself: just close
const BAR_TIMER: usize = 1;

/// the newest decoded picture
struct Frame { px: Vec<u8>, w: u32, h: u32 }

/// everything the window needs; only one mirror window exists at a time
struct Win {
    frame: Arc<Mutex<Frame>>,
    stop: Arc<AtomicBool>,
    on_top: bool,
    full: Option<(isize, RECT)>, // style and place before fullscreen
    bar_until: Instant,
    hover: i32,
    title: String,
    shape: (u32, u32), // picture size, applied when the window is ready
}
static WIN: Mutex<Option<Win>> = Mutex::new(None);
static HWND_NOW: AtomicIsize = AtomicIsize::new(0);

fn hwnd() -> Option<HWND> { match HWND_NOW.load(SeqCst) { 0 => None, h => Some(HWND(h as _)) } }
fn de() -> bool { crate::clips::german() }

struct Decoder { child: Child, stdin: ChildStdin }
impl Drop for Decoder { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }

pub struct Player { size: (u32, u32), dec: Option<Decoder>, frame: Arc<Mutex<Frame>> }

impl Player {
    pub fn new(device: String, stop: Arc<AtomicBool>, on_top: bool) -> Player {
        // an old window (previous session) goes first
        if let Some(old) = hwnd() {
            unsafe { let _ = PostMessageW(Some(old), WM_QUIET_CLOSE, WPARAM(0), LPARAM(0)); }
            let t = Instant::now();
            while hwnd().is_some() && t.elapsed() < Duration::from_secs(2) { thread::sleep(Duration::from_millis(20)); }
        }
        let frame = Arc::new(Mutex::new(Frame { px: vec![], w: 0, h: 0 }));
        let title = format!("AirPlay · {device}");
        *WIN.lock().unwrap_or_else(|e| e.into_inner()) = Some(Win {
            frame: frame.clone(), stop, on_top, full: None, bar_until: Instant::now(), hover: -1, title, shape: (0, 0),
        });
        thread::spawn(window_thread);
        Player { size: (0, 0), dec: None, frame }
    }
    fn start_decoder(&mut self) {
        self.dec = None;
        let (w, h) = self.size;
        let Ok(mut child) = Command::new(crate::clips::ffmpeg()).creation_flags(NO_WINDOW)
            .args(["-hide_banner", "-loglevel", "error", "-flags", "low_delay", "-probesize", "32", "-analyzeduration", "0",
                   "-f", "h264", "-i", "pipe:0", "-vf"])
            .arg(format!("scale={w}:{h}:flags=bicubic,format=bgra"))
            .args(["-f", "rawvideo", "-pix_fmt", "bgra", "pipe:1"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else { return };
        let (Some(stdin), Some(mut out)) = (child.stdin.take(), child.stdout.take()) else { return };
        let frame = self.frame.clone();
        thread::spawn(move || {
            let mut buf = vec![0u8; (w * h * 4) as usize];
            while out.read_exact(&mut buf).is_ok() {
                {
                    let mut f = frame.lock().unwrap_or_else(|e| e.into_inner());
                    if (f.w, f.h) != (w, h) { f.w = w; f.h = h; f.px = vec![0; buf.len()]; }
                    std::mem::swap(&mut f.px, &mut buf);
                }
                if let Some(h) = hwnd() { unsafe { let _ = PostMessageW(Some(h), WM_FRAME, WPARAM(0), LPARAM(0)); } }
            }
        });
        self.dec = Some(Decoder { child, stdin });
    }
}
impl Sink for Player {
    fn format(&mut self, w: u32, h: u32) {
        // even sizes, a sane range; a new size means the iPad was turned
        let (w, h) = (w.clamp(16, 4096) & !1, h.clamp(16, 4096) & !1);
        if (w, h) == self.size { return; }
        self.size = (w, h);
        with(|x| x.shape = (w, h));
        self.start_decoder();
        if let Some(hw) = hwnd() { unsafe { let _ = PostMessageW(Some(hw), WM_SHAPE, WPARAM(w as usize), LPARAM(h as isize)); } }
    }
    fn frame(&mut self, b: &[u8]) {
        if self.size == (0, 0) { self.size = (1920, 1080); self.start_decoder(); }
        let bad = match self.dec.as_mut() { Some(d) => d.stdin.write_all(b).is_err(), None => true };
        if bad { self.dec = None; }
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.dec = None;
        // only our own window: a newer session may already have its own
        let mine = with(|w| Arc::ptr_eq(&w.frame, &self.frame)).unwrap_or(false);
        if let (true, Some(h)) = (mine, hwnd()) { unsafe { let _ = PostMessageW(Some(h), WM_QUIET_CLOSE, WPARAM(0), LPARAM(0)); } }
    }
}

/* ---------- window ---------- */
fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(Some(0)).collect() }
fn with<R>(f: impl FnOnce(&mut Win) -> R) -> Option<R> { WIN.lock().unwrap_or_else(|e| e.into_inner()).as_mut().map(f) }

fn window_thread() {
    unsafe {
        let Ok(inst) = GetModuleHandleW(None) else { return };
        let class = w!("PCDashboardAirPlay");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32, style: CS_DBLCLKS | CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc_), hInstance: inst.into(), lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: LoadIconW(Some(inst.into()), PCWSTR(32512 as _)).or_else(|_| LoadIconW(Some(inst.into()), PCWSTR(1 as _))).unwrap_or_default(),
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0), ..Default::default()
        };
        RegisterClassExW(&wc); // fails harmlessly the second time
        let title = wide(&with(|w| w.title.clone()).unwrap_or_default());
        // start in the middle of the screen, 16:9, two thirds of its width
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let (ww, wh) = (sw * 2 / 3, sw * 2 / 3 * 9 / 16);
        let Ok(h) = CreateWindowExW(WINDOW_EX_STYLE(0), class, PCWSTR(title.as_ptr()), WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            (sw - ww) / 2, (sh - wh) / 2, ww, wh, None, None, Some(inst.into()), None) else { return };
        HWND_NOW.store(h.0 as isize, SeqCst);
        let (pw, ph) = with(|w| w.shape).unwrap_or((0, 0));
        if pw > 0 { reshape(h, pw as i32, ph as i32); }
        if with(|w| w.on_top).unwrap_or(false) { let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE); }
        let _ = SetForegroundWindow(h);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        HWND_NOW.store(0, SeqCst);
    }
}

/// where the picture goes inside the window: as big as possible, never stretched
fn fit(cw: i32, ch: i32, w: u32, h: u32) -> RECT {
    if w == 0 || h == 0 { return RECT { left: 0, top: 0, right: cw, bottom: ch }; }
    let s = (cw as f64 / w as f64).min(ch as f64 / h as f64);
    let (dw, dh) = ((w as f64 * s) as i32, (h as f64 * s) as i32);
    RECT { left: (cw - dw) / 2, top: (ch - dh) / 2, right: (cw - dw) / 2 + dw, bottom: (ch - dh) / 2 + dh }
}
/// the three buttons of the bar, bottom middle
fn buttons(cw: i32, ch: i32) -> [RECT; 3] {
    let (bw, bh) = (132, 40);
    let x0 = (cw - bw * 3 - 16) / 2;
    let y = ch - bh - 22;
    [0, 1, 2].map(|i| RECT { left: x0 + 8 + i * bw, top: y, right: x0 + 8 + (i + 1) * bw, bottom: y + bh })
}
fn labels(full: bool, top: bool) -> [String; 3] {
    let g = de();
    [
        (if full { if g { "Vollbild aus" } else { "Exit fullscreen" } } else if g { "Vollbild" } else { "Fullscreen" }).to_string(),
        format!("{}{}", if top { "✓ " } else { "" }, if g { "Im Vordergrund" } else { "On top" }),
        (if g { "Trennen" } else { "Disconnect" }).to_string(),
    ]
}

unsafe fn paint(h: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let dc = BeginPaint(h, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(h, &mut rc);
    let (cw, ch) = (rc.right.max(1), rc.bottom.max(1));
    // draw off-screen first, so the bar never flickers
    let mem = CreateCompatibleDC(Some(dc));
    let bmp = CreateCompatibleBitmap(dc, cw, ch);
    let old = SelectObject(mem, bmp.into());
    FillRect(mem, &rc, HBRUSH(GetStockObject(BLACK_BRUSH).0));
    let (frame, show_bar, full, top, hover) = with(|w| (w.frame.clone(), Instant::now() < w.bar_until, w.full.is_some(), w.on_top, w.hover)).unwrap_or((Arc::new(Mutex::new(Frame { px: vec![], w: 0, h: 0 })), false, false, false, -1));
    {
        let f = frame.lock().unwrap_or_else(|e| e.into_inner());
        if f.w > 0 && f.px.len() == (f.w * f.h * 4) as usize {
            let d = fit(cw, ch, f.w, f.h);
            let mut bi = BITMAPINFO::default();
            bi.bmiHeader = BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: f.w as i32, biHeight: -(f.h as i32), biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default() };
            SetStretchBltMode(mem, HALFTONE);
            let _ = SetBrushOrgEx(mem, 0, 0, None);
            StretchDIBits(mem, d.left, d.top, d.right - d.left, d.bottom - d.top, 0, 0, f.w as i32, f.h as i32,
                Some(f.px.as_ptr() as _), &bi, DIB_RGB_COLORS, SRCCOPY);
        } else {
            // waiting for the first picture
            SetBkMode(mem, TRANSPARENT);
            SetTextColor(mem, COLORREF(0x00A0A0A0));
            let font = CreateFontW(22, 0, 0, 0, 500, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, w!("Segoe UI"));
            let of = SelectObject(mem, font.into());
            let mut t: Vec<u16> = (if de() { "Verbindet …" } else { "Connecting …" }).encode_utf16().collect();
            let mut r = rc;
            DrawTextW(mem, &mut t, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            SelectObject(mem, of);
            let _ = DeleteObject(font.into());
        }
    }
    if show_bar {
        let bs = buttons(cw, ch);
        let back = CreateSolidBrush(COLORREF(0x001E1C1C));
        let lit = CreateSolidBrush(COLORREF(0x00363434));
        let ob = SelectObject(mem, back.into());
        let op = SelectObject(mem, GetStockObject(NULL_PEN));
        let _ = RoundRect(mem, bs[0].left - 8, bs[0].top - 6, bs[2].right + 8, bs[2].bottom + 6, 22, 22);
        let font = CreateFontW(17, 0, 0, 0, 600, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, 0, w!("Segoe UI"));
        let of = SelectObject(mem, font.into());
        SetBkMode(mem, TRANSPARENT);
        for (i, (b, l)) in bs.iter().zip(labels(full, top)).enumerate() {
            if hover == i as i32 { SelectObject(mem, lit.into()); let _ = RoundRect(mem, b.left, b.top, b.right, b.bottom, 16, 16); SelectObject(mem, back.into()); }
            SetTextColor(mem, if i == 2 { COLORREF(0x006B6BFF) } else { COLORREF(0x00F0F0F0) });
            let mut t: Vec<u16> = l.encode_utf16().collect();
            let mut r = *b;
            DrawTextW(mem, &mut t, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
        }
        SelectObject(mem, of);
        SelectObject(mem, op);
        SelectObject(mem, ob);
        let _ = DeleteObject(font.into());
        let _ = DeleteObject(back.into());
        let _ = DeleteObject(lit.into());
    }
    let _ = BitBlt(dc, 0, 0, cw, ch, Some(mem), 0, 0, SRCCOPY);
    SelectObject(mem, old);
    let _ = DeleteObject(bmp.into());
    let _ = DeleteDC(mem);
    let _ = EndPaint(h, &ps);
}

unsafe fn toggle_full(h: HWND) {
    let saved = with(|w| w.full.take()).flatten();
    match saved {
        Some((style, r)) => {
            SetWindowLongPtrW(h, GWL_STYLE, style);
            let top = if with(|w| w.on_top).unwrap_or(false) { HWND_TOPMOST } else { HWND_NOTOPMOST };
            let _ = SetWindowPos(h, Some(top), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_FRAMECHANGED | SWP_SHOWWINDOW);
        }
        None => {
            let mut r = RECT::default();
            let _ = GetWindowRect(h, &mut r);
            let style = GetWindowLongPtrW(h, GWL_STYLE);
            with(|w| w.full = Some((style, r)));
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST), &mut mi);
            let m = mi.rcMonitor;
            SetWindowLongPtrW(h, GWL_STYLE, (WS_POPUP | WS_VISIBLE).0 as isize);
            let _ = SetWindowPos(h, Some(HWND_TOP), m.left, m.top, m.right - m.left, m.bottom - m.top, SWP_FRAMECHANGED | SWP_SHOWWINDOW);
        }
    }
    let _ = InvalidateRect(Some(h), None, false);
}
unsafe fn toggle_top(h: HWND) {
    let on = with(|w| { w.on_top = !w.on_top; w.on_top }).unwrap_or(false);
    let _ = SetWindowPos(h, Some(if on { HWND_TOPMOST } else { HWND_NOTOPMOST }), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
    super::remember_on_top(on);
    let _ = InvalidateRect(Some(h), None, false);
}
fn disconnect() { with(|w| w.stop.store(true, SeqCst)); super::kick(); }

/// new picture shape (iPad turned): keep the window's area, change its shape
unsafe fn reshape(h: HWND, w: i32, hh: i32) {
    if with(|x| x.full.is_some()).unwrap_or(true) || IsZoomed(h).as_bool() { return; }
    let mut r = RECT::default();
    let _ = GetWindowRect(h, &mut r);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let _ = GetMonitorInfoW(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST), &mut mi);
    let work = mi.rcWork;
    let (mw, mh) = ((work.right - work.left) as f64 * 0.9, (work.bottom - work.top) as f64 * 0.9);
    let area = ((r.right - r.left) * (r.bottom - r.top)) as f64;
    let ratio = w as f64 / hh as f64;
    let mut ch = (area / ratio).sqrt();
    let mut cw = ch * ratio;
    let s = (mw / cw).min(mh / ch).min(1.0);
    cw *= s; ch *= s;
    let mut want = RECT { left: 0, top: 0, right: cw as i32, bottom: ch as i32 };
    let _ = AdjustWindowRectEx(&mut want, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE(0));
    let (nw, nh) = (want.right - want.left, want.bottom - want.top);
    let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
    let x = (cx - nw / 2).clamp(work.left, (work.right - nw).max(work.left));
    let y = (cy - nh / 2).clamp(work.top, (work.bottom - nh).max(work.top));
    let _ = SetWindowPos(h, None, x, y, nw, nh, SWP_NOZORDER | SWP_NOACTIVATE);
}

fn hit(h: HWND, lp: LPARAM) -> i32 {
    let (x, y) = ((lp.0 & 0xFFFF) as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as i16 as i32);
    let mut rc = RECT::default();
    unsafe { let _ = GetClientRect(h, &mut rc); }
    if !with(|w| Instant::now() < w.bar_until).unwrap_or(false) { return -1; }
    buttons(rc.right, rc.bottom).iter().position(|b| x >= b.left && x < b.right && y >= b.top && y < b.bottom).map_or(-1, |i| i as i32)
}

unsafe extern "system" fn proc_(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_FRAME => { let _ = InvalidateRect(Some(h), None, false); LRESULT(0) }
        WM_SHAPE => { reshape(h, wp.0 as i32, lp.0 as i32); LRESULT(0) }
        WM_PAINT => { paint(h); LRESULT(0) }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let was = with(|w| Instant::now() < w.bar_until).unwrap_or(false);
            with(|w| w.bar_until = Instant::now() + Duration::from_millis(2500));
            let hv = hit(h, lp);
            let changed = with(|w| { let c = w.hover != hv; w.hover = hv; c }).unwrap_or(false);
            if !was || changed { let _ = InvalidateRect(Some(h), None, false); }
            SetTimer(Some(h), BAR_TIMER, 2600, None);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == BAR_TIMER => {
            let _ = KillTimer(Some(h), BAR_TIMER);
            with(|w| w.hover = -1);
            let _ = InvalidateRect(Some(h), None, false);
            LRESULT(0)
        }
        WM_SETCURSOR => {
            // in fullscreen the mouse pointer hides together with the bar
            let hide = with(|w| w.full.is_some() && Instant::now() >= w.bar_until).unwrap_or(false);
            if hide && (lp.0 & 0xFFFF) as u32 == HTCLIENT { SetCursor(None); return LRESULT(1); }
            DefWindowProcW(h, msg, wp, lp)
        }
        WM_LBUTTONUP => {
            match hit(h, lp) { 0 => toggle_full(h), 1 => toggle_top(h), 2 => disconnect(), _ => {} }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => { if hit(h, lp) < 0 { toggle_full(h); } LRESULT(0) }
        WM_KEYDOWN => {
            if wp.0 as u16 == VK_F11.0 { toggle_full(h); }
            if wp.0 as u16 == VK_ESCAPE.0 && with(|w| w.full.is_some()).unwrap_or(false) { toggle_full(h); }
            LRESULT(0)
        }
        WM_CLOSE => {
            // closing the window by hand ends the mirroring too
            disconnect();
            let _ = DestroyWindow(h);
            LRESULT(0)
        }
        WM_QUIET_CLOSE => { let _ = DestroyWindow(h); LRESULT(0) }
        WM_DESTROY => { HWND_NOW.store(0, SeqCst); PostQuitMessage(0); LRESULT(0) }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}
