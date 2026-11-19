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
    let Ok(s) = UdpSocket::bind(("0.0.0.0", 0)) else { return Vec::new() };
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
        let Ok((n, _)) = s.recv_from(&mut buf) else { continue };
        if let Some(b) = decode_beacon(&buf[..n]) {
            if !out.iter().any(|o| o.host == b.host && o.port == b.port) {
                out.push(b);
            }
        }
    }
    out
}

/// Otomatik yeniden bağlanma: kayıtlı PC'nin yeni adresi. Aynı port şart (token o PC'nin);
/// kayıtlı ad eşleşirse o, yoksa tek aday varsa o. Birden çok belirsiz aday → `None` (yanlış PC'ye gitme).
pub fn pick<'a>(found: &'a [Beacon], port: u16, name: &str) -> Option<&'a Beacon> {
    let same_port: Vec<&Beacon> = found.iter().filter(|b| b.port == port).collect();
    if !name.is_empty() {
        if let Some(b) = same_port.iter().find(|b| b.name == name) {
            return Some(b);
        }
    }
    match same_port[..] {
        [b] => Some(b),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(host: &str, port: u16, name: &str) -> Beacon {
        Beacon { proto: 4, host: host.into(), port, name: name.into() }
    }

    #[test]
    fn pick_rules() {
        let one = [b("192.168.1.191", 47810, "XAN-PC")];
        assert_eq!(pick(&one, 47810, "").unwrap().host, "192.168.1.191");
        assert!(pick(&one, 47820, "").is_none()); // farklı port: başka kurulum
        let two = [b("192.168.1.5", 47810, "OFIS"), b("192.168.1.191", 47810, "XAN-PC")];
        assert_eq!(pick(&two, 47810, "XAN-PC").unwrap().host, "192.168.1.191");
        assert!(pick(&two, 47810, "").is_none());
        assert!(pick(&two, 47810, "YOK").is_none());
    }
}
