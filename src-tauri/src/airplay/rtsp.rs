// One AirPlay control connection (RTSP on port 7000): device info, pairing, FairPlay, SETUP of
// the streams, keep-alives and TEARDOWN. Protocol as in UxPlay / RPiPlay (GPL-3.0).
use super::{fairplay, mirror, pin, sound, Me};
use aes::Aes128;
use ctr::cipher::{KeyIvInit, StreamCipher};
use ed25519_dalek::{Signer, Verifier};
use plist::{Dictionary, Value};
use sha2::{Digest, Sha512};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type Ctr = ctr::Ctr128BE<Aes128>;

struct Req { method: String, path: String, proto: String, headers: Vec<(String, String)>, body: Vec<u8> }
impl Req {
    fn header(&self, k: &str) -> &str { self.headers.iter().find(|h| h.0 == k).map_or("", |h| h.1.as_str()) }
}
struct Res { code: u16, headers: Vec<(String, String)>, body: Vec<u8>, close: bool }
impl Res {
    fn ok() -> Self { Res { code: 200, headers: vec![], body: vec![], close: false } }
    fn err(code: u16) -> Self { Res { code, headers: vec![], body: vec![], close: false } }
    fn bin(body: Vec<u8>) -> Self { Res { code: 200, headers: vec![("Content-Type".into(), "application/octet-stream".into())], body, close: false } }
    fn plist(v: Value) -> Self {
        let mut body = vec![];
        let _ = plist::to_writer_binary(&mut body, &v);
        Res { code: 200, headers: vec![("Content-Type".into(), "application/x-apple-binary-plist".into())], body, close: false }
    }
}

/// everything one iPad has set up on this connection; dropped = all of it stops
#[derive(Default)]
struct Conn {
    peer: Option<IpAddr>,
    ecdh: Option<[u8; 32]>,
    verify: Option<([u8; 32], [u8; 32], [u8; 32])>, // ours, theirs (X25519), theirs (Ed25519)
    keymsg: Option<[u8; 164]>,
    srp: Option<pin::Srp>,
    srp_key: Option<[u8; 40]>,
    verified: bool,
    aeskey: Option<[u8; 16]>,
    aesiv: Option<[u8; 16]>,
    sock: Option<TcpStream>,
    timing: Option<Arc<AtomicBool>>,
    audio: Option<Arc<AtomicBool>>,
    mirror: Option<mirror::Handle>,
    device: String,
}
impl Drop for Conn {
    fn drop(&mut self) {
        for s in [&self.timing, &self.audio].into_iter().flatten() { s.store(true, SeqCst); }
        if self.mirror.take().is_some() { super::set_mirroring(None); }
    }
}

fn read_request(r: &mut BufReader<TcpStream>) -> std::io::Result<Option<Req>> {
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 { return Ok(None); }
        if !line.trim().is_empty() { break; }
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let proto = parts.next().unwrap_or("RTSP/1.0").to_string();
    let mut headers = vec![];
    loop {
        let mut h = String::new();
        if r.read_line(&mut h)? == 0 || h.trim().is_empty() { break; }
        if let Some((k, v)) = h.split_once(':') { headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string())); }
        if headers.len() > 64 { break; }
    }
    let len: usize = headers.iter().find(|h| h.0 == "content-length").and_then(|h| h.1.parse().ok()).unwrap_or(0);
    if len > 1 << 20 { return Ok(None); }
    let mut body = vec![0; len];
    r.read_exact(&mut body)?;
    Ok(Some(Req { method, path, proto, headers, body }))
}

pub fn serve(c: TcpStream) -> std::io::Result<()> {
    c.set_nodelay(true)?;
    let mut w = c.try_clone()?;
    let mut conn = Conn::default();
    conn.peer = c.peer_addr().ok().map(|a| a.ip());
    conn.sock = c.try_clone().ok();
    let mut r = BufReader::new(c);
    while let Some(req) = read_request(&mut r)? {
        if !super::live() { break; } // switched off meanwhile
        let res = handle(&mut conn, &req);
        let reason = match res.code { 200 => "OK", 400 => "Bad Request", 401 => "Unauthorized", 404 => "Not Found", 470 => "Connection Authorization Required", _ => "Error" };
        let mut head = format!("{} {} {reason}\r\nServer: AirTunes/{}\r\n", req.proto, res.code, super::SRCVERS);
        let seq = req.header("cseq");
        if !seq.is_empty() { head += &format!("CSeq: {seq}\r\n"); }
        for (k, v) in &res.headers { head += &format!("{k}: {v}\r\n"); }
        head += &format!("Content-Length: {}\r\n\r\n", res.body.len());
        let mut out = head.into_bytes();
        out.extend_from_slice(&res.body);
        w.write_all(&out)?;
        if res.close { break; }
    }
    Ok(())
}

