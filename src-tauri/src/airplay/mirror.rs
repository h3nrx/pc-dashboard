// The mirrored picture: one TCP stream of 128-byte headers + payloads. Type 1 = SPS/PPS (plain),
// type 0 = H.264 frame (AES-CTR, one key stream for the whole connection).
use aes::Aes128;
use ctr::cipher::{KeyIvInit, StreamCipher};
use sha2::{Digest, Sha512};
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

type Ctr = ctr::Ctr128BE<Aes128>;
const START: [u8; 4] = [0, 0, 0, 1];

pub struct Handle { pub port: u16, stop: Arc<AtomicBool> }
impl Drop for Handle { fn drop(&mut self) { self.stop.store(true, SeqCst); } }

/// where decoded-ready H.264 (Annex B) goes
pub trait Sink: Send {
    fn format(&mut self, w: u32, h: u32);
    fn frame(&mut self, annexb: &[u8]);
}

pub fn start(aeskey: [u8; 16], id: u64, device: String) -> std::io::Result<Handle> {
    let l = TcpListener::bind(("0.0.0.0", 0))?;
    l.set_nonblocking(true)?;
    let port = l.local_addr()?.port();
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    thread::spawn(move || {
        let t = Instant::now();
        while !s2.load(SeqCst) && t.elapsed() < Duration::from_secs(20) {
            match l.accept() {
                Ok((c, _)) => {
                    let mut sink = super::sink(device, s2.clone());
                    let _ = run(c, &aeskey, id, &s2, sink.as_mut());
                    return;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(50)),
                Err(_) => return,
            }
        }
    });
    Ok(Handle { port, stop })
}

/// reads exactly n bytes, but gives up when the session is stopped
fn read_full(c: &mut TcpStream, buf: &mut [u8], stop: &AtomicBool) -> std::io::Result<()> {
    let mut at = 0;
    while at < buf.len() {
        if stop.load(SeqCst) { return Err(ErrorKind::Interrupted.into()); }
        match c.read(&mut buf[at..]) {
            Ok(0) => return Err(ErrorKind::UnexpectedEof.into()),
            Ok(n) => at += n,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn run(mut c: TcpStream, aeskey: &[u8; 16], id: u64, stop: &AtomicBool, sink: &mut dyn Sink) -> std::io::Result<()> {
    c.set_nonblocking(false)?;
    c.set_read_timeout(Some(Duration::from_millis(500)))?;
    let mut h = Sha512::new();
    h.update(format!("AirPlayStreamKey{id}").as_bytes());
    h.update(aeskey);
    let key = h.finalize();
    let mut h = Sha512::new();
    h.update(format!("AirPlayStreamIV{id}").as_bytes());
    h.update(aeskey);
    let iv = h.finalize();
    let mut cipher = Ctr::new(key[..16].into(), iv[..16].into());
    let mut head = [0u8; 128];
    let mut pending: Option<Vec<u8>> = None; // SPS + PPS, put in front of the next frame
    loop {
        read_full(&mut c, &mut head, stop)?;
        let size = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) as usize;
        if size > 16 << 20 { return Err(ErrorKind::InvalidData.into()); }
        let mut p = vec![0u8; size];
        read_full(&mut c, &mut p, stop)?;
        match head[4] {
            0 => {
                cipher.apply_keystream(&mut p);
                // 4-byte lengths -> start codes
                let mut at = 0;
                let mut ok = true;
                while at + 4 <= p.len() {
                    let n = u32::from_be_bytes([p[at], p[at + 1], p[at + 2], p[at + 3]]) as usize;
                    if n > p.len() - at - 4 { ok = false; break; }
                    p[at..at + 4].copy_from_slice(&START);
                    at += 4 + n;
                }
                if !ok || at != p.len() { continue; }
                match pending.take() {
                    Some(mut sp) => { sp.extend_from_slice(&p); sink.frame(&sp); }
                    None => sink.frame(&p),
                }
            }
            1 if p.len() > 11 => {
                let sps_n = u16::from_be_bytes([p[6], p[7]]) as usize;
                if 8 + sps_n + 3 > p.len() { continue; }
                let pps_n = u16::from_be_bytes([p[9 + sps_n], p[10 + sps_n]]) as usize;
                if 11 + sps_n + pps_n > p.len() { continue; }
                let mut sp = START.to_vec();
                sp.extend_from_slice(&p[8..8 + sps_n]);
                sp.extend_from_slice(&START);
                sp.extend_from_slice(&p[11 + sps_n..11 + sps_n + pps_n]);
                pending = Some(sp);
                let f = |i: usize| f32::from_le_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]]) as u32;
                sink.format(f(56), f(60));
            }
            _ => {} // reports and heartbeats
        }
    }
}

/// writes the raw stream to a file (for testing) and reports progress
pub struct Probe { pub device: String, pub file: Option<std::fs::File>, pub frames: u64, pub size: (u32, u32) }
impl Sink for Probe {
    fn format(&mut self, w: u32, h: u32) { self.size = (w, h); super::set_mirroring(Some((self.device.clone(), w, h))); }
    fn frame(&mut self, b: &[u8]) {
        self.frames += 1;
        if let Some(f) = self.file.as_mut() { let _ = f.write_all(b); }
    }
}
