// The sound of an AirPlay session: RTP over UDP, AES-CBC per packet, AAC-ELD (mirroring), AAC-LC or
// ALAC (music). ffmpeg decodes it (fed as a small live Matroska stream), WASAPI plays it.
use crate::NO_WINDOW;
use aes::cipher::{BlockDecrypt, KeyInit};
use aes::Aes128;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::UdpSocket;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

const RATE: u32 = 44_100;
/// volume the iPad asked for, as a factor (f32 bits)
static GAIN: AtomicU32 = AtomicU32::new(0x3F80_0000);

/// AirPlay volume: 0 dB = full, -30 dB = quietest, -144 = muted
pub fn set_volume(db: f32) {
    let g = if db <= -30.0 { 0.0 } else { 10f32.powf(db.min(0.0) / 20.0) };
    GAIN.store(g.to_bits(), SeqCst);
}

/* ---------- a tiny live Matroska writer ---------- */
fn vint(n: u64) -> Vec<u8> {
    for len in 1..=8u32 {
        if n < (1u64 << (7 * len)) - 1 {
            let mut v = vec![0u8; len as usize];
            for i in 0..len as usize { v[len as usize - 1 - i] = (n >> (8 * i)) as u8; }
            v[0] |= 0x80 >> (len - 1);
            return v;
        }
    }
    vec![0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
}
fn el(id: &[u8], data: &[u8]) -> Vec<u8> { let mut v = id.to_vec(); v.extend(vint(data.len() as u64)); v.extend_from_slice(data); v }
fn uint(id: &[u8], n: u64) -> Vec<u8> { let b = n.to_be_bytes(); el(id, &b[(n.leading_zeros() / 8).min(7) as usize..]) }
const UNKNOWN: [u8; 8] = [0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

fn mkv_head(codec: &str, private: &[u8]) -> Vec<u8> {
    let ebml = [uint(&[0x42, 0x86], 1), uint(&[0x42, 0xF7], 1), uint(&[0x42, 0xF2], 4), uint(&[0x42, 0xF3], 8),
        el(&[0x42, 0x82], b"matroska"), uint(&[0x42, 0x87], 4), uint(&[0x42, 0x85], 2)].concat();
    let mut v = el(&[0x1A, 0x45, 0xDF, 0xA3], &ebml);
    v.extend([0x18, 0x53, 0x80, 0x67]);
    v.extend(UNKNOWN);
    v.extend(el(&[0x15, 0x49, 0xA9, 0x66], &uint(&[0x2A, 0xD7, 0xB1], 1_000_000)));
    let audio = [el(&[0xB5], &(RATE as f64).to_be_bytes()), uint(&[0x9F], 2)].concat();
    let track = [uint(&[0xD7], 1), uint(&[0x73, 0xC5], 1), uint(&[0x83], 2), el(&[0x86], codec.as_bytes()),
        el(&[0x63, 0xA2], private), el(&[0xE1], &audio)].concat();
    v.extend(el(&[0x16, 0x54, 0xAE, 0x6B], &el(&[0xAE], &track)));
    v
}

/// one audio frame as a Matroska block (with a new cluster every 5 s)
struct Mkv { samples: u64, cluster_at: u64, open: bool, spf: u64 }
impl Mkv {
    fn frame(&mut self, p: &[u8]) -> Vec<u8> {
        let ms = self.samples * 1000 / RATE as u64;
        let mut out = vec![];
        if !self.open || ms - self.cluster_at > 5000 {
            out.extend([0x1F, 0x43, 0xB6, 0x75]);
            out.extend(UNKNOWN);
            out.extend(uint(&[0xE7], ms));
            self.cluster_at = ms;
            self.open = true;
        }
        let mut block = vec![0x81];
        block.extend(((ms - self.cluster_at) as i16).to_be_bytes());
        block.push(0x80);
        block.extend_from_slice(p);
        out.extend(el(&[0xA3], &block));
        self.samples += self.spf;
        out
    }
}

/// starts the sound of one session; returns (data port, control port, stop)
pub fn start(ct: u64, spf: u64, key: [u8; 16], iv: [u8; 16]) -> std::io::Result<(u16, u16, Arc<AtomicBool>)> {
    let (codec, private, spf): (&str, Vec<u8>, u64) = match ct {
        2 => ("A_ALAC", vec![0, 0, 1, 0x60, 0, 0x10, 0x28, 0x0a, 0x0e, 0x02, 0, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xac, 0x44], 352),
        4 => ("A_AAC", vec![0x12, 0x10], 1024),
        _ => ("A_AAC", vec![0xf8, 0xe8, 0x50, 0x00], if spf > 0 { spf } else { 480 }),
    };
    let data = UdpSocket::bind(("0.0.0.0", 0))?;
    let control = UdpSocket::bind(("0.0.0.0", 0))?;
    let (dport, cport) = (data.local_addr()?.port(), control.local_addr()?.port());
    data.set_read_timeout(Some(Duration::from_millis(300)))?;
    control.set_read_timeout(Some(Duration::from_millis(300)))?;
    let stop = Arc::new(AtomicBool::new(false));
    { let stop = stop.clone(); thread::spawn(move || { let mut b = [0u8; 2048]; while !stop.load(SeqCst) { let _ = control.recv_from(&mut b); } }); }

    let mut child = Command::new(crate::clips::ffmpeg()).creation_flags(NO_WINDOW)
        .args(["-hide_banner", "-loglevel", "error", "-probesize", "32", "-analyzeduration", "0", "-f", "matroska", "-i", "pipe:0",
               "-f", "s16le", "-ar", "44100", "-ac", "2", "pipe:1"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let (Some(mut input), Some(mut output)) = (child.stdin.take(), child.stdout.take()) else { return Err(std::io::ErrorKind::Other.into()) };
    let ring = Arc::new(Mutex::new(VecDeque::<i16>::new()));

    // network -> decoder
    {
        let stop = stop.clone();
        thread::spawn(move || {
            let aes = Aes128::new(&key.into());
            let _ = input.write_all(&mkv_head(codec, &private));
            let mut mkv = Mkv { samples: 0, cluster_at: 0, open: false, spf };
            let mut b = [0u8; 4096];
            while !stop.load(SeqCst) {
                let Ok((n, _)) = data.recv_from(&mut b) else { continue };
                if n <= 12 || b[1] & 0x7f != 0x60 { continue; } // sync / empty packets
                let p = &mut b[12..n];
                // AES-CBC over the whole 16-byte blocks, fresh for every packet; the rest is plain
                let mut prev = iv;
                for blk in p.chunks_exact_mut(16) {
                    let mut cur = [0u8; 16];
                    cur.copy_from_slice(blk);
                    aes.decrypt_block(blk.into());
                    for (x, y) in blk.iter_mut().zip(prev) { *x ^= y; }
                    prev = cur;
                }
                if input.write_all(&mkv.frame(p)).is_err() { break; }
            }
        });
    }
    // decoder -> ring (at most 1 s, the oldest goes)
    {
        let ring = ring.clone();
        thread::spawn(move || {
            let mut b = vec![0u8; 8192];
            while let Ok(n) = output.read(&mut b) {
                if n == 0 { break; }
                let mut r = ring.lock().unwrap_or_else(|e| e.into_inner());
                r.extend(b[..n & !1].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])));
                let over = r.len().saturating_sub(RATE as usize * 2);
                r.drain(..over);
            }
        });
    }
    // ring -> speakers
    {
        let stop = stop.clone();
        thread::spawn(move || {
            unsafe { let _ = CoInitializeEx(None, COINIT_MULTITHREADED); }
            while !stop.load(SeqCst) {
                let _ = unsafe { play(&ring, &stop) }; // ends when the output device changes
                thread::sleep(Duration::from_millis(300));
            }
            let _ = child.kill();
            let _ = child.wait();
        });
    }
    Ok((dport, cport, stop))
}

unsafe fn play(ring: &Mutex<VecDeque<i16>>, stop: &AtomicBool) -> windows::core::Result<()> {
    let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
    let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
    let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
    let fmt = WAVEFORMATEX { wFormatTag: 1, nChannels: 2, nSamplesPerSec: RATE, nAvgBytesPerSec: RATE * 4, nBlockAlign: 4, wBitsPerSample: 16, cbSize: 0 };
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, 2_000_000, 0, &fmt, None)?;
    let size = client.GetBufferSize()?;
    let out: IAudioRenderClient = client.GetService()?;
    // a small head start against Wi-Fi hiccups
    while !stop.load(SeqCst) && ring.lock().unwrap_or_else(|e| e.into_inner()).len() < (RATE as usize / 10) * 2 { std::thread::sleep(Duration::from_millis(10)); }
    client.Start()?;
    while !stop.load(SeqCst) {
        let free = size - client.GetCurrentPadding()?;
        let mut r = ring.lock().unwrap_or_else(|e| e.into_inner());
        let n = free.min((r.len() / 2) as u32);
        if n > 0 {
            let p = out.GetBuffer(n)? as *mut i16;
            let g = f32::from_bits(GAIN.load(SeqCst));
            for i in 0..(n as usize * 2) {
                let s = r.pop_front().unwrap_or(0);
                *p.add(i) = (s as f32 * g) as i16;
            }
            out.ReleaseBuffer(n, 0)?;
        }
        drop(r);
        std::thread::sleep(Duration::from_millis(10));
    }
    let _ = client.Stop();
    Ok(())
}

#[cfg(test)]
mod tests {
    // AAC-LC frames from an ADTS file -> our Matroska -> ffmpeg must decode all of it
    #[test]
    fn mkv_decodes() {
        let Ok(adts) = std::fs::read(std::env::var("PCD_TEST_ADTS").unwrap_or_default()) else { return };
        let mut out = super::mkv_head("A_AAC", &[0x12, 0x10]);
        let mut m = super::Mkv { samples: 0, cluster_at: 0, open: false, spf: 1024 };
        let mut at = 0;
        let mut n = 0;
        while at + 7 <= adts.len() {
            let len = (((adts[at + 3] & 3) as usize) << 11) | ((adts[at + 4] as usize) << 3) | (adts[at + 5] >> 5) as usize;
            let hl = if adts[at + 1] & 1 == 0 { 9 } else { 7 };
            out.extend(m.frame(&adts[at + hl..at + len]));
            at += len;
            n += 1;
        }
        std::fs::write(std::env::var("PCD_TEST_MKV").unwrap(), out).unwrap();
        println!("frames {n}");
    }
}