fn sha512(parts: &[&[u8]]) -> [u8; 64] {
    let mut h = Sha512::new();
    for p in parts { h.update(p); }
    h.finalize().into()
}
fn pair_cipher(shared: &[u8; 32]) -> Ctr {
    let key = sha512(&[b"Pair-Verify-AES-Key", shared]);
    let iv = sha512(&[b"Pair-Verify-AES-IV", shared]);
    Ctr::new(key[..16].into(), iv[..16].into())
}
fn dict_of(body: &[u8]) -> Option<Dictionary> {
    Value::from_reader(std::io::Cursor::new(body)).ok()?.into_dictionary()
}
fn uint(d: &Dictionary, k: &str) -> Option<u64> { d.get(k)?.as_unsigned_integer().or_else(|| d.get(k)?.as_signed_integer().map(|v| v as u64)) }

fn handle(c: &mut Conn, req: &Req) -> Res {
    let me = super::me();
    let path = req.path.split('?').next().unwrap_or("");
    match (req.method.as_str(), path) {
        ("GET", "/info") => info(&me, req),
        ("POST", "/pair-setup") => {
            if req.body.len() != 32 { return Res::err(400); }
            Res::bin(me.key.verifying_key().to_bytes().to_vec())
        }
        ("POST", "/pair-verify") => pair_verify(c, &me, &req.body),
        ("POST", "/pair-pin-start") => {
            if !super::pin_mode() { return Res::err(404); }
            super::show_pin(&pin::new_pin());
            Res::ok()
        }
        ("POST", "/pair-setup-pin") => pair_setup_pin(c, &me, &req.body),
        ("POST", "/fp-setup") => {
            if req.body.len() == 16 { return fairplay::setup(&req.body).map_or(Res::err(400), Res::bin); }
            match fairplay::handshake(&req.body) { Some((res, msg)) => { c.keymsg = Some(msg); Res::bin(res) } None => Res::err(400) }
        }
        ("OPTIONS", _) => {
            let mut r = Res::ok();
            r.headers.push(("Public".into(), "SETUP, RECORD, PAUSE, FLUSH, TEARDOWN, OPTIONS, GET_PARAMETER, SET_PARAMETER, POST, GET".into()));
            r
        }
        ("SETUP", _) => setup(c, req),
        ("RECORD", _) => {
            let mut r = Res::ok();
            r.headers.push(("Audio-Latency".into(), "11025".into()));
            r.headers.push(("Audio-Jack-Status".into(), "connected; type=analog".into()));
            r
        }
        ("GET_PARAMETER", _) => {
            if String::from_utf8_lossy(&req.body).contains("volume") {
                let mut r = Res::ok();
                r.headers.push(("Content-Type".into(), "text/parameters".into()));
                r.body = b"volume: 0.000000\r\n".to_vec();
                return r;
            }
            Res::ok()
        }
        ("TEARDOWN", _) => {
            // with a list of streams only those end, without one the whole session ends
            let types: Vec<u64> = dict_of(&req.body).and_then(|d| d.get("streams")?.as_array().cloned())
                .map(|a| a.iter().filter_map(|s| uint(s.as_dictionary()?, "type")).collect()).unwrap_or_default();
            if types.is_empty() || types.contains(&110) {
                if c.mirror.take().is_some() { super::set_mirroring(None); }
            }
            if types.is_empty() || types.contains(&96) { if let Some(a) = c.audio.take() { a.store(true, SeqCst); } }
            if types.is_empty() { if let Some(t) = c.timing.take() { t.store(true, SeqCst); } }
            Res::ok()
        }
        // keep-alive every 2 s, volume, flush, audio mode: nothing to do
        ("SET_PARAMETER", _) => {
            let t = String::from_utf8_lossy(&req.body).to_string();
            if let Some(v) = t.strip_prefix("volume: ").and_then(|v| v.trim().parse::<f32>().ok()) { sound::set_volume(v); }
            Res::ok()
        }
        ("POST", "/feedback") | ("FLUSH", _) | ("POST", "/audioMode") | ("PAUSE", _) => Res::ok(),
        _ => Res::err(404),
    }
}

