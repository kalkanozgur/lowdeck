//! PC keşfi (ADR-0007): UDP yayın sorgusu, agent tek datagram ile yanıtlar. Yalnız kullanıcı
//! "PC'yi bul" deyince, `WAIT` boyunca (kısa, tek seferlik); sürekli dinleme yok.

use lowdeck_proto::{decode_beacon, Beacon, DISCOVERY_PORT, DISCOVER_REQ};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

pub const WAIT: Duration = Duration::from_millis(1500);
/// Wi-Fi'da yayın paketi kaybolabilir: sorgu bu aralıkla bir kez tekrarlanır.
const RESEND: Duration = Duration::from_millis(600);

/// Bloklar (`wait` kadar): çağıran ayrı thread'de çalıştırır. Aynı host:port bir kez.
pub fn discover(wait: Duration) -> Vec<Beacon> {
    let Ok(s) = UdpSocket::bind(("0.0.0.0", 0)) else {
        return Vec::new();
    };
    let _ = s.set_broadcast(true);
    let _ = s.set_read_timeout(Some(Duration::from_millis(200)));
    let dst = SocketAddr::from(([255, 255, 255, 255], DISCOVERY_PORT));
    let (start, mut sent, mut buf) = (Instant::now(), 0u32, [0u8; 512]);
    let mut out: Vec<Beacon> = Vec::new();
    while start.elapsed() < wait {
        if sent < 2 && start.elapsed() >= RESEND * sent {
            let _ = s.send_to(&DISCOVER_REQ, dst);
            sent += 1;
        }
        let Ok((n, _)) = s.recv_from(&mut buf) else {
            continue;
        };
        if let Some(b) = decode_beacon(&buf[..n]) {
            if !out.iter().any(|o| o.host == b.host && o.port == b.port) {
                out.push(b);
            }
        }
    }
    out
}
