// Ping: an ICMP echo like ping.exe (no extra program). Hosts that don't answer pings
// are measured by how long a connection to them takes, which is about the same.
use crate::blocking;
use serde::Serialize;
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};
use windows::Win32::NetworkManagement::IpHelper::{IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY};

#[derive(Serialize)]
pub struct Ping { host: String, ms: Option<u32>, ip: String }

fn valid(h: &str) -> bool {
    !h.is_empty() && h.len() <= 253 && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn icmp(ip: std::net::Ipv4Addr) -> Option<u32> {
    unsafe {
        let h = IcmpCreateFile().ok()?;
        let data = [0x61u8; 32];
        let mut reply = vec![0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + data.len() + 8];
        let n = IcmpSendEcho(h, u32::from_ne_bytes(ip.octets()), data.as_ptr() as _, data.len() as u16, None, reply.as_mut_ptr() as _, reply.len() as u32, 1000);
        let r = std::ptr::read_unaligned(reply.as_ptr() as *const ICMP_ECHO_REPLY);
        let _ = IcmpCloseHandle(h);
        (n > 0 && r.Status == 0).then_some(r.RoundTripTime.max(1))
    }
}

fn tcp(addr: IpAddr) -> Option<u32> {
    let t = Instant::now();
    TcpStream::connect_timeout(&SocketAddr::new(addr, 443), Duration::from_millis(1000)).ok()?;
    Some((t.elapsed().as_millis() as u32).max(1))
}

fn ping(host: String) -> Ping {
    if !valid(&host) { return Ping { host, ms: None, ip: String::new() }; }
    let addr = (host.as_str(), 0).to_socket_addrs().ok().and_then(|mut a| a.find(|x| x.is_ipv4())).map(|a| a.ip());
    let Some(addr) = addr else { return Ping { host, ms: None, ip: String::new() } };
    let ms = match addr { IpAddr::V4(v4) => icmp(v4), _ => None }.or_else(|| tcp(addr));
    Ping { host, ms, ip: addr.to_string() }
}

/// all hosts at the same time, so one slow host doesn't hold up the others
#[tauri::command]
pub async fn ping_hosts(hosts: Vec<String>) -> Vec<Ping> {
    blocking(move || {
        let jobs: Vec<_> = hosts.into_iter().take(12).map(|h| std::thread::spawn(move || ping(h))).collect();
        jobs.into_iter().filter_map(|j| j.join().ok()).collect()
    })
    .await
}