/// the TXT record of one service in DNS wire format
fn txt_wire(items: &[(String, String)]) -> Vec<u8> {
    let mut v = vec![];
    for (k, x) in items {
        let s = format!("{k}={x}");
        let b = &s.as_bytes()[..s.len().min(255)];
        v.push(b.len() as u8);
        v.extend_from_slice(b);
    }
    v
}

fn info(me: &Me, req: &Req) -> Res {
    let mut d = Dictionary::new();
    if req.header("content-type").contains("binary-plist") {
        // the first /info only asks for one TXT record
        let q = dict_of(&req.body).and_then(|d| d.get("qualifier")?.as_array()?.first()?.as_string().map(str::to_string)).unwrap_or_default();
        if q == "txtAirPlay" { d.insert("txtAirPlay".into(), Value::Data(txt_wire(&me.txt_airplay))); }
        if q == "txtRAOP" { d.insert("txtRAOP".into(), Value::Data(txt_wire(&me.txt_raop))); }
        return Res::plist(Value::Dictionary(d));
    }
    let int = |v: u64| Value::Integer(v.into());
    d.insert("deviceID".into(), Value::String(me.id.clone()));
    d.insert("macAddress".into(), Value::String(me.id.clone()));
    d.insert("pk".into(), Value::Data(me.key.verifying_key().to_bytes().to_vec()));
    d.insert("features".into(), int(super::FEATURES_NUM));
    d.insert("name".into(), Value::String(me.name.clone()));
    d.insert("pi".into(), Value::String(me.pi.clone()));
    d.insert("vv".into(), int(2));
    d.insert("statusFlags".into(), int(68));
    d.insert("keepAliveLowPower".into(), Value::Boolean(true));
    d.insert("sourceVersion".into(), Value::String(super::SRCVERS.into()));
    d.insert("keepAliveSendStatsAsBody".into(), Value::Boolean(true));
    d.insert("model".into(), Value::String(super::MODEL.into()));
    d.insert("initialVolume".into(), Value::Real(0.0));
    let audio = |t: u64| { let mut a = Dictionary::new(); a.insert("type".into(), int(t)); a.insert("audioInputFormats".into(), int(0x3fffffc)); a.insert("audioOutputFormats".into(), int(0x3fffffc)); Value::Dictionary(a) };
    d.insert("audioFormats".into(), Value::Array(vec![audio(100), audio(101)]));
    let lat = |t: u64| {
        let mut a = Dictionary::new();
        a.insert("type".into(), int(t)); a.insert("audioType".into(), Value::String("default".into()));
        a.insert("inputLatencyMicros".into(), int(0)); a.insert("outputLatencyMicros".into(), Value::Boolean(false));
        Value::Dictionary(a)
    };
    d.insert("audioLatencies".into(), Value::Array(vec![lat(100), lat(101)]));
    let (w, h) = super::DISPLAY;
    let mut disp = Dictionary::new();
    disp.insert("uuid".into(), Value::String("e0ff8a27-6738-3d56-8a16-cc53aacee925".into()));
    disp.insert("widthPhysical".into(), int(0)); disp.insert("heightPhysical".into(), int(0));
    disp.insert("width".into(), int(w)); disp.insert("height".into(), int(h));
    disp.insert("widthPixels".into(), int(w)); disp.insert("heightPixels".into(), int(h));
    disp.insert("rotation".into(), Value::Boolean(false));
    disp.insert("refreshRate".into(), Value::Real(1.0 / 60.0));
    disp.insert("maxFPS".into(), int(60));
    disp.insert("overscanned".into(), Value::Boolean(false));
    disp.insert("features".into(), int(14));
    d.insert("displays".into(), Value::Array(vec![Value::Dictionary(disp)]));
    Res::plist(Value::Dictionary(d))
}

fn pair_verify(c: &mut Conn, me: &Me, body: &[u8]) -> Res {
    match body.first() {
        Some(1) if body.len() == 68 => {
            let mut seed = [0u8; 32];
            if getrandom::getrandom(&mut seed).is_err() { return Res::err(500); }
            let secret = x25519_dalek::StaticSecret::from(seed);
            let ours = x25519_dalek::PublicKey::from(&secret).to_bytes();
            let mut theirs = [0u8; 32];
            theirs.copy_from_slice(&body[4..36]);
            let mut ed = [0u8; 32];
            ed.copy_from_slice(&body[36..68]);
            let shared = secret.diffie_hellman(&x25519_dalek::PublicKey::from(theirs)).to_bytes();
            // with a PIN only devices that entered it once may go on
            if super::pin_mode() && !super::is_known(&ed) { let mut r = Res::err(470); r.close = true; return r; }
            let mut sig = me.key.sign(&[ours, theirs].concat()).to_bytes();
            pair_cipher(&shared).apply_keystream(&mut sig);
            c.ecdh = Some(shared);
            c.verify = Some((ours, theirs, ed));
            let mut out = ours.to_vec();
            out.extend_from_slice(&sig);
            Res::bin(out)
        }
        Some(0) if body.len() == 68 => {
            let (Some(shared), Some((ours, theirs, ed))) = (c.ecdh, c.verify) else { return Res::err(470) };
            let mut sig = [0u8; 64];
            sig.copy_from_slice(&body[4..68]);
            let mut cipher = pair_cipher(&shared);
            let mut skip = [0u8; 64];
            cipher.apply_keystream(&mut skip); // the first 64 bytes of the key stream were used for our signature
            cipher.apply_keystream(&mut sig);
            let ok = ed25519_dalek::VerifyingKey::from_bytes(&ed)
                .map(|k| k.verify(&[theirs, ours].concat(), &ed25519_dalek::Signature::from_bytes(&sig)).is_ok()).unwrap_or(false);
            if !ok { c.ecdh = None; let mut r = Res::err(470); r.close = true; return r; }
            c.verified = true;
            Res::bin(vec![])
        }
        _ => Res::err(400),
    }
}

fn setup(c: &mut Conn, req: &Req) -> Res {
    let Some(d) = dict_of(&req.body) else { return Res::err(400) };
    let mut res = Dictionary::new();
    if let (Some(ekey), Some(eiv)) = (d.get("ekey").and_then(|v| v.as_data()), d.get("eiv").and_then(|v| v.as_data())) {
        if eiv.len() == 16 { let mut iv = [0u8; 16]; iv.copy_from_slice(eiv); c.aesiv = Some(iv); }
        if super::pin_mode() && !c.verified { return Res::err(470); }
        let Some(msg) = c.keymsg else { return Res::err(400) };
        let Some(mut key) = fairplay::decrypt(&msg, ekey) else { return Res::err(400) };
        // after pairing, the key is mixed with the shared secret
        if let Some(shared) = c.ecdh { let h = sha512(&[&key, &shared]); key.copy_from_slice(&h[..16]); }
        c.aeskey = Some(key);
        c.device = d.get("name").and_then(|v| v.as_string()).unwrap_or("iPad").chars().take(40).collect();
        super::set_device(&c.device);
        let rport = uint(&d, "timingPort").unwrap_or(0) as u16;
        let local = match (c.peer, rport) { (Some(ip), p) if p > 0 => start_timing(ip, p).map(|(port, stop)| { c.timing = Some(stop); port }).unwrap_or(0), _ => 0 };
        res.insert("timingPort".into(), Value::Integer((local as u64).into()));
        res.insert("eventPort".into(), Value::Integer(0u64.into()));
    }
    if let Some(streams) = d.get("streams").and_then(|v| v.as_array()) {
        let mut out = vec![];
        for s in streams {
            let Some(s) = s.as_dictionary() else { continue };
            let mut r = Dictionary::new();
            match uint(s, "type") {
                Some(110) => {
                    let (Some(key), Some(id)) = (c.aeskey, uint(s, "streamConnectionID")) else { return Res::err(400) };
                    let Ok(h) = mirror::start(key, id, c.device.clone()) else { return Res::err(500) };
                    if let Some(sock) = &c.sock { super::set_session(sock); }
                    r.insert("type".into(), Value::Integer(110u64.into()));
                    r.insert("dataPort".into(), Value::Integer((h.port as u64).into()));
                    c.mirror = Some(h);
                }
                Some(96) => {
                    let (Some(key), Some(iv)) = (c.aeskey, c.aesiv) else { return Res::err(400) };
                    if let Some(old) = c.audio.take() { old.store(true, SeqCst); }
                    let Ok((dport, cport, stop)) = sound::start(uint(s, "ct").unwrap_or(8), uint(s, "spf").unwrap_or(0), key, iv) else { return Res::err(500) };
                    r.insert("type".into(), Value::Integer(96u64.into()));
                    r.insert("dataPort".into(), Value::Integer((dport as u64).into()));
                    r.insert("controlPort".into(), Value::Integer((cport as u64).into()));
                    c.audio = Some(stop);
                }
                _ => continue,
            }
            out.push(Value::Dictionary(r));
        }
        res.insert("streams".into(), Value::Array(out));
    }
    Res::plist(Value::Dictionary(res))
}

/// NTP-like timing: we ask the iPad for its clock every 3 s, it answers on the same socket
fn start_timing(ip: IpAddr, port: u16) -> std::io::Result<(u16, Arc<AtomicBool>)> {
    let sock = UdpSocket::bind(("0.0.0.0", 0))?;
    sock.set_read_timeout(Some(Duration::from_millis(500)))?;
    let local = sock.local_addr()?.port();
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 128];
        let mut tick = 0u32;
        while !s2.load(SeqCst) {
            if tick % 6 == 0 {
                let mut req = [0u8; 32];
                req[..4].copy_from_slice(&[0x80, 0xd2, 0x00, 0x07]);
                let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
                let secs = (t.as_secs() + 2_208_988_800) as u32;
                let frac = ((t.subsec_nanos() as u64) << 32) / 1_000_000_000;
                req[24..28].copy_from_slice(&secs.to_be_bytes());
                req[28..32].copy_from_slice(&(frac as u32).to_be_bytes());
                let _ = sock.send_to(&req, (ip, port));
            }
            let _ = sock.recv_from(&mut buf);
            tick += 1;
        }
    });
    Ok((local, stop))
}

/// PIN pairing (SRP): 1 the iPad names itself, 2 it proves the PIN, 3 both swap their long-term keys
fn pair_setup_pin(c: &mut Conn, me: &Me, body: &[u8]) -> Res {
    let fail = || { let mut r = Res::err(470); r.close = true; r };
    let Some(d) = dict_of(body) else { return fail() };
    let data = |k: &str| d.get(k).and_then(|v| v.as_data()).map(|v| v.to_vec());
    let mut out = Dictionary::new();
    if let (Some(method), Some(user)) = (d.get("method").and_then(|v| v.as_string()), d.get("user").and_then(|v| v.as_string())) {
        let code = super::current_pin();
        if method != "pin" || code.is_empty() { return fail(); }
        let (srp, salt, pk) = pin::Srp::new(user, &code);
        c.srp = Some(srp);
        out.insert("pk".into(), Value::Data(pk));
        out.insert("salt".into(), Value::Data(salt));
    } else if let (Some(pk), Some(proof)) = (data("pk"), data("proof")) {
        let Some(srp) = c.srp.take() else { return fail() };
        let Some((m2, key)) = srp.verify(&pk, &proof) else { return fail() };
        c.srp_key = Some(key);
        out.insert("proof".into(), Value::Data(m2.to_vec()));
    } else if let (Some(epk), Some(tag)) = (data("epk"), data("authTag")) {
        // the iPad is remembered, so the PIN is only needed once
        let Some(key) = c.srp_key.take() else { return fail() };
        let ours = me.key.verifying_key().to_bytes();
        let Some((client, epk2, tag2)) = pin::confirm(&key, &epk, &tag, &ours) else { return fail() };
        super::remember(&client);
        super::show_pin("");
        out.insert("epk".into(), Value::Data(epk2));
        out.insert("authTag".into(), Value::Data(tag2));
    } else {
        return fail();
    }
    Res::plist(Value::Dictionary(out))
}
